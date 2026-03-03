use crate::constants::{
    COMPLETION_KEYWORDS, INTENT_PHRASES, MAX_NUDGE_ATTEMPTS, MAX_OUTPUT_LEN,
    SHORT_EXPLANATION_LIMIT, TOOL_NAME,
};
use crate::llm::router;
use crate::llm::{Message, StreamChunk, ToolDefinition, ToolFunction};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;

// --- Types ---

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "content")]
pub enum AgentResponse {
    Text(String),
    CommandProposal(String),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AgentStepResult {
    pub response: AgentResponse,
    pub updated_history: Vec<Message>,
}

// --- Tool Implementation: PowerShell ---

pub fn run_powershell_command(command: &str, cwd: Option<&str>) -> String {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", command]);

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd.output();

    match output {
        Ok(o) => {
            let exit_code = o.status.code().unwrap_or(-1);
            let success = o.status.success();
            let mut stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();

            if stdout.len() > MAX_OUTPUT_LEN {
                let truncate_msg = format!(
                    "\n... [Output truncated, hiding {} characters] ...",
                    stdout.len() - MAX_OUTPUT_LEN
                );
                stdout.truncate(MAX_OUTPUT_LEN);
                stdout.push_str(&truncate_msg);
            }

            let mut result = if success {
                format!("[Exit code: {} — Success]\n{}", exit_code, stdout)
            } else {
                format!("[Exit code: {} — Failed]\n{}", exit_code, stdout)
            };

            // Only include STDERR if it contains meaningful error info.
            // Many tools (ffmpeg, etc.) write version banners to STDERR on success.
            if !stderr.is_empty() && !success {
                let mut stderr_trimmed = stderr;
                if stderr_trimmed.len() > MAX_OUTPUT_LEN {
                    let truncate_msg = format!(
                        "\n... [Error truncated, hiding {} characters] ...",
                        stderr_trimmed.len() - MAX_OUTPUT_LEN
                    );
                    stderr_trimmed.truncate(MAX_OUTPUT_LEN);
                    stderr_trimmed.push_str(&truncate_msg);
                }
                result.push_str(&format!("\nSTDERR:\n{}", stderr_trimmed));
            }

            result
        }
        Err(e) => format!("[Failed to execute command]\n{}", e),
    }
}

// --- Agent Logic ---

fn build_tools() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        r#type: "function".to_string(),
        function: ToolFunction {
            name: TOOL_NAME.to_string(),
            description: "Execute a PowerShell command on the user's system.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The PowerShell command to execute"
                    }
                },
                "required": ["command"]
            }),
        },
    }]
}

