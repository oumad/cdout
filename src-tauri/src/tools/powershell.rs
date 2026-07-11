use super::{Tool, ToolResult};
use crate::constants::MAX_OUTPUT_LEN;
use serde::Serialize;
use serde_json::{json, Value};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Serialize)]
pub struct RunningCommand {
    pub pid: u32,
    pub command_preview: String,
}

static CURRENT_PROCESS: OnceLock<Mutex<Option<RunningCommand>>> = OnceLock::new();

fn current_process() -> &'static Mutex<Option<RunningCommand>> {
    CURRENT_PROCESS.get_or_init(|| Mutex::new(None))
}

fn set_running(pid: u32, command: &str) {
    let preview = if command.chars().count() > 200 {
        let truncated: String = command.chars().take(200).collect();
        format!("{}…", truncated)
    } else {
        command.to_string()
    };
    *current_process().lock().unwrap() = Some(RunningCommand {
        pid,
        command_preview: preview,
    });
}

fn clear_running() {
    *current_process().lock().unwrap() = None;
}

pub fn get_running() -> Option<RunningCommand> {
    current_process().lock().unwrap().clone()
}

pub fn kill_running() -> Result<(), String> {
    let cmd = current_process().lock().unwrap().clone();
    match cmd {
        Some(rc) => {
            // /F = force, /T = kill process tree (catches ffmpeg/etc. spawned by powershell)
            let output = Command::new("taskkill")
                .args(["/F", "/PID", &rc.pid.to_string(), "/T"])
                .output()
                .map_err(|e| format!("Failed to spawn taskkill: {}", e))?;
            if !output.status.success() {
                return Err(format!(
                    "taskkill failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            Ok(())
        }
        None => Err("No command is currently running".to_string()),
    }
}

pub struct PowerShellTool;

impl Tool for PowerShellTool {
    fn name(&self) -> &str {
        "run_powershell"
    }

    fn description(&self) -> &str {
        "Execute a PowerShell command on the user's system."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The PowerShell command to execute"
                }
            },
            "required": ["command"]
        })
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn validate_input(&self, arguments: &Value) -> Result<(), String> {
        let command = arguments
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'command' parameter — expected a string")?;

        if command.trim().is_empty() {
            return Err("Command cannot be empty".to_string());
        }
        Ok(())
    }

    fn execute(&self, arguments: &Value, cwd: Option<&str>) -> ToolResult {
        let command = match arguments.get("command").and_then(|v| v.as_str()) {
            Some(cmd) => cmd,
            None => {
                return ToolResult {
                    output: "Missing 'command' parameter".to_string(),
                    is_error: true,
                }
            }
        };

        run_powershell(command, cwd)
    }
}

/// Execute a PowerShell command and return the result.
pub fn run_powershell(command: &str, cwd: Option<&str>) -> ToolResult {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", command])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return ToolResult {
                output: format!("[Failed to spawn command]\n{}", e),
                is_error: true,
            };
        }
    };

    let pid = child.id();
    set_running(pid, command);
    let output = child.wait_with_output();
    clear_running();

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

            ToolResult {
                output: result,
                is_error: !success,
            }
        }
        Err(e) => ToolResult {
            output: format!("[Failed to execute command]\n{}", e),
            is_error: true,
        },
    }
}
