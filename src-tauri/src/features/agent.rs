use crate::constants::{
    COMPLETION_KEYWORDS, INTENT_PHRASES, MAX_NUDGE_ATTEMPTS, SHORT_EXPLANATION_LIMIT, TOOL_NAME,
};
use crate::llm::router;
use crate::llm::{Message, StreamChunk};
use crate::tools::{self, ToolRegistry};
use serde::{Deserialize, Serialize};

// --- Types ---

#[derive(Serialize, Deserialize, Clone)]
pub struct ToolProposal {
    pub tool_name: String,
    pub command: String,
    pub tool_call_id: Option<String>,
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

        // Extract the human-readable command for the approval UI
        let display_command = call
            .function
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                // Fallback: show the full JSON arguments
                ""
            });

        if display_command.is_empty() {
            continue;
        }

        proposals.push(ToolProposal {
            tool_name: call.function.name.clone(),
            command: display_command.to_string(),
            tool_call_id: call.id.clone(),
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

fn should_nudge(content: &str, content_lower: &str) -> bool {
    let has_intent = INTENT_PHRASES.iter().any(|p| content_lower.contains(p));
    let is_short_explanation =
        content.len() < SHORT_EXPLANATION_LIMIT && !content_lower.contains("```");
    let not_complete = !is_success_summary(content_lower)
        && !content_lower.contains("error")
        && !content_lower.contains("fail");

    (has_intent || is_short_explanation) && not_complete
}

// --- Core logic: processes an LLM response into an AgentStepResult ---

/// Given an LLM response message, check for tool calls / fallbacks / nudge needs.
/// Returns `Some(result)` if we have a final answer, or `None` if we should nudge and retry.
fn process_response(
    response_msg: &Message,
    registry: &ToolRegistry,
    history: &mut Vec<Message>,
    current_context: &mut Vec<Message>,
    attempts: &mut usize,
) -> Option<AgentStepResult> {
    // Check for proper tool calls (all of them)
    let proposals = extract_tool_proposals(response_msg, registry);
    if !proposals.is_empty() {
        history.push(response_msg.clone());
        let response = if proposals.len() == 1 {
            AgentResponse::CommandProposal(proposals[0].command.clone())
        } else {
            AgentResponse::ToolProposals(proposals)
        };
        return Some(AgentStepResult {
            response,
            updated_history: history.clone(),
        });
    }

    // Fallback extraction from text
    let content_lower = response_msg.content.to_lowercase();
    if !is_success_summary(&content_lower) {
        if let Some(cmd) = extract_code_block(&response_msg.content) {
            history.push(response_msg.clone());
            return Some(AgentStepResult {
                response: AgentResponse::CommandProposal(cmd),
                updated_history: history.clone(),
            });
        }

        if let Some(cmd) = extract_raw_tool_call(&response_msg.content) {
            history.push(response_msg.clone());
            return Some(AgentStepResult {
                response: AgentResponse::CommandProposal(cmd),
                updated_history: history.clone(),
            });
        }
    }

    // Auto-nudge
    if should_nudge(&response_msg.content, &content_lower) {
        let nudge_count = current_context
            .iter()
            .filter(|m| {
                m.content.contains("EXECUTE NOW") || m.content.contains("WRITE THE CODE NOW")
            })
            .count();

        if nudge_count < MAX_NUDGE_ATTEMPTS - 1 {
            current_context.push(response_msg.clone());
            let nudge_msg = if nudge_count == 0 {
                "STOP. You MUST call the run_powershell tool NOW. Do not explain - EXECUTE NOW."
            } else {
                "You MUST output the actual command. If you cannot call the tool, write the command inside a ```powershell code block. Do NOT describe what you would do - WRITE THE CODE NOW."
            };
            current_context.push(Message {
                role: "system".to_string(),
                content: nudge_msg.to_string(),
                tool_calls: None,
            });
            *attempts += 1;
            return None; // Signal: retry
        }
    }

    // Text response (success or final)
    history.push(response_msg.clone());
    Some(AgentStepResult {
        response: AgentResponse::Text(response_msg.content.clone()),
        updated_history: history.clone(),
    })
}

// --- Public API ---

/// Non-streaming agent step.
pub async fn run_agent_step(
    model: String,
    history: Vec<Message>,
) -> Result<AgentStepResult, String> {
    let registry = tools::build_default_registry();
    let tool_defs = registry.definitions();

    let mut attempts = 0;
    let mut current_context = history.clone();
    let mut hist = history;
    let mut last_response: Option<Message> = None;

    while attempts < MAX_NUDGE_ATTEMPTS {
        let response_msg =
            router::route_chat(&model, current_context.clone(), Some(tool_defs.clone())).await?;

        last_response = Some(response_msg.clone());

        if let Some(result) =
            process_response(&response_msg, &registry, &mut hist, &mut current_context, &mut attempts)
        {
            return Ok(result);
        }
        // process_response returned None → nudge was applied, loop continues
    }

    // Exhausted retries
    if let Some(msg) = last_response {
        hist.push(msg.clone());
        Ok(AgentStepResult {
            response: AgentResponse::Text(msg.content),
            updated_history: hist,
        })
    } else {
        Ok(AgentStepResult {
            response: AgentResponse::Text(
                "Agent failed to respond correctly after retries.".to_string(),
            ),
            updated_history: hist,
        })
    }
}

/// Streaming agent step — sends text deltas and Done event via Tauri channel.
pub async fn run_agent_step_stream(
    model: String,
    history: Vec<Message>,
    channel: tauri::ipc::Channel<StreamChunk>,
) -> Result<AgentStepResult, String> {
    let registry = tools::build_default_registry();
    let tool_defs = registry.definitions();

    let mut attempts = 0;
    let mut current_context = history.clone();
    let mut hist = history;
    let mut last_response: Option<Message> = None;

    while attempts < MAX_NUDGE_ATTEMPTS {
        let ch = channel.clone();
        let on_chunk = move |text: String| {
            let _ = ch.send(StreamChunk::TextDelta { text });
        };

        let response_msg = match router::route_chat_stream(
            &model,
            current_context.clone(),
            Some(tool_defs.clone()),
            on_chunk,
        )
        .await
        {
            Ok(msg) => msg,
            Err(e) => {
                let _ = channel.send(StreamChunk::Error {
                    error: e.clone(),
                });
                return Err(e);
            }
        };

        last_response = Some(response_msg.clone());

        if let Some(result) =
            process_response(&response_msg, &registry, &mut hist, &mut current_context, &mut attempts)
        {
            let _ = channel.send(StreamChunk::Done {
                message: result.updated_history.last().unwrap().clone(),
            });
            return Ok(result);
        }
        // process_response returned None → nudge was applied, loop continues
    }

    // Exhausted retries
    if let Some(msg) = last_response {
        hist.push(msg.clone());
        let _ = channel.send(StreamChunk::Done {
            message: msg.clone(),
        });
        Ok(AgentStepResult {
            response: AgentResponse::Text(msg.content),
            updated_history: hist,
        })
    } else {
        let fallback_msg = Message {
            role: "assistant".to_string(),
            content: "Agent failed to respond correctly after retries.".to_string(),
            tool_calls: None,
        };
        let _ = channel.send(StreamChunk::Done {
            message: fallback_msg,
        });
        Ok(AgentStepResult {
            response: AgentResponse::Text(
                "Agent failed to respond correctly after retries.".to_string(),
            ),
            updated_history: hist,
        })
    }
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
            tool_calls: None,
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
            },
            ToolProposal {
                tool_name: "run_powershell".to_string(),
                command: "echo second".to_string(),
                tool_call_id: None,
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
    fn test_agent_step_result_serialization() {
        let result = AgentStepResult {
            response: AgentResponse::CommandProposal("dir".to_string()),
            updated_history: vec![
                Message {
                    role: "user".to_string(),
                    content: "list files".to_string(),
                    tool_calls: None,
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
                },
            ],
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
        };
        let mut history = vec![];
        let mut context = vec![];
        let mut attempts = 0;

        let result = process_response(&msg, &registry, &mut history, &mut context, &mut attempts);
        assert!(result.is_some());
        let r = result.unwrap();
        match r.response {
            AgentResponse::CommandProposal(cmd) => assert_eq!(cmd, "Get-ChildItem"),
            _ => panic!("Expected CommandProposal"),
        }
        assert_eq!(r.updated_history.len(), 1); // msg added to history
    }

    #[test]
    fn test_process_response_with_code_block() {
        let registry = tools::build_default_registry();
        let msg = Message {
            role: "assistant".to_string(),
            content: "Here's the command:\n```powershell\nGet-Process\n```".to_string(),
            tool_calls: None,
        };
        let mut history = vec![];
        let mut context = vec![];
        let mut attempts = 0;

        let result = process_response(&msg, &registry, &mut history, &mut context, &mut attempts);
        assert!(result.is_some());
        match result.unwrap().response {
            AgentResponse::CommandProposal(cmd) => assert_eq!(cmd, "Get-Process"),
            _ => panic!("Expected CommandProposal from code block"),
        }
    }

    #[test]
    fn test_process_response_success_text() {
        let registry = tools::build_default_registry();
        let msg = Message {
            role: "assistant".to_string(),
            content: "Task complete! All files have been processed successfully.".to_string(),
            tool_calls: None,
        };
        let mut history = vec![];
        let mut context = vec![];
        let mut attempts = 0;

        let result = process_response(&msg, &registry, &mut history, &mut context, &mut attempts);
        assert!(result.is_some());
        match result.unwrap().response {
            AgentResponse::Text(text) => assert!(text.contains("Task complete")),
            _ => panic!("Expected Text response for success summary"),
        }
    }

    #[test]
    fn test_process_response_nudge() {
        let registry = tools::build_default_registry();
        // Short explanation with intent phrase → should trigger nudge
        let msg = Message {
            role: "assistant".to_string(),
            content: "I'll process the files now.".to_string(),
            tool_calls: None,
        };
        let mut history = vec![];
        let mut context = vec![];
        let mut attempts = 0;

        let result = process_response(&msg, &registry, &mut history, &mut context, &mut attempts);
        // Should return None (nudge applied, retry needed)
        assert!(result.is_none());
        assert_eq!(attempts, 1);
        // Context should have the response + nudge message
        assert_eq!(context.len(), 2);
        assert!(context[1].content.contains("EXECUTE NOW"));
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
