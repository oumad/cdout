use crate::constants::{ASK_USER_QUESTION_TOOL, COMPLETION_KEYWORDS, TOOL_NAME};
use crate::features::loop_detector::{self, LoopVerdict};
use crate::llm::history;
use crate::llm::retry::{classify, ErrorClass};
use crate::llm::router;
use crate::llm::{Message, StreamChunk};
use crate::tools::{self, ToolRegistry};
use serde::{Deserialize, Serialize};

/// How many times to attempt history recovery (trim + retry) before bailing.
const MAX_CONTEXT_RECOVERY_ATTEMPTS: usize = 2;

// --- Types ---

#[derive(Serialize, Deserialize, Clone)]
pub struct QuestionOption {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct QuestionData {
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multi_select: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ToolProposal {
    pub tool_name: String,
    /// For run_powershell: the PowerShell script. For ask_user_question: the question text.
    pub command: String,
    pub tool_call_id: Option<String>,
    /// Present only for ask_user_question — drives the multiple-choice UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question_data: Option<QuestionData>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "content")]
pub enum AgentResponse {
    Text(String),
    /// Single command proposal (backward compatible with frontend)
    CommandProposal(String),
    /// Multiple tool call proposals for a single LLM turn
    ToolProposals(Vec<ToolProposal>),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AgentStepResult {
    pub response: AgentResponse,
    pub updated_history: Vec<Message>,
    /// Loop-detector signal attached when a tool proposal is the response.
    /// Frontend should pause auto-execute when verdict.should_pause_auto_execute().
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_verdict: Option<LoopVerdict>,
}

// --- Extraction helpers ---

/// Extract proposals from ALL tool calls in a message.
/// Uses the registry for validation and to check if the tool needs approval.
/// Only returns proposals for tools that require approval — auto-executable tools
/// would be executed inline (none exist yet, but the infrastructure is ready).
fn extract_tool_proposals(msg: &Message, registry: &ToolRegistry) -> Vec<ToolProposal> {
    let tool_calls = match &msg.tool_calls {
        Some(calls) if !calls.is_empty() => calls,
        _ => return Vec::new(),
    };

    let mut proposals = Vec::new();
    for call in tool_calls {
        let tool = match registry.get(&call.function.name) {
            Some(t) => t,
            None => continue, // Unknown tool, skip
        };

        // Validate input before proposing
        if let Err(e) = tool.validate_input(&call.function.arguments) {
            eprintln!(
                "Tool input validation failed for {}: {}",
                call.function.name, e
            );
            continue;
        }

        if !tool.requires_approval() {
            // Future: auto-execute safe tools inline and feed result back.
            // For now, all registered tools require approval, so this branch is unused.
            continue;
        }

        if call.function.name == ASK_USER_QUESTION_TOOL {
            // ask_user_question carries structured options instead of a command string.
            let qd: QuestionData =
                match serde_json::from_value(call.function.arguments.clone()) {
                    Ok(q) => q,
                    Err(e) => {
                        eprintln!("Failed to parse ask_user_question arguments: {}", e);
                        continue;
                    }
                };
            proposals.push(ToolProposal {
                tool_name: call.function.name.clone(),
                command: qd.question.clone(),
                tool_call_id: call.id.clone(),
                question_data: Some(qd),
            });
            continue;
        }

        // Extract the human-readable command for the approval UI
        let display_command = call
            .function
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if display_command.is_empty() {
            continue;
        }

        proposals.push(ToolProposal {
            tool_name: call.function.name.clone(),
            command: display_command.to_string(),
            tool_call_id: call.id.clone(),
            question_data: None,
        });
    }
    proposals
}

/// Fallback: extract command when the model outputs a raw tool call as text.
fn extract_raw_tool_call(content: &str) -> Option<String> {
    let idx = content.find(TOOL_NAME)?;
    let rest = &content[idx..];

    let cmd_key_idx = rest.find("\"command\"")?;
    let after_key = &rest[cmd_key_idx + "\"command\"".len()..];

    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_quote = after_colon.trim_start().strip_prefix('"')?;

    let end_idx = after_quote.rfind("\"}")?;
    let raw = &after_quote[..end_idx];

    if raw.trim().is_empty() {
        return None;
    }

    let unescaped = raw
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\\"", "\"")
        .replace("\\\\", "\\");

    Some(unescaped)
}

fn extract_code_block(content: &str) -> Option<String> {
    let code_block_pattern = "```";
    let start = content.find(code_block_pattern)?;
    let rest = &content[start + 3..];
    let code_start = rest.find('\n').map(|i| i + 1).unwrap_or(0);
    let code_rest = &rest[code_start..];
    let end = code_rest.find("```")?;
    let candidate = code_rest[..end].trim();

    if !candidate.is_empty() && !candidate.contains("input1.mp4") {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn is_success_summary(content_lower: &str) -> bool {
    COMPLETION_KEYWORDS
        .iter()
        .any(|kw| content_lower.contains(kw))
}

/// Pick the proposal we should surface to the user when the model emitted
/// multiple in a single turn. ask_user_question always wins — if the model is
/// asking for clarification, that pauses the loop regardless of any sibling
/// run_powershell proposal.
fn select_proposal(proposals: &[ToolProposal]) -> Option<&ToolProposal> {
    proposals
        .iter()
        .find(|p| p.tool_name == ASK_USER_QUESTION_TOOL)
        .or_else(|| proposals.first())
}

/// Record the selected proposal in the global LoopDetector and return its
/// verdict. ask_user_question is excluded — clarification breaks loops by
/// definition, recording it would poison the no-progress detector.
fn detect_loop(proposal: &ToolProposal) -> Option<LoopVerdict> {
    if proposal.tool_name == ASK_USER_QUESTION_TOOL {
        return None;
    }
    // Fingerprint over the actual tool arguments, not just `command`, so that
    // future tools with non-string args still fingerprint stably.
    let args = serde_json::json!({ "command": proposal.command });
    Some(loop_detector::record_proposal_global(
        &proposal.tool_name,
        &args,
    ))
}

// --- Core logic: processes an LLM response into an AgentStepResult ---

/// Given an LLM response message, dispatch to the right response shape.
/// Always returns Some — the legacy nudge loop is gone (LoopDetector replaces
/// the text-pattern heuristic that misfired on natural assistant prose).
fn process_response(
    response_msg: &Message,
    registry: &ToolRegistry,
    history: &mut Vec<Message>,
) -> AgentStepResult {
    // 1) Native tool_calls path (Anthropic, OpenAI, Ollama).
    let proposals = extract_tool_proposals(response_msg, registry);
    if !proposals.is_empty() {
        history.push(response_msg.clone());
        let verdict = select_proposal(&proposals).and_then(detect_loop);
        // Collapse to the compact CommandProposal shape ONLY for a lone
        // run_powershell. ask_user_question carries `question_data` that the
        // CommandProposal variant can't hold — collapsing it would drop the
        // multiple-choice payload and the frontend would render it as a
        // PowerShell command. So any question (or multiple proposals) goes
        // through ToolProposals which preserves question_data.
        let is_lone_powershell =
            proposals.len() == 1 && proposals[0].tool_name == TOOL_NAME;
        let response = if is_lone_powershell {
            AgentResponse::CommandProposal(proposals[0].command.clone())
        } else {
            AgentResponse::ToolProposals(proposals)
        };
        return AgentStepResult {
            response,
            updated_history: history.clone(),
            loop_verdict: verdict,
        };
    }

    // 2) Fallback extraction from text (for models that don't reliably emit
    //    native tool_calls — small Ollama models, mid-stream interruptions).
    //    Skip if the message reads like a completion summary, to avoid false
    //    extraction from prose like "I successfully ran echo X."
    let content_lower = response_msg.content.to_lowercase();
    if !is_success_summary(&content_lower) {
        if let Some(cmd) =
            extract_code_block(&response_msg.content).or_else(|| extract_raw_tool_call(&response_msg.content))
        {
            history.push(response_msg.clone());
            let synthetic = ToolProposal {
                tool_name: TOOL_NAME.to_string(),
                command: cmd.clone(),
                tool_call_id: None,
                question_data: None,
            };
            let verdict = detect_loop(&synthetic);
            return AgentStepResult {
                response: AgentResponse::CommandProposal(cmd),
                updated_history: history.clone(),
                loop_verdict: verdict,
            };
        }
    }

    // 3) Plain text response (success summary, refusal, or clarification text).
    history.push(response_msg.clone());
    AgentStepResult {
        response: AgentResponse::Text(response_msg.content.clone()),
        updated_history: history.clone(),
        loop_verdict: None,
    }
}

// --- Context-recovery wrapper ---

/// Wrap a streaming call with in-loop context-window recovery. On
/// `ContextWindowExceeded`, runs the trim ladder and retries up to
/// `MAX_CONTEXT_RECOVERY_ATTEMPTS` times. (Transient network errors are NOT
/// retried for streaming — partial chunks may already have been emitted.)
async fn call_stream_with_context_recovery(
    model: &str,
    current_context: &mut Vec<Message>,
    tool_defs: &[crate::llm::ToolDefinition],
    channel: &tauri::ipc::Channel<StreamChunk>,
) -> Result<Message, String> {
    let mut recovery_attempts = 0;
    loop {
        let ch = channel.clone();
        let on_chunk = move |text: String| {
            let _ = ch.send(StreamChunk::TextDelta { text });
        };
        match router::route_chat_stream(
            model,
            current_context.clone(),
            Some(tool_defs.to_vec()),
            on_chunk,
        )
        .await
        {
            Ok(msg) => return Ok(msg),
            Err(e) => {
                if recovery_attempts < MAX_CONTEXT_RECOVERY_ATTEMPTS
                    && classify(&e) == ErrorClass::ContextWindowExceeded
                {
                    let (trimmed, dropped, orphans) =
                        history::recover_from_context_overflow(current_context);
                    eprintln!(
                        "[agent stream] Context overflow recovery: trimmed {} tool bodies, dropped {} messages, reconciled {} orphans (attempt {}/{})",
                        trimmed,
                        dropped,
                        orphans,
                        recovery_attempts + 1,
                        MAX_CONTEXT_RECOVERY_ATTEMPTS
                    );
                    recovery_attempts += 1;
                    continue;
                }
                return Err(e);
            }
        }
    }
}

// --- Public API ---

/// Streaming agent step — sends text deltas and Done event via Tauri channel.
///
/// `drop_tools` is set by the frontend when the previous turn's loop_verdict
/// was `Break`. We honour it by sending an empty tool-defs list, forcing the
/// model into a text-only reassessment.
pub async fn run_agent_step_stream(
    model: String,
    history: Vec<Message>,
    drop_tools: bool,
    channel: tauri::ipc::Channel<StreamChunk>,
) -> Result<AgentStepResult, String> {
    let registry = tools::build_default_registry();
    let tool_defs = if drop_tools {
        Vec::new()
    } else {
        registry.definitions()
    };
    let mut current_context = history;

    let response_msg =
        match call_stream_with_context_recovery(&model, &mut current_context, &tool_defs, &channel)
            .await
        {
            Ok(msg) => msg,
            Err(e) => {
                let _ = channel.send(StreamChunk::Error { error: e.clone() });
                return Err(e);
            }
        };

    let result = process_response(&response_msg, &registry, &mut current_context);
    if let Some(last) = result.updated_history.last() {
        let _ = channel.send(StreamChunk::Done {
            message: last.clone(),
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{FunctionCall, ToolCall};
    use serde_json::json;

    // --- extract_tool_proposals tests ---

    fn make_msg_with_tool_calls(calls: Vec<ToolCall>) -> Message {
        Message {
            role: "assistant".to_string(),
            content: String::new(),
            tool_calls: Some(calls),
            ..Default::default()
        }
    }

    #[test]
    fn test_extract_tool_proposals_single_valid() {
        let registry = tools::build_default_registry();
        let msg = make_msg_with_tool_calls(vec![ToolCall {
            id: Some("toolu_abc123".to_string()),
            function: FunctionCall {
                name: "run_powershell".to_string(),
                arguments: json!({ "command": "Get-ChildItem" }),
            },
        }]);
        let proposals = extract_tool_proposals(&msg, &registry);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].command, "Get-ChildItem");
        assert_eq!(proposals[0].tool_call_id, Some("toolu_abc123".to_string()));
        assert_eq!(proposals[0].tool_name, "run_powershell");
    }

    #[test]
    fn test_extract_tool_proposals_multiple() {
        let registry = tools::build_default_registry();
        let msg = make_msg_with_tool_calls(vec![
            ToolCall {
                id: Some("toolu_1".to_string()),
                function: FunctionCall {
                    name: "run_powershell".to_string(),
                    arguments: json!({ "command": "echo first" }),
                },
            },
            ToolCall {
                id: Some("toolu_2".to_string()),
                function: FunctionCall {
                    name: "run_powershell".to_string(),
                    arguments: json!({ "command": "echo second" }),
                },
            },
        ]);
        let proposals = extract_tool_proposals(&msg, &registry);
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0].command, "echo first");
        assert_eq!(proposals[1].command, "echo second");
    }

    #[test]
    fn test_extract_tool_proposals_skips_invalid_input() {
        let registry = tools::build_default_registry();
        let msg = make_msg_with_tool_calls(vec![
            ToolCall {
                id: None,
                function: FunctionCall {
                    name: "run_powershell".to_string(),
                    arguments: json!({ "command": "" }), // empty = invalid
                },
            },
            ToolCall {
                id: None,
                function: FunctionCall {
                    name: "run_powershell".to_string(),
                    arguments: json!({ "command": "echo valid" }),
                },
            },
        ]);
        let proposals = extract_tool_proposals(&msg, &registry);
        // The empty-command call is skipped due to validation failure
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].command, "echo valid");
    }

    #[test]
    fn test_extract_tool_proposals_no_tool_calls() {
        let registry = tools::build_default_registry();
        let msg = Message {
            role: "assistant".to_string(),
            content: "Just a text response".to_string(),
            ..Default::default()
        };
        let proposals = extract_tool_proposals(&msg, &registry);
        assert!(proposals.is_empty());
    }

    #[test]
    fn test_extract_tool_proposals_unknown_tool_still_skipped() {
        let registry = tools::build_default_registry();
        let msg = make_msg_with_tool_calls(vec![ToolCall {
            id: None,
            function: FunctionCall {
                name: "unknown_tool".to_string(),
                arguments: json!({ "command": "something" }),
            },
        }]);
        let proposals = extract_tool_proposals(&msg, &registry);
        assert!(proposals.is_empty()); // unknown tool name != TOOL_NAME
    }

    #[test]
    fn test_extract_tool_proposals_missing_command_field() {
        let registry = tools::build_default_registry();
        let msg = make_msg_with_tool_calls(vec![ToolCall {
            id: None,
            function: FunctionCall {
                name: "run_powershell".to_string(),
                arguments: json!({ "wrong_key": "value" }),
            },
        }]);
        let proposals = extract_tool_proposals(&msg, &registry);
        // Missing "command" key → validation fails → skipped
        assert!(proposals.is_empty());
    }

    // --- Serialization tests (Rust ↔ Frontend contract) ---

    #[test]
    fn test_agent_response_command_proposal_serialization() {
        let response = AgentResponse::CommandProposal("Get-ChildItem".to_string());
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["type"], "CommandProposal");
        assert_eq!(json["content"], "Get-ChildItem");
    }