fn extract_tool_command(msg: &Message) -> Option<String> {
    msg.tool_calls.as_ref().and_then(|calls| {
        calls.first().and_then(|call| {
            if call.function.name == TOOL_NAME {
                call.function
                    .arguments
                    .get("command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
    })
}

/// Fallback: extract command when the model outputs a raw tool call as text,
/// e.g. `run_powershell[ARGS]{"command": "..."}` or `run_powershell({"command": "..."})`
fn extract_raw_tool_call(content: &str) -> Option<String> {
    let idx = content.find(TOOL_NAME)?;
    let rest = &content[idx..];

    // Find the "command" key
    let cmd_key_idx = rest.find("\"command\"")?;
    let after_key = &rest[cmd_key_idx + "\"command\"".len()..];

    // Skip `: "` to reach the value
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_quote = after_colon.trim_start().strip_prefix('"')?;

    // The command value ends at the last `"}` which closes the JSON object
    let end_idx = after_quote.rfind("\"}")?;
    let raw = &after_quote[..end_idx];

    if raw.trim().is_empty() {
        return None;
    }

    // Unescape basic JSON string escapes
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
    COMPLETION_KEYWORDS.iter().any(|kw| content_lower.contains(kw))
}

fn should_nudge(content: &str, content_lower: &str) -> bool {
    let has_intent = INTENT_PHRASES.iter().any(|p| content_lower.contains(p));
    let is_short_explanation = content.len() < SHORT_EXPLANATION_LIMIT && !content_lower.contains("```");
    let not_complete = !is_success_summary(content_lower)
        && !content_lower.contains("error")
        && !content_lower.contains("fail");

    (has_intent || is_short_explanation) && not_complete
}

pub async fn run_agent_step(
    model: String,
    mut history: Vec<Message>,
) -> Result<AgentStepResult, String> {
    let tools = build_tools();

    let mut attempts = 0;
    let mut current_context = history.clone();
    let mut last_response: Option<Message> = None;

    while attempts < MAX_NUDGE_ATTEMPTS {
        let response_msg =
            router::route_chat(&model, current_context.clone(), Some(tools.clone())).await?;

        last_response = Some(response_msg.clone());

        // Check for tool calls
        if let Some(cmd) = extract_tool_command(&response_msg) {
            history.push(response_msg);
            return Ok(AgentStepResult {
                response: AgentResponse::CommandProposal(cmd),
                updated_history: history,
            });
        }

        // Fallback 1: check for markdown code blocks
        let content_lower = response_msg.content.to_lowercase();
        if !is_success_summary(&content_lower) {
            if let Some(cmd) = extract_code_block(&response_msg.content) {
                history.push(response_msg);
                return Ok(AgentStepResult {
                    response: AgentResponse::CommandProposal(cmd),
                    updated_history: history,
                });
            }

            // Fallback 2: raw tool call as text (model didn't use function calling)
            if let Some(cmd) = extract_raw_tool_call(&response_msg.content) {
                history.push(response_msg);
                return Ok(AgentStepResult {
                    response: AgentResponse::CommandProposal(cmd),
                    updated_history: history,
                });
            }
        }

        // Auto-nudge
        if should_nudge(&response_msg.content, &content_lower) {
            let nudge_count = current_context
                .iter()
                .filter(|m| m.content.contains("EXECUTE NOW"))
                .count();

            if nudge_count < MAX_NUDGE_ATTEMPTS - 1 {
                current_context.push(response_msg);
                current_context.push(Message {
                    role: "system".to_string(),
                    content: "STOP. You MUST call the run_powershell tool NOW. Do not explain - EXECUTE NOW.".to_string(),
                    tool_calls: None,
                });
                attempts += 1;
                continue;
            }
        }

        // Text response (success or final failure)
        history.push(response_msg.clone());
        return Ok(AgentStepResult {
            response: AgentResponse::Text(response_msg.content),
            updated_history: history,
        });
    }

    // Exhausted retries
    if let Some(msg) = last_response {
        history.push(msg.clone());
        Ok(AgentStepResult {
            response: AgentResponse::Text(msg.content),
            updated_history: history,
        })
    } else {
        Ok(AgentStepResult {
            response: AgentResponse::Text(
                "Agent failed to respond correctly after retries.".to_string(),
            ),
            updated_history: history,
        })
    }
}

pub async fn run_agent_step_stream(
    model: String,
    mut history: Vec<Message>,
    channel: tauri::ipc::Channel<StreamChunk>,
) -> Result<AgentStepResult, String> {
    let tools = build_tools();

    let mut attempts = 0;
    let mut current_context = history.clone();
    let mut last_response: Option<Message> = None;

    while attempts < MAX_NUDGE_ATTEMPTS {
        let ch = channel.clone();
        let on_chunk = move |text: String| {
            let _ = ch.send(StreamChunk::TextDelta { text });
        };

        let response_msg = router::route_chat_stream(
            &model,
            current_context.clone(),
            Some(tools.clone()),
            on_chunk,
        )
        .await?;

        last_response = Some(response_msg.clone());

        // Check for tool calls
        if let Some(cmd) = extract_tool_command(&response_msg) {
            history.push(response_msg);
            let result = AgentStepResult {
                response: AgentResponse::CommandProposal(cmd),
                updated_history: history,
            };
            let _ = channel.send(StreamChunk::Done {
                message: result.updated_history.last().unwrap().clone(),
            });
            return Ok(result);
        }

        // Fallback 1: check for markdown code blocks
        let content_lower = response_msg.content.to_lowercase();
        if !is_success_summary(&content_lower) {
            if let Some(cmd) = extract_code_block(&response_msg.content) {
                history.push(response_msg);
                let result = AgentStepResult {
                    response: AgentResponse::CommandProposal(cmd),
                    updated_history: history,
                };
                let _ = channel.send(StreamChunk::Done {
                    message: result.updated_history.last().unwrap().clone(),
                });
                return Ok(result);
            }

            // Fallback 2: raw tool call as text (model didn't use function calling)
            if let Some(cmd) = extract_raw_tool_call(&response_msg.content) {
                history.push(response_msg);
                let result = AgentStepResult {
                    response: AgentResponse::CommandProposal(cmd),
                    updated_history: history,
                };
                let _ = channel.send(StreamChunk::Done {
                    message: result.updated_history.last().unwrap().clone(),
                });
                return Ok(result);
            }
        }

        // Auto-nudge
        if should_nudge(&response_msg.content, &content_lower) {
            let nudge_count = current_context
                .iter()
                .filter(|m| m.content.contains("EXECUTE NOW"))
                .count();

            if nudge_count < MAX_NUDGE_ATTEMPTS - 1 {
                current_context.push(response_msg);
                current_context.push(Message {
                    role: "system".to_string(),
                    content: "STOP. You MUST call the run_powershell tool NOW. Do not explain - EXECUTE NOW.".to_string(),
                    tool_calls: None,
                });
                attempts += 1;
                continue;
            }
        }

        // Text response
        history.push(response_msg.clone());
        let result = AgentStepResult {
            response: AgentResponse::Text(response_msg.content.clone()),
            updated_history: history,
        };
        let _ = channel.send(StreamChunk::Done {
            message: response_msg,
        });
        return Ok(result);
    }

    // Exhausted retries
    if let Some(msg) = last_response {
        history.push(msg.clone());
        let _ = channel.send(StreamChunk::Done {
            message: msg.clone(),
        });
        Ok(AgentStepResult {
            response: AgentResponse::Text(msg.content),
            updated_history: history,
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
            updated_history: history,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_raw_tool_call_args_format() {
        let content = r#"run_powershell[ARGS]{"command": "$files = @('D:\\video\\a.mp4', 'D:\\video\\b.mp4')\nforeach ($file in $files) {\n  ffmpeg -i $file -vframes 1 $output\n}"}"#;
        let cmd = extract_raw_tool_call(content).unwrap();
        assert!(cmd.contains("$files = @("));
        assert!(cmd.contains("ffmpeg -i $file"));
        assert!(cmd.contains('\n')); // \n should be unescaped
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
