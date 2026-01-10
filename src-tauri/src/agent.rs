use crate::llm::{chat, Message, ToolCall, ToolDefinition, ToolFunction};
use serde_json::json;
use std::process::Command;
use serde::{Deserialize, Serialize};

// --- Types ---

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "content")]
pub enum AgentResponse {
    Text(String),
    CommandProposal(String), // The proposed command
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
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            if !stderr.is_empty() {
                format!("STDOUT:\n{}\nSTDERR:\n{}", stdout, stderr)
            } else {
                stdout.to_string()
            }
        },
        Err(e) => format!("Failed to execute command: {}", e),
    }
}

pub fn get_initial_system_prompt(context_path: &str, selected_files: &Vec<String>) -> String {
    let file_count = selected_files.len();
    let files_list = if selected_files.is_empty() {
        "No specific files selected (operating on directory)".to_string()
    } else {
        selected_files.iter().enumerate()
            .map(|(i, f)| format!("{}. {}", i + 1, f))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "You are an intelligent system assistant with access to PowerShell. \
        
CONTEXT:
- Working directory: '{}'
- Selected files ({} total):
{}

CRITICAL INSTRUCTIONS:
1. If the user's request applies to MULTIPLE files, you MUST process ALL {} files, not just the first one.
2. Process files ONE BY ONE in sequence. After each file, immediately proceed to the next.
3. Do NOT stop after processing just one file. Keep going until ALL files are done.
4. Only provide a final summary AFTER all files have been processed.

MANDATORY ACTION RULE:
- When you decide to do something, you MUST IMMEDIATELY call the run_powershell tool in the SAME response.
- Do NOT just describe what you will do and stop. That is NOT allowed.
- WRONG: 'I'll process these files one by one.' (stops without action)
- CORRECT: 'Processing file 1...' + [calls run_powershell tool]

TOOL USAGE:
- Use the 'run_powershell' tool to execute commands.
- ALWAYS include the tool call in the same response as your explanation.
- PowerShell uses ';' as command separator - quote arguments containing semicolons.

FFMPEG TIPS:
- STDERR output is normal for FFmpeg - check for 'video:...kB audio:...' to confirm success.
- For grids: use 'hstack', 'vstack', or 'xstack' filters.

VERIFICATION:
- After each command, verify output with `Test-Path 'filename'` if creating files.
- If a command fails, diagnose and retry.

COMPLETION:
- Only stop when ALL {} files are processed or the task is fully complete.
- Provide a concise text summary (no code blocks) at the very end.",
        context_path, file_count, files_list, file_count, file_count
    )
}

// --- Agent Logic ---

pub async fn run_agent_step(model: String, mut history: Vec<Message>) -> Result<AgentStepResult, String> {
    
    // 1. Define Tools
    let tools = vec![
        ToolDefinition {
            r#type: "function".to_string(),
            function: ToolFunction {
                name: "run_powershell".to_string(),
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
        }
    ];

    // 2. Call LLM with current history
    // We assume history already contains System Prompt + User Prompt (handled by frontend or init wrapper)
    let response_msg = chat(&model, history.clone(), Some(tools.clone())).await?;
    history.push(response_msg.clone());

    // 3. Check for Tool Calls
    if let Some(calls) = response_msg.tool_calls {
        if !calls.is_empty() {
             let call = &calls[0];
             if call.function.name == "run_powershell" {
                 let args = &call.function.arguments;
                 if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                     return Ok(AgentStepResult {
                         response: AgentResponse::CommandProposal(cmd.to_string()),
                         updated_history: history,
                     });
                 }
             }
        }
    }

    // 4. Fallback: Check for Markdown Code Blocks
    // Only do this if the output DOES NOT contain success markers.
    // This prevents the agent's "summary explanation" with code blocks from being parsed as a new command.
    let content_lower = response_msg.content.to_lowercase();
    let is_success_summary = content_lower.contains("success") || content_lower.contains("completed");

    if !is_success_summary {
        let content = &response_msg.content;
        let code_block_pattern = "```";
        if let Some(start) = content.find(code_block_pattern) {
            // Find end of generic code block or specific language block
            let rest = &content[start + 3..];
            // Skip language identifier (e.g. "powershell\n")
            let code_start = if let Some(newline_idx) = rest.find('\n') {
                newline_idx + 1
            } else {
                0
            };
            
            let code_rest = &rest[code_start..];
            if let Some(end) = code_rest.find("```") {
                let command_candidate = &code_rest[..end].trim();
                // Filter out obviously "example" commands that contain placeholders like "input1.mp4"
                if !command_candidate.is_empty() && !command_candidate.contains("input1.mp4") {
                     return Ok(AgentStepResult {
                        response: AgentResponse::CommandProposal(command_candidate.to_string()),
                        updated_history: history,
                    });
                }
            }
        }
    }
    
    // No valid tool call or code block, so it's a text response
    Ok(AgentStepResult {
        response: AgentResponse::Text(response_msg.content),
        updated_history: history,
    })
}
