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
        "You are an intelligent PowerShell automation assistant.

CONTEXT:
- Working directory: '{}'
- Selected files ({} total):
{}

WORKFLOW - Follow these steps IN ORDER:

STEP 1 - PLAN (text only, no tool call):
Before executing anything, briefly state:
- What transformation/operation you'll perform
- How you'll handle all {} files (loop strategy)
- Output naming convention

STEP 2 - EXECUTE (call run_powershell tool):
Write a COMPLETE PowerShell script that processes ALL files in ONE execution.
Use foreach loops for batch operations. Example structure:

```powershell
$files = @(
    'file1.mp4',
    'file2.mp4'
)
foreach ($file in $files) {{
    $output = $file -replace '\\.mp4$', '_processed.mp4'
    ffmpeg -i $file <filters> $output
    if (Test-Path $output) {{ Write-Host \"SUCCESS: $output\" }}
}}
```

SCRIPT REQUIREMENTS:
- Process ALL {} files in a single script using foreach
- Include the full file paths in a $files array
- Use proper output naming to avoid overwrites
- Add success checks with Test-Path and Write-Host
- Quote paths with spaces properly

STEP 3 - VERIFY & REPORT:
After execution, provide a brief summary of what was processed.

FFMPEG NOTES:
- STDERR output is normal - look for 'video:...kB' to confirm success
- Common filters: hflip, vflip, scale, hue, hstack, vstack

IMPORTANT:
- Do NOT process files one-by-one with separate tool calls
- Write ONE comprehensive script that handles everything
- If the script fails, diagnose and provide a corrected version",
        context_path, file_count, files_list, file_count, file_count
    )
}

// --- Agent Logic ---

pub async fn run_agent_step(
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

    // No valid tool call or code block, so it's a text response
    Ok(AgentStepResult {
        response: AgentResponse::Text(response_msg.content),
        updated_history: history,
    })
}
