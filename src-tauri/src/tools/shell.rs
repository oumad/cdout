//! The single execution surface: run one shell command, capture its output,
//! hand it back to the model.
//!
//! Which shell that is — and therefore what the tool is called — comes from
//! [`crate::platform::SHELL`]: `run_powershell` on Windows, `run_shell` (zsh)
//! on macOS. Only one command runs at a time, tracked in `CURRENT_PROCESS` so
//! the UI can offer to kill a long ffmpeg job.

use super::{Tool, ToolResult};
use crate::constants::MAX_OUTPUT_LEN;
use crate::platform;
use serde::Serialize;
use serde_json::{json, Value};
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
        Some(rc) => platform::kill_process_tree(rc.pid),
        None => Err("No command is currently running".to_string()),
    }
}

pub struct ShellTool;

impl Tool for ShellTool {
    // Not a getter for `ShellSpec::name` (which is the *shell's* name, e.g.
    // "zsh"); this is the Tool trait's identifier, which is `tool_name`.
    #[allow(clippy::misnamed_getters)]
    fn name(&self) -> &str {
        platform::SHELL.tool_name
    }

    fn description(&self) -> &str {
        // Naming the shell in the description matters as much as the tool
        // name: it is what stops a model from reaching for PowerShell
        // cmdlets on macOS.
        #[cfg(target_os = "windows")]
        {
            "Execute a PowerShell command on the user's system."
        }
        #[cfg(not(target_os = "windows"))]
        {
            "Execute a shell command on the user's macOS system (zsh, POSIX syntax)."
        }
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": if cfg!(target_os = "windows") {
                        "The PowerShell command to execute"
                    } else {
                        "The shell command to execute (zsh / POSIX syntax)"
                    }
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

        run_shell(command, cwd)
    }
}

/// Cap a blob at `MAX_OUTPUT_LEN`, noting how much was hidden.
fn truncate(mut text: String) -> String {
    if text.len() > MAX_OUTPUT_LEN {
        let hidden = text.len() - MAX_OUTPUT_LEN;
        // Respect char boundaries — tool output is arbitrary bytes lossily
        // decoded, so a naive truncate can land mid-codepoint and panic.
        let mut cut = MAX_OUTPUT_LEN;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str(&format!(
            "\n... [Error truncated, hiding {} characters] ...",
            hidden
        ));
    }
    text
}

/// Markers that distinguish a real problem from routine stderr noise. Kept
/// deliberately narrow: a false positive costs context, and a tool that logs
/// the word "warning" on every run would poison every result.
const ERROR_MARKERS: &[&str] = &[
    "error",
    "invalid",
    "corrupt",
    "failed",
    "cannot",
    "no such file",
    "not permitted",
    "denied",
    "unsupported",
    "malformed",
    "truncated",
];

/// Pull just the error-shaped lines out of a successful command's stderr.
/// Returns `None` when nothing looks like an error.
fn error_lines(stderr: &str) -> Option<String> {
    let mut hits: Vec<&str> = Vec::new();
    for line in stderr.lines() {
        let lower = line.to_lowercase();
        if ERROR_MARKERS.iter().any(|m| lower.contains(m)) {
            hits.push(line);
        }
    }
    if hits.is_empty() {
        return None;
    }
    // Hundreds of near-identical decode errors say nothing more than a dozen
    // do, and the tail is where the fatal one usually is.
    const MAX_LINES: usize = 20;
    let omitted = hits.len().saturating_sub(MAX_LINES);
    let shown: Vec<&str> = hits.into_iter().rev().take(MAX_LINES).rev().collect();
    let mut out = shown.join("\n");
    if omitted > 0 {
        out = format!("[{} earlier similar line(s) omitted]\n{}", omitted, out);
    }
    Some(out)
}

