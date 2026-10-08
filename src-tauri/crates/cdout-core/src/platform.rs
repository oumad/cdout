//! Everything that differs between Windows and macOS lives here, so the rest
//! of the codebase can stay `cfg`-free.
//!
//! Three things vary per OS and all three leak into the LLM conversation, not
//! just into system calls:
//!
//! 1. **The shell.** `powershell -NoProfile -Command` vs `zsh -lc`. The tool
//!    is even named differently (`run_powershell` / `run_shell`) because the
//!    name is part of the prompt the model reads — calling a zsh tool
//!    "run_powershell" reliably makes models emit `Get-ChildItem`.
//! 2. **The file manager.** Explorer vs Finder, which changes user-facing
//!    labels as well as the introspection API (COM vs AppleScript).
//! 3. **Prompt phrasing.** `Test-Path` vs `test -f`, `$files = @(...)` vs
//!    `files=(...)`. Collected in [`ShellSpec`] so `prompts.rs` interpolates
//!    instead of branching.

use std::process::Command;

/// Stable OS key handed to the frontend so it can pick labels/shortcut glyphs.
pub const OS_KEY: &str = if cfg!(target_os = "windows") {
    "windows"
} else if cfg!(target_os = "macos") {
    "macos"
} else {
    "linux"
};

/// Display name of the OS, as it appears in the system prompt and the UI.
pub const OS_NAME: &str = if cfg!(target_os = "windows") {
    "Windows"
} else if cfg!(target_os = "macos") {
    "macOS"
} else {
    "Linux"
};

/// Name of the OS file manager, as users know it. Used in UI labels and in
/// the prompt's context block.
pub const FILE_MANAGER: &str = if cfg!(target_os = "macos") {
    "Finder"
} else {
    "Explorer"
};

/// Prompt- and UI-facing description of the shell we execute in. Every string
/// the model reads about *how* to write a command comes from here.
pub struct ShellSpec {
    /// Human name of the shell ("PowerShell", "zsh").
    pub name: &'static str,
    /// Tool name exposed to the LLM and stored in session history.
    pub tool_name: &'static str,
    /// Languages to steer away from, phrased for the rules block.
    pub other_languages: &'static str,
    /// One-liner that chains "produce a file" with "verify it exists".
    pub verify_example: &'static str,
    /// The existence test, named so the model can be told to use it.
    pub exists_check: &'static str,
    /// What a zero exit code does and does not prove in this shell.
    pub exit_code_note: &'static str,
    /// Directory-listing command, for the "diagnose then retry" rule.
    pub list_dir: &'static str,
    /// Script skeleton when nothing is selected (operate on the directory).
    pub template_dir: &'static str,
    /// Script skeleton for a short, inline file list.
    pub template_inline: &'static str,
    /// Script skeleton that reads the list from a temp file. `{LIST_FILE}` is
    /// substituted with the temp path.
    pub template_list_file: &'static str,
    /// Prose instruction for reading that temp file. `{LIST_FILE}` likewise.
    pub read_list_hint: &'static str,
}

#[cfg(target_os = "windows")]
pub const SHELL: ShellSpec = ShellSpec {
    name: "PowerShell",
    tool_name: "run_powershell",
    other_languages: "Never suggest Python, batch files, or other languages.",
    verify_example: "ffmpeg -i input.mp4 output.mp4; if (-not (Test-Path output.mp4)) { throw 'output.mp4 was not created' }",
    exists_check: "Test-Path",
    exit_code_note: "PowerShell `[Exit code: 0 — Success]` only means the LAST command in the pipeline returned success.",
    list_dir: "Get-ChildItem",
    template_dir: "$files = Get-ChildItem -Path . -Filter *.mp4\nforeach ($file in $files) { ... }",
    template_inline: "$files = @('path1', 'path2')\nforeach ($file in $files) { ... }",
    template_list_file: "$files = Get-Content '{LIST_FILE}'\nforeach ($file in $files) { ... }",
    read_list_hint: "Access the list using: $files = Get-Content '{LIST_FILE}'",
};

#[cfg(not(target_os = "windows"))]
pub const SHELL: ShellSpec = ShellSpec {
    name: "zsh",
    tool_name: "run_shell",
    other_languages: "Never suggest Python or AppleScript — use shell commands and the CLI tools already on the system.",
    verify_example: "ffmpeg -i input.mp4 output.mp4 && [ -f output.mp4 ] || { echo 'output.mp4 was not created' >&2; exit 1; }",
    exists_check: "test -f",
    exit_code_note: "A `[Exit code: 0 — Success]` only means the LAST command in the pipeline returned success.",
    list_dir: "ls -la",
    template_dir: "for f in *.mp4; do\n  [ -e \"$f\" ] || continue\n  ...\ndone",
    template_inline: "files=('path1' 'path2')\nfor f in \"${files[@]}\"; do ... done",
    template_list_file: "while IFS= read -r f; do\n  ...\ndone < '{LIST_FILE}'",
    read_list_hint: "Read the list with: while IFS= read -r f; do ... done < '{LIST_FILE}'",
};

/// Substitute the temp-file path into one of the `{LIST_FILE}` templates.
pub fn with_list_file(template: &str, path: &str) -> String {
    template.replace("{LIST_FILE}", path)
}

