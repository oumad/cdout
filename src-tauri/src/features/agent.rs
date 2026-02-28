use crate::llm::clients::ollama::chat;
use crate::llm::{Message, ToolDefinition, ToolFunction};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::fs::File;
use std::io::Write;
use std::process::Command;

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
            let mut stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let mut stderr = String::from_utf8_lossy(&o.stderr).to_string();

            // Truncation Logic
            const MAX_OUTPUT_LEN: usize = 2000;
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

pub fn get_initial_system_prompt(
    context_path: &str,
    selected_files: &Vec<String>,
) -> Result<String, String> {
    let file_count = selected_files.len();
    const MAX_FILES_TO_LIST: usize = 20;

    let (files_section, rules_section, script_template) = if selected_files.is_empty() {
        (
            "Selected files: None (operating on directory)".to_string(),
            "4. Process all relevant files in the directory.".to_string(),
            "$files = Get-ChildItem -Path . -Filter *.mp4\nforeach ($file in $files) { ... }"
                .to_string(),
        )
    } else if file_count > MAX_FILES_TO_LIST {
        // Create temp file
        let temp_path = env::temp_dir().join("shuttle_agent_context_files.txt");
        let mut file =
            File::create(&temp_path).map_err(|e| format!("Failed to create temp file: {}", e))?;

        for f in selected_files {
            writeln!(file, "{}", f).map_err(|e| format!("Failed to write to temp file: {}", e))?;
        }
        let temp_path_str = temp_path.to_string_lossy().replace("\\", "/");

        (
            format!("Selected files ({} total):\n[LIST TRUNCATED]\nThe full list of files has been written to: '{}'", file_count, temp_path_str),
            format!("4. READ THE FILE LIST:\n   - The user selected {} files, which is too many to list here.\n   - Access the list using: $files = Get-Content '{}'\n   - Do NOT try to guess the files. READ THE TEXT FILE.", file_count, temp_path_str),
            format!("$files = Get-Content '{}'\nforeach ($file in $files) {{ ... }}", temp_path_str)
        )
    } else {
        let list = selected_files
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{}. {}", i + 1, f))
            .collect::<Vec<_>>()
            .join("\n");

        (
            format!("Selected files ({} total):\n{}", file_count, list),
            format!(
                "4. Process ALL {} files in ONE script using a foreach loop.",
                file_count
            ),
            "$files = @('path1', 'path2')\nforeach ($file in $files) { ... }".to_string(),
        )
    };

    Ok(format!(
        "You are a PowerShell automation assistant. You have ONE tool: run_powershell.

CONTEXT:
- Working directory: '{}'
- {}

CRITICAL RULES - READ CAREFULLY:

1. YOU MUST CALL THE run_powershell TOOL. Do NOT just write code in your response text.
   - WRONG: Writing a code block in your message
   - CORRECT: Calling the run_powershell function with the command

2. Use ONLY PowerShell. Never suggest Python, batch files, or other languages.

3. After briefly stating your plan (1-2 sentences max), IMMEDIATELY call run_powershell.

{}

5. When the ENTIRE task is finished, you MUST say \"Task Complete\".

6. WHEN ERRORS OCCUR - NEVER STOP:
   - If a command fails, briefly note the issue (1 sentence) then IMMEDIATELY call run_powershell with a FIXED version
   - Do NOT just explain the problem and stop
   - Do NOT ask for permission to fix - just fix it
   - Keep trying until the task succeeds or you've exhausted options

7. SAFETY - DATA PROTECTION:
   - Do NOT delete original files unless the user EXPLICITLY asks to \"delete\", \"remove\", or \"replace\" them.
   - Always create NEW output files with a suffix (e.g. \"_processed\", \"_new\"). 
   - Never overwrite the input file directly.

EXAMPLE WORKFLOW:

User: \"reverse these videos\"
You: \"I'll reverse the video while keeping audio intact using ffmpeg.\"
[IMMEDIATELY call run_powershell with the script]

SCRIPT TEMPLATE:
```
{}
```

REMEMBER: 
- Call the tool, don't just show code
- PowerShell only, no Python
- Brief plan then IMMEDIATE tool call",
        context_path, files_section, rules_section, script_template
    ))
}

// --- Agent Logic ---

pub async fn run_agent_step(
    ollama_url: String,
    model: String,
    mut history: Vec<Message>,
) -> Result<AgentStepResult, String> {
    // 1. Define Tools
    let tools = vec![ToolDefinition {
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
    }];

    // Iterative Nudge Loop (max 3 attempts)
    let mut attempts = 0;
    // We use a temporary history for the retry loop so we don't pollute the main history with failed attempts
    let mut current_context = history.clone();
    let mut last_response: Option<Message> = None;

    while attempts < 3 {
        let response_msg = if model.starts_with("antigravity") {
            crate::llm::clients::antigravity::chat_stream(
                "gemini-3-flash",
                current_context.clone(),
                Some(tools.clone()),
                |_| {},
            )
            .await?
        } else if model.starts_with("openai:") {
            let keys = crate::utils::config::load_api_keys();
            let key = keys
                .openai
                .ok_or("LLM Error: No OpenAI API Key set. Please configure it in Settings.")?;
            let model_name = model.strip_prefix("openai:").unwrap_or("gpt-4");
            crate::llm::clients::openai::chat_openai(model_name, current_context.clone(), &key)
                .await?
        } else if model.starts_with("gemini:") {
            let keys = crate::utils::config::load_api_keys();
            let key = keys
                .gemini
                .ok_or("LLM Error: No Gemini API Key set. Please configure it in Settings.")?;
            let model_name = model.strip_prefix("gemini:").unwrap_or("gemini-1.5-pro");
            crate::llm::clients::gemini::chat_gemini(model_name, current_context.clone(), &key)
                .await?
        } else {
            chat(
                &ollama_url,
                &model,
                current_context.clone(),
                Some(tools.clone()),
            )
            .await?
        };

        last_response = Some(response_msg.clone());

        // 3. Check for Tool Calls (Extract command first to avoid borrow/move conflict)
        let tool_command = if let Some(calls) = &response_msg.tool_calls {
            if let Some(call) = calls.first() {
                if call.function.name == "run_powershell" {
                    call.function
                        .arguments
                        .get("command")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        if let Some(cmd) = tool_command {
            // Success! Update the REAL history and return
            history.push(response_msg);
            return Ok(AgentStepResult {
                response: AgentResponse::CommandProposal(cmd),
                updated_history: history,
            });
        }

        // 4. Fallback: Check for Markdown Code Blocks
        let content_lower = response_msg.content.to_lowercase();
        let is_success_summary = content_lower.contains("success")
            || content_lower.contains("completed")
            || content_lower.contains("task complete");

        if !is_success_summary {
            // Clone content to avoid borrowing response_msg
            let content = response_msg.content.clone();
            let code_block_pattern = "```";
            if let Some(start) = content.find(code_block_pattern) {
                let rest = &content[start + 3..];
                let code_start = if let Some(newline_idx) = rest.find('\n') {
                    newline_idx + 1
                } else {
                    0
                };

                let code_rest = &rest[code_start..];
                if let Some(end) = code_rest.find("```") {
                    let command_candidate = &code_rest[..end].trim();
                    if !command_candidate.is_empty() && !command_candidate.contains("input1.mp4") {
                        // Success! Update the REAL history and return
                        let cmd_string = command_candidate.to_string();
                        history.push(response_msg);
                        return Ok(AgentStepResult {
                            response: AgentResponse::CommandProposal(cmd_string),
                            updated_history: history,
                        });
                    }
                }
            }
        }

        // 5. Auto-nudge
        let intent_phrases = [
            "i'll",
            "i will",
            "let me",
            "i'm going to",
            "i am going to",
            "which reduces",
            "which will",
            "this will",
            "to remove",
            "to crop",
            "to process",
            "to convert",
            "here's",
            "here is",
            "the command",
        ];
        let has_intent = intent_phrases.iter().any(|p| content_lower.contains(p));
        let is_short_explanation =
            response_msg.content.len() < 500 && !content_lower.contains("```");
        let not_complete = !is_success_summary
            && !content_lower.contains("error")
            && !content_lower.contains("fail");

        if (has_intent || is_short_explanation) && not_complete {
            let nudge_count = current_context
                .iter()
                .filter(|m| m.content.contains("EXECUTE NOW"))
                .count();

            if nudge_count < 2 {
                // Add to temporary context ONLY
                current_context.push(response_msg);
                current_context.push(Message {
                    role: "system".to_string(),
                    content: "STOP. You MUST call the run_powershell tool NOW. Do not explain - EXECUTE NOW.".to_string(),
                    tool_calls: None,
                });
                attempts += 1;
                continue; // Retry loop
            }
        }

        // No valid tool call or code block, so it's a text response (success or final failure)
        history.push(response_msg.clone());
        return Ok(AgentStepResult {
            response: AgentResponse::Text(response_msg.content),
            updated_history: history,
        });
    }

    // If we exit loop, we failed to get a good response. Use the last one.
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
