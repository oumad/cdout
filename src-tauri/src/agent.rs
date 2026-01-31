use crate::llm::{chat, Message, ToolDefinition, ToolFunction};
use serde::{Deserialize, Serialize};
use serde_json::json;
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
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            if !stderr.is_empty() {
                format!("STDOUT:\n{}\nSTDERR:\n{}", stdout, stderr)
            } else {
                stdout.to_string()
            }
        }
        Err(e) => format!("Failed to execute command: {}", e),
    }
}

pub fn get_initial_system_prompt(context_path: &str, selected_files: &Vec<String>) -> String {
    let file_count = selected_files.len();
    let files_list = if selected_files.is_empty() {
        "No specific files selected (operating on directory)".to_string()
    } else {
        selected_files
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{}. {}", i + 1, f))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "You are a PowerShell automation assistant. You have ONE tool: run_powershell.

CONTEXT:
- Working directory: '{}'
- Selected files ({} total):
{}

CRITICAL RULES - READ CAREFULLY:

1. YOU MUST CALL THE run_powershell TOOL. Do NOT just write code in your response text.
   - WRONG: Writing a code block in your message
   - CORRECT: Calling the run_powershell function with the command

2. Use ONLY PowerShell. Never suggest Python, batch files, or other languages.

3. After briefly stating your plan (1-2 sentences max), IMMEDIATELY call run_powershell.

4. Process ALL {} files in ONE script using a foreach loop.

5. WHEN ERRORS OCCUR - NEVER STOP:
   - If a command fails, briefly note the issue (1 sentence) then IMMEDIATELY call run_powershell with a FIXED version
   - Do NOT just explain the problem and stop
   - Do NOT ask for permission to fix - just fix it
   - Keep trying until the task succeeds or you've exhausted options
   - Example: \"The script failed because X. Fixing now.\" [IMMEDIATELY call run_powershell with corrected script]

EXAMPLE WORKFLOW:

User: \"reverse these videos\"
You: \"I'll reverse the video while keeping audio intact using ffmpeg.\"
[IMMEDIATELY call run_powershell with the script - do NOT write code as text]

SCRIPT TEMPLATE:
```
$files = @('full_path_1', 'full_path_2')
foreach ($file in $files) {{
    $output = $file -replace '\\.mp4$', '_processed.mp4'
    ffmpeg -i $file <filters> $output
    if (Test-Path $output) {{ Write-Host \"SUCCESS: $output\" }}
}}
```

REMEMBER: 
- Call the tool, don't just show code
- PowerShell only, no Python
- Brief plan then IMMEDIATE tool call
- If you encounter an error, FIX IT immediately - don't just explain and stop
- If you find yourself writing ```powershell in your response, STOP - you should be calling the tool instead",
        context_path, file_count, files_list, file_count
    )
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

    let response_msg = chat(&ollama_url, &model, history.clone(), Some(tools.clone())).await?;
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
    let is_success_summary =
        content_lower.contains("success") || content_lower.contains("completed");

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

    // 5. Auto-nudge: If the LLM said it WILL do something but didn't call the tool, retry with a nudge
    // Detect phrases that indicate intent without action
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

    // Also detect short responses that just describe the plan
    let is_short_explanation = response_msg.content.len() < 500 && !content_lower.contains("```");

    let not_complete =
        !is_success_summary && !content_lower.contains("error") && !content_lower.contains("fail");

    if (has_intent || is_short_explanation) && not_complete {
        // The LLM described what it will do but didn't actually call the tool
        // Add a nudge message and recursively retry (up to 2 times to avoid infinite loops)
        let nudge_count = history
            .iter()
            .filter(|m| m.content.contains("EXECUTE NOW"))
            .count();
        if nudge_count < 2 {
            history.push(Message {
                role: "system".to_string(),
                content:
                    "STOP. You MUST call the run_powershell tool NOW. Do not explain - EXECUTE NOW."
                        .to_string(),
                tool_calls: None,
            });

            // Recursive call with the nudge
            return Box::pin(run_agent_step(ollama_url, model, history)).await;
        }
    }

    // No valid tool call or code block, so it's a text response
    Ok(AgentStepResult {
        response: AgentResponse::Text(response_msg.content),
        updated_history: history,
    })
}