/// Build the command that runs `script` in the platform shell.
///
/// macOS uses a **login** shell (`-l`), unlike the Windows `-NoProfile`. A
/// bundled `.app` launched from Finder inherits only `/usr/bin:/bin:/usr/sbin:
/// /sbin`, so without sourcing the user's dotfiles nothing installed by
/// Homebrew (ffmpeg, exiftool, oiiotool — i.e. every bundled skill) would be
/// on PATH.
pub fn shell_command(script: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", script]);
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = Command::new(shell_path());
        cmd.args(["-l", "-c", script]);
        // Belt and braces for the PATH problem above: a user whose PATH is set
        // in fish/nushell config has nothing in ~/.zprofile for `-l` to read,
        // so seed the standard Homebrew prefixes explicitly.
        cmd.env("PATH", augmented_path());
        cmd
    }
}

/// Absolute path to the shell we execute in. Deliberately *not* `$SHELL`: the
/// system prompt promises the model POSIX/zsh syntax, and a user whose login
/// shell is fish or nushell would otherwise get syntax errors on every
/// `for f in ...` loop.
#[cfg(not(target_os = "windows"))]
pub fn shell_path() -> &'static str {
    if std::path::Path::new("/bin/zsh").exists() {
        "/bin/zsh"
    } else {
        "/bin/bash"
    }
}

/// Current PATH plus any missing standard CLI prefixes (Apple silicon and
/// Intel Homebrew, plus MacPorts).
#[cfg(not(target_os = "windows"))]
fn augmented_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<&str> = current.split(':').filter(|p| !p.is_empty()).collect();
    for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"] {
        if !parts.contains(&extra) {
            parts.push(extra);
        }
    }
    parts.join(":")
}

/// Spawn `cmd` such that [`kill_process_tree`] can later take down everything
/// it started, not just the shell itself.
///
/// The shell is usually a thin wrapper around a long-running child (ffmpeg is
/// the motivating case), so killing only the direct child would leave the real
/// work running. Windows solves this at kill time with `taskkill /T`; on Unix
/// the child has to be made a process-group leader up front so the whole group
/// can be signalled.
pub fn prepare_group(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = cmd;
    }
}

/// Kill a running command and every process it spawned.
pub fn kill_process_tree(pid: u32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // /F = force, /T = kill process tree (catches ffmpeg/etc. spawned by powershell)
        let output = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string(), "/T"])
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
    #[cfg(unix)]
    {
        // `prepare_group` made the child its own group leader, so its pid
        // doubles as the process-group id. SIGKILL rather than SIGTERM: this
        // is the user hitting "kill", and ffmpeg ignores a single SIGTERM
        // while flushing.
        // SAFETY: `killpg` is a plain signal syscall; it cannot violate memory
        // safety. A stale pid returns ESRCH, which we surface as an error.
        let rc = unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL) };
        if rc != 0 {
            return Err(format!(
                "Failed to kill process group {}: {}",
                pid,
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}

/// Whether `name` is an executable on PATH.
///
/// Runs through the login shell for the same reason [`shell_command`] does: a
/// bare `Command::new("which")` would search the `.app`'s stunted PATH and
/// report every Homebrew tool as missing, quarantining all bundled skills.
pub fn binary_exists(name: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        Command::new("where.exe")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        // `command -v` is the POSIX builtin; unlike `which` it exists even on
        // a stripped-down system and needs no PATH lookup of its own.
        shell_command(&format!("command -v {}", shell_quote(name)))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Single-quote a string for POSIX shells.
#[cfg(not(target_os = "windows"))]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_key_is_known() {
        assert!(["windows", "macos", "linux"].contains(&OS_KEY));
    }

    #[test]
    fn shell_spec_templates_carry_the_list_file_placeholder() {
        // A missing placeholder would silently produce a script that reads
        // from a literal "{LIST_FILE}" path.
        assert!(SHELL.template_list_file.contains("{LIST_FILE}"));
        assert!(SHELL.read_list_hint.contains("{LIST_FILE}"));
    }

    #[test]
    fn with_list_file_substitutes_every_occurrence() {
        let out = with_list_file("a {LIST_FILE} b {LIST_FILE}", "/tmp/x.txt");
        assert_eq!(out, "a /tmp/x.txt b /tmp/x.txt");
        assert!(!out.contains("{LIST_FILE}"));
    }

    #[test]
    fn tool_name_matches_the_shell() {
        // The tool name is prompt surface, not just an identifier — see the
        // module docs.
        if cfg!(target_os = "windows") {
            assert_eq!(SHELL.tool_name, "run_powershell");
        } else {
            assert_eq!(SHELL.tool_name, "run_shell");
        }
    }

    #[test]
    fn binary_exists_agrees_with_reality() {
        let present = if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "ls"
        };
        assert!(binary_exists(present));
        assert!(!binary_exists("cdout_definitely_not_a_real_binary"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn augmented_path_includes_homebrew() {
        let p = augmented_path();
        assert!(p.contains("/opt/homebrew/bin"));
        assert!(p.contains("/usr/local/bin"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_quote_escapes_embedded_quotes() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }
}