/// Execute a command in the platform shell and return the result.
pub fn run_shell(command: &str, cwd: Option<&str>) -> ToolResult {
    let mut cmd = platform::shell_command(command);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    platform::prepare_group(&mut cmd);

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

            if !stderr.is_empty() {
                if !success {
                    result.push_str(&format!("\nSTDERR:\n{}", truncate(stderr)));
                } else if let Some(errors) = error_lines(&stderr) {
                    // Exit 0 does NOT mean nothing went wrong, and the system
                    // prompt explicitly tells the model so. Dropping stderr
                    // outright used to make that instruction impossible to
                    // follow: a partially corrupt input makes ffmpeg emit
                    // hundreds of "Invalid NAL unit size" lines, produce a
                    // damaged file, and still exit 0 — and the model saw an
                    // empty result. Only error-shaped lines are forwarded, so
                    // ffmpeg's normal banner/progress chatter (all of which is
                    // stderr too) still stays out of the context window.
                    result.push_str(&format!(
                        "\nSTDERR (command exited 0, but wrote errors — the output may be \
                         incomplete or corrupt; verify it before trusting it):\n{}",
                        truncate(errors)
                    ));
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A script that writes `err` to stderr and `out` to stdout, then exits
    /// with `code` — spelled for whichever shell this platform runs. The
    /// POSIX form (`echo x >&2`) is a parse error in PowerShell, so these
    /// end-to-end tests silently only covered macOS until CI ran them on
    /// Windows.
    fn script(err: &str, out: &str, code: i32) -> String {
        if cfg!(target_os = "windows") {
            format!("[Console]::Error.WriteLine('{err}'); Write-Output '{out}'; exit {code}")
        } else {
            format!("echo '{err}' >&2; echo '{out}'; exit {code}")
        }
    }

    #[test]
    fn error_lines_ignores_routine_chatter() {
        // ffmpeg writes its banner, stream table and progress to stderr on a
        // perfectly good run. None of it may reach the model.
        let noise = "ffmpeg version 7.1 Copyright (c) 2000-2024\n                       libavutil 59. 39.100\n                     Stream #0:0: Video: h264, yuv420p, 1920x1080\n                     frame=  120 fps=0.0 q=-1.0 size=     512kB time=00:00:04.00\n                     video:498kB audio:14kB muxing overhead: 0.9%";
        assert_eq!(error_lines(noise), None);
    }

    #[test]
    fn error_lines_catches_decode_errors_on_success() {
        // The real regression: this exact shape exits 0 and yields a damaged
        // file, and used to be invisible.
        let stderr = "[h264 @ 0x9e30] Invalid NAL unit size (679765253 > 569).\n                      [h264 @ 0x9e30] Error splitting the input into NAL units.\n                      frame=  100 fps=0.0 q=28.0 size=     256kB";
        let out = error_lines(stderr).expect("must surface decode errors");
        assert!(out.contains("Invalid NAL unit size"));
        assert!(out.contains("Error splitting"));
        // Progress lines are not errors and stay out.
        assert!(!out.contains("fps="));
    }

    #[test]
    fn error_lines_caps_repetitive_spam_keeping_the_tail() {
        // 200 identical decode errors carry no more information than 20, but
        // the LAST lines are where the fatal one lands.
        let mut stderr = String::new();
        for i in 0..200 {
            stderr.push_str(&format!("[h264] Invalid NAL unit size ({})\n", i));
        }
        stderr.push_str("[out#0] Error muxing a packet\n");
        let out = error_lines(&stderr).unwrap();
        assert!(out.contains("earlier similar line(s) omitted"));
        assert!(out.contains("Error muxing a packet"), "tail must survive");
        assert!(out.lines().count() <= 21);
    }

    #[test]
    fn error_lines_matches_case_insensitively() {
        assert!(error_lines("CANNOT OPEN FILE").is_some());
        assert!(error_lines("Permission denied").is_some());
        assert!(error_lines("all good here").is_none());
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        // Multi-byte input longer than the cap must not panic mid-codepoint.
        let text = "é".repeat(MAX_OUTPUT_LEN);
        let out = truncate(text);
        assert!(out.contains("Error truncated"));
    }

    #[test]
    fn truncate_leaves_short_text_alone() {
        assert_eq!(truncate("short".to_string()), "short");
    }

    #[test]
    fn successful_command_forwards_error_stderr() {
        // End-to-end through the real shell: exit 0 while writing an error.
        let r = run_shell(&script("Invalid NAL unit size", "done", 0), None);
        assert!(!r.is_error, "exit 0 must still be a success");
        assert!(r.output.contains("done"));
        assert!(
            r.output.contains("Invalid NAL unit size"),
            "error-shaped stderr must reach the model on exit 0: {}",
            r.output
        );
        assert!(r.output.contains("command exited 0, but wrote errors"));
    }

    #[test]
    fn successful_command_hides_harmless_stderr() {
        let r = run_shell(&script("frame= 120 fps=30", "ok", 0), None);
        assert!(!r.is_error);
        assert!(r.output.contains("ok"));
        assert!(
            !r.output.contains("STDERR"),
            "routine stderr must not be forwarded: {}",
            r.output
        );
    }

    #[test]
    fn failing_command_still_forwards_all_stderr() {
        // Unchanged behaviour on failure: the whole stderr, not just matches.
        let r = run_shell(&script("plain detail", "", 3), None);
        assert!(r.is_error);
        assert!(r.output.contains("[Exit code: 3 — Failed]"));
        assert!(r.output.contains("plain detail"));
    }
}
