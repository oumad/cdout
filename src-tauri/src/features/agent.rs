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
            let mut stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let mut stderr = String::from_utf8_lossy(&o.stderr).to_string();

            if stdout.len() > MAX_OUTPUT_LEN {
                let truncate_msg = format!(
                    "\n... [Output truncated, hiding {} characters] ...",
                    stdout.len() - MAX_OUTPUT_LEN
                );
                stdout.truncate(MAX_OUTPUT_LEN);
                stdout.push_str(&truncate_msg);
            }
            if stderr.len() > MAX_OUTPUT_LEN {
                let truncate_msg = format!(
                    "\n... [Error truncated, hiding {} characters] ...",
                    stderr.len() - MAX_OUTPUT_LEN
                );
                stderr.truncate(MAX_OUTPUT_LEN);
                stderr.push_str(&truncate_msg);
            }

            if !stderr.is_empty() {
                format!("STDOUT:\n{}\nSTDERR:\n{}", stdout, stderr)
            } else {
                stdout
            }
        }
        Err(e) => format!("Failed to execute command: {}", e),
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

        // Fallback: check for markdown code blocks
        let content_lower = response_msg.content.to_lowercase();
        if !is_success_summary(&content_lower) {
            if let Some(cmd) = extract_code_block(&response_msg.content) {
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

        // Fallback: check for markdown code blocks
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