    #[test]
    fn test_agent_response_text_serialization() {
        let response = AgentResponse::Text("Task Complete".to_string());
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["type"], "Text");
        assert_eq!(json["content"], "Task Complete");
    }

    #[test]
    fn test_agent_response_tool_proposals_serialization() {
        let response = AgentResponse::ToolProposals(vec![
            ToolProposal {
                tool_name: "run_powershell".to_string(),
                command: "echo first".to_string(),
                tool_call_id: Some("toolu_1".to_string()),
                question_data: None,
            },
            ToolProposal {
                tool_name: "run_powershell".to_string(),
                command: "echo second".to_string(),
                tool_call_id: None,
                question_data: None,
            },
        ]);
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["type"], "ToolProposals");
        let content = json["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["command"], "echo first");
        assert_eq!(content[0]["tool_call_id"], "toolu_1");
        assert_eq!(content[1]["command"], "echo second");
        assert!(content[1]["tool_call_id"].is_null());
    }

    #[test]
    fn test_extract_tool_proposals_ask_user_question() {
        let registry = tools::build_default_registry();
        let msg = make_msg_with_tool_calls(vec![ToolCall {
            id: Some("toolu_q1".to_string()),
            function: FunctionCall {
                name: "ask_user_question".to_string(),
                arguments: json!({
                    "question": "Which format do you want?",
                    "header": "Format",
                    "options": [
                        { "label": "mp4", "description": "H.264, broadly compatible" },
                        { "label": "mov", "description": "ProRes-friendly" }
                    ]
                }),
            },
        }]);
        let proposals = extract_tool_proposals(&msg, &registry);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].tool_name, "ask_user_question");
        assert_eq!(proposals[0].command, "Which format do you want?");
        let qd = proposals[0].question_data.as_ref().unwrap();
        assert_eq!(qd.question, "Which format do you want?");
        assert_eq!(qd.header.as_deref(), Some("Format"));
        assert_eq!(qd.options.len(), 2);
        assert_eq!(qd.options[0].label, "mp4");
        assert!(!qd.multi_select);
    }

    #[test]
    fn test_agent_step_result_serialization() {
        let result = AgentStepResult {
            response: AgentResponse::CommandProposal("dir".to_string()),
            updated_history: vec![
                Message {
                    role: "user".to_string(),
                    content: "list files".to_string(),
                    ..Default::default()
                },
                Message {
                    role: "assistant".to_string(),
                    content: "".to_string(),
                    tool_calls: Some(vec![ToolCall {
                        id: Some("toolu_abc".to_string()),
                        function: FunctionCall {
                            name: "run_powershell".to_string(),
                            arguments: json!({ "command": "dir" }),
                        },
                    }]),
                    ..Default::default()
                },
            ],
            loop_verdict: None,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["response"]["type"], "CommandProposal");
        assert_eq!(json["response"]["content"], "dir");
        assert_eq!(json["updated_history"].as_array().unwrap().len(), 2);
        // Verify tool_calls serialize with id
        let tc = &json["updated_history"][1]["tool_calls"][0];
        assert_eq!(tc["id"], "toolu_abc");
        assert_eq!(tc["function"]["name"], "run_powershell");
    }

    #[test]
    fn test_tool_call_id_serialization_none() {
        let tc = ToolCall {
            id: None,
            function: FunctionCall {
                name: "run_powershell".to_string(),
                arguments: json!({ "command": "test" }),
            },
        };
        let json = serde_json::to_value(&tc).unwrap();
        // id should be present but null (default serialization)
        // The frontend ToolCall has `id?: string` so null is fine
        assert!(json.get("id").is_some());
    }

    #[test]
    fn test_tool_call_id_deserialization_missing() {
        // Frontend might send tool calls without an id field
        let json_str = r#"{"function":{"name":"run_powershell","arguments":{"command":"test"}}}"#;
        let tc: ToolCall = serde_json::from_str(json_str).unwrap();
        assert!(tc.id.is_none());
        assert_eq!(tc.function.name, "run_powershell");
    }

    // --- process_response tests ---

    #[test]
    fn test_process_response_with_tool_call() {
        let _guard = loop_detector::test_lock();
        loop_detector::reset_global();
        let registry = tools::build_default_registry();
        let msg = Message {
            role: "assistant".to_string(),
            content: "I'll list the files.".to_string(),
            tool_calls: Some(vec![ToolCall {
                id: Some("toolu_1".to_string()),
                function: FunctionCall {
                    name: "run_powershell".to_string(),
                    arguments: json!({ "command": "Get-ChildItem" }),
                },
            }]),
            ..Default::default()
        };
        let mut history = vec![];
        let r = process_response(&msg, &registry, &mut history);
        match r.response {
            AgentResponse::CommandProposal(cmd) => assert_eq!(cmd, "Get-ChildItem"),
            _ => panic!("Expected CommandProposal"),
        }
        assert_eq!(r.updated_history.len(), 1);
        // First proposal — should not trip loop detection.
        assert!(r.loop_verdict.as_ref().map(|v| v.is_ok()).unwrap_or(true));
    }

    #[test]
    fn test_process_response_lone_question_returns_tool_proposals() {
        // Regression: a lone ask_user_question must NOT collapse to
        // CommandProposal (which drops question_data and makes the frontend
        // render it as PowerShell). It must return ToolProposals so the
        // multiple-choice payload survives.
        let _guard = loop_detector::test_lock();
        loop_detector::reset_global();
        let registry = tools::build_default_registry();
        let msg = Message {
            role: "assistant".to_string(),
            content: "Which format?".to_string(),
            tool_calls: Some(vec![ToolCall {
                id: Some("toolu_q".to_string()),
                function: FunctionCall {
                    name: "ask_user_question".to_string(),
                    arguments: json!({
                        "question": "Which format?",
                        "options": [{ "label": "mp4" }, { "label": "mov" }]
                    }),
                },
            }]),
            ..Default::default()
        };
        let mut history = vec![];
        let r = process_response(&msg, &registry, &mut history);
        match r.response {
            AgentResponse::ToolProposals(props) => {
                assert_eq!(props.len(), 1);
                assert_eq!(props[0].tool_name, "ask_user_question");
                assert!(
                    props[0].question_data.is_some(),
                    "question_data must survive"
                );
            }
            other => panic!(
                "Expected ToolProposals for a lone question, got {:?}",
                serde_json::to_value(&other).unwrap()
            ),
        }
    }

    #[test]
    fn test_process_response_with_code_block() {
        let _guard = loop_detector::test_lock();
        loop_detector::reset_global();
        let registry = tools::build_default_registry();
        let msg = Message {
            role: "assistant".to_string(),
            content: "Here's the command:\n```powershell\nGet-Process\n```".to_string(),
            ..Default::default()
        };
        let mut history = vec![];
        let r = process_response(&msg, &registry, &mut history);
        match r.response {
            AgentResponse::CommandProposal(cmd) => assert_eq!(cmd, "Get-Process"),
            _ => panic!("Expected CommandProposal from code block"),
        }
    }

    #[test]
    fn test_process_response_success_text() {
        let _guard = loop_detector::test_lock();
        loop_detector::reset_global();
        let registry = tools::build_default_registry();
        let msg = Message {
            role: "assistant".to_string(),
            content: "Task complete! All files have been processed successfully.".to_string(),
            ..Default::default()
        };
        let mut history = vec![];
        let r = process_response(&msg, &registry, &mut history);
        match r.response {
            AgentResponse::Text(text) => assert!(text.contains("Task complete")),
            _ => panic!("Expected Text response for success summary"),
        }
        assert!(r.loop_verdict.is_none());
    }

    #[test]
    fn test_process_response_text_no_extraction() {
        // Natural assistant prose without a tool call should now just return
        // Text instead of misfiring the old nudge heuristic. This is the
        // behavior change driven by deleting INTENT_PHRASES / should_nudge.
        let _guard = loop_detector::test_lock();
        loop_detector::reset_global();
        let registry = tools::build_default_registry();
        let msg = Message {
            role: "assistant".to_string(),
            content: "I'll process the files now.".to_string(),
            ..Default::default()
        };
        let mut history = vec![];
        let r = process_response(&msg, &registry, &mut history);
        match r.response {
            AgentResponse::Text(_) => {}
            other => panic!("Expected Text, got {:?}", serde_json::to_value(&other).unwrap()),
        }
    }

    #[test]
    fn test_process_response_loop_verdict_escalates_on_repeat() {
        let _guard = loop_detector::test_lock();
        loop_detector::reset_global();
        let registry = tools::build_default_registry();
        let make_msg = |cmd: &str| Message {
            role: "assistant".to_string(),
            content: String::new(),
            synthetic: false,
            tool_calls: Some(vec![ToolCall {
                id: Some("toolu_1".to_string()),
                function: FunctionCall {
                    name: "run_powershell".to_string(),
                    arguments: json!({ "command": cmd }),
                },
            }]),
        };
        let mut history = vec![];
        let _ = process_response(&make_msg("echo same"), &registry, &mut history);
        let _ = process_response(&make_msg("echo same"), &registry, &mut history);
        let r3 = process_response(&make_msg("echo same"), &registry, &mut history);
        assert!(
            r3.loop_verdict.is_some(),
            "expected verdict on 3rd identical call"
        );
        assert!(!r3.loop_verdict.unwrap().is_ok());
    }

    // --- Existing extraction tests ---

    #[test]
    fn test_extract_raw_tool_call_args_format() {
        let content = r#"run_powershell[ARGS]{"command": "$files = @('D:\\video\\a.mp4', 'D:\\video\\b.mp4')\nforeach ($file in $files) {\n  ffmpeg -i $file -vframes 1 $output\n}"}"#;
        let cmd = extract_raw_tool_call(content).unwrap();
        assert!(cmd.contains("$files = @("));
        assert!(cmd.contains("ffmpeg -i $file"));
        assert!(cmd.contains('\n'));
    }

    #[test]
    fn test_extract_raw_tool_call_parens_format() {
        let content = r#"run_powershell({"command": "Get-ChildItem -Path ."})"#;
        let cmd = extract_raw_tool_call(content).unwrap();
        assert_eq!(cmd, "Get-ChildItem -Path .");
    }

    #[test]
    fn test_extract_raw_tool_call_with_prefix_text() {
        let content = r#"I'll extract the frames now. run_powershell[ARGS]{"command": "ffmpeg -i input.mp4 -vframes 1 out.png"}"#;
        let cmd = extract_raw_tool_call(content).unwrap();
        assert_eq!(cmd, "ffmpeg -i input.mp4 -vframes 1 out.png");
    }

    #[test]
    fn test_extract_raw_tool_call_no_match() {
        assert!(extract_raw_tool_call("Just some text").is_none());
        assert!(extract_raw_tool_call("run_powershell without json").is_none());
    }

    #[test]
    fn test_extract_raw_tool_call_escaped_quotes() {
        let content = r#"run_powershell[ARGS]{"command": "echo \"hello world\""}"#;
        let cmd = extract_raw_tool_call(content).unwrap();
        assert_eq!(cmd, r#"echo "hello world""#);
    }

    #[test]
    fn test_extract_code_block_powershell() {
        let content = "Here's the command:\n```powershell\nGet-ChildItem -Path .\n```";
        let cmd = extract_code_block(content).unwrap();
        assert_eq!(cmd, "Get-ChildItem -Path .");
    }

    #[test]
    fn test_extract_code_block_no_match() {
        assert!(extract_code_block("Just text, no code block").is_none());
    }
}
