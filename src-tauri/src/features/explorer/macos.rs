//! Finder introspection via AppleScript.
//!
//! The Windows side of this module walks the window Z-order and talks COM to
//! find the topmost Explorer tab. Finder needs none of that: it maintains its
//! own window ordering, and `front Finder window` plus `selection` answer both
//! questions directly — including when Finder is not the frontmost app, which
//! matches the Windows behaviour of reaching past our own spotlight window.
//!
//! The catch is permissions. Sending Apple events to Finder is gated by TCC
//! (System Settings → Privacy & Security → Automation). The first call raises
//! a consent dialog; a denial is permanent until the user flips it back, so
//! `-1743` is translated into an instruction rather than an opaque error.

use super::ExplorerState;
use serde::Serialize;
use std::process::Command;

/// Record separator. Newline would be ambiguous — macOS filenames may legally
/// contain one.
const SEP: char = '\u{1e}';

/// Emits: front window's target path, then one entry per selected item.
/// Both halves are wrapped in `try` so a Finder window whose target has no
/// filesystem path (search results, Trash, a network share that just dropped)
/// still yields the selection, and vice versa.
const CONTEXT_SCRIPT: &str = r#"
set sep to character id 30
if application "Finder" is not running then return ""
tell application "Finder"
    set thePath to ""
    try
        if (count of Finder windows) > 0 then
            set thePath to POSIX path of (target of front Finder window as alias)
        end if
    end try
    set out to thePath
    try
        repeat with anItem in (get selection)
            set out to out & sep & (POSIX path of (anItem as alias))
        end repeat
    end try
    return out
end tell
"#;

const DEBUG_SCRIPT: &str = r#"
set sep to character id 30
set frontApp to ""
try
    set frontApp to (path to frontmost application as text)
end try
if application "Finder" is not running then return "false" & sep & frontApp & sep & "0" & sep & ""
tell application "Finder"
    set winCount to 0
    try
        set winCount to (count of Finder windows)
    end try
    set target to ""
    try
        if winCount > 0 then
            set target to POSIX path of (target of front Finder window as alias)
        end if
    end try
    return "true" & sep & frontApp & sep & (winCount as text) & sep & target
end tell
"#;

#[derive(Serialize, Debug)]
pub struct ExplorerDebugInfo {
    pub os: String,
    /// False when Finder is not running at all (rare, but possible).
    pub finder_running: bool,
    /// Frontmost application, as an HFS path. Empty if unavailable.
    pub frontmost_app: String,
    pub finder_window_count: i32,
    /// Target of the front Finder window, before trailing-slash normalisation.
    pub front_window_target: String,
    /// Whether Apple events to Finder are permitted. `false` here is the
    /// single most likely reason detection returns nothing on macOS.
    pub automation_authorized: bool,
    pub resolved_path: String,
    pub selected_count: usize,
    pub selection: Vec<String>,
    /// Raw stderr from the last failed `osascript`, if any.
    pub script_error: String,
}

/// Run an AppleScript and return its stdout, trimmed of the trailing newline
/// `osascript` always appends.
fn run_osascript(script: &str) -> Result<String, String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("Failed to run osascript: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(translate_script_error(&stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end_matches('\n')
        .to_string())
}

/// Turn AppleScript's error text into something the user can act on. The TCC
/// denial is the only one worth special-casing — it is not a bug, it is a
/// checkbox.
fn translate_script_error(stderr: &str) -> String {
    if is_permission_error(stderr) {
        return format!(
            "cdout is not allowed to control Finder. Open System Settings → \
             Privacy & Security → Automation, expand \"cdout\", and enable \
             \"Finder\". (osascript: {})",
            stderr
        );
    }
    format!("Finder query failed: {}", stderr)
}

fn is_permission_error(stderr: &str) -> bool {
    stderr.contains("-1743") || stderr.contains("Not authorized to send Apple events")
}

/// POSIX paths for directories come back with a trailing slash; strip it so a
/// folder and a file are formatted alike (and so the string matches what the
/// shell tool receives as `cwd`). The filesystem root keeps its slash.
fn normalize(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        if path.is_empty() {
            String::new()
        } else {
            "/".to_string()
        }
    } else {
        trimmed.to_string()
    }
}

fn parse_context(raw: &str) -> ExplorerState {
    let mut parts = raw.split(SEP);
    let path = parts.next().map(normalize).unwrap_or_default();
    let mut selected_files: Vec<String> = parts
        .filter(|s| !s.is_empty())
        .map(normalize)
        .filter(|s| !s.is_empty())
        .collect();

    // Case-INSENSITIVE natural order, to match how Explorer and Finder both
    // display a folder. With case-sensitive ordering, `SCENE_03.MOV` sorts
    // before `scene_01.mov`, so the numbered list handed to the model is in a
    // different order than the one the user is looking at — which is how a
    // model ends up asking "in what order did you want these?" or, worse,
    // silently assembling a reel backwards.
    selected_files.sort_by(|a, b| natord::compare_ignore_case(a, b));

    // Desktop selections have no Finder window behind them, so there is no
    // window target to report. The containing folder of what's selected is
    // the useful working directory in that case.
    let path = if path.is_empty() {
        selected_files
            .first()
            .and_then(|f| std::path::Path::new(f).parent())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        path
    };

    ExplorerState {
        path,
        selected_files,
    }
}

pub fn get_active_explorer_info() -> Result<ExplorerState, String> {
    let raw = run_osascript(CONTEXT_SCRIPT)?;
    Ok(parse_context(&raw))
}

pub fn get_explorer_debug_info() -> Result<ExplorerDebugInfo, String> {
    let mut info = ExplorerDebugInfo {
        os: "macos".to_string(),
        finder_running: false,
        frontmost_app: String::new(),
        finder_window_count: 0,
        front_window_target: String::new(),
        automation_authorized: true,
        resolved_path: String::new(),
        selected_count: 0,
        selection: Vec::new(),
        script_error: String::new(),
    };

    // Unlike the happy path, debug never propagates the error — reporting
    // "automation_authorized: false" *is* the diagnosis.
    match run_osascript(DEBUG_SCRIPT) {
        Ok(raw) => {
            let parts: Vec<&str> = raw.split(SEP).collect();
            info.finder_running = parts.first().map(|s| *s == "true").unwrap_or(false);
            info.frontmost_app = parts.get(1).unwrap_or(&"").to_string();
            info.finder_window_count = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            info.front_window_target = parts.get(3).unwrap_or(&"").to_string();
        }
        Err(e) => {
            info.automation_authorized = !e.contains("not allowed to control Finder");
            info.script_error = e;
        }
    }

    match get_active_explorer_info() {
        Ok(state) => {
            info.resolved_path = state.path;
            info.selected_count = state.selected_files.len();
            info.selection = state.selected_files;
        }
        Err(e) => {
            if info.script_error.is_empty() {
                info.script_error = e;
            }
        }
    }

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_folder_trailing_slash() {
        assert_eq!(normalize("/Users/me/Pictures/"), "/Users/me/Pictures");
        assert_eq!(normalize("/Users/me/a.mp4"), "/Users/me/a.mp4");
    }

    #[test]
    fn normalize_keeps_root_and_empty() {
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn parse_context_reads_path_and_selection() {
        let raw = format!(
            "/Users/me/Pictures/{sep}/Users/me/Pictures/b.jpg{sep}/Users/me/Pictures/a.jpg",
            sep = SEP
        );
        let state = parse_context(&raw);
        assert_eq!(state.path, "/Users/me/Pictures");
        // Natural sort, matching the Windows implementation.
        assert_eq!(
            state.selected_files,
            vec!["/Users/me/Pictures/a.jpg", "/Users/me/Pictures/b.jpg"]
        );
    }

    #[test]
    fn parse_context_sorts_case_insensitively_like_finder() {
        // Finder lists these as scene_01, scene 02, SCENE_03; a case-sensitive
        // sort would put SCENE_03 first and mislead the model about order.
        let raw = format!(
            "/p/{sep}/p/SCENE_03.MOV{sep}/p/scene_01.mov{sep}/p/scene 02 final.mov",
            sep = SEP
        );
        let state = parse_context(&raw);
        assert_eq!(
            state.selected_files,
            vec![
                "/p/scene 02 final.mov",
                "/p/scene_01.mov",
                "/p/SCENE_03.MOV"
            ]
        );
    }

    #[test]
    fn parse_context_sorts_naturally_not_lexically() {
        let raw = format!("/p/{sep}/p/img10.png{sep}/p/img9.png", sep = SEP);
        let state = parse_context(&raw);
        assert_eq!(state.selected_files, vec!["/p/img9.png", "/p/img10.png"]);
    }

    #[test]
    fn parse_context_handles_no_finder_window() {
        let state = parse_context("");
        assert_eq!(state.path, "");
        assert!(state.selected_files.is_empty());
    }

    #[test]
    fn parse_context_handles_selection_without_window() {
        // Desktop selection: empty target, items present.
        let raw = format!("{sep}/Users/me/Desktop/shot.png", sep = SEP);
        let state = parse_context(&raw);
        assert_eq!(state.path, "/Users/me/Desktop");
        assert_eq!(state.selected_files, vec!["/Users/me/Desktop/shot.png"]);
    }

    #[test]
    fn parse_context_handles_window_without_selection() {
        let state = parse_context("/Users/me/Movies/");
        assert_eq!(state.path, "/Users/me/Movies");
        assert!(state.selected_files.is_empty());
    }

    #[test]
    fn parse_context_keeps_filenames_containing_newlines() {
        // The whole reason the protocol is not newline-delimited.
        let raw = format!("/p{sep}/p/we\nird.mp4", sep = SEP);
        let state = parse_context(&raw);
        assert_eq!(state.selected_files, vec!["/p/we\nird.mp4"]);
    }

    #[test]
    fn permission_errors_are_recognised_and_explained() {
        let stderr = "execution error: Not authorized to send Apple events to Finder. (-1743)";
        assert!(is_permission_error(stderr));
        let msg = translate_script_error(stderr);
        assert!(msg.contains("Privacy & Security"));

        let other = "execution error: Finder got an error: Can't get window 1. (-1728)";
        assert!(!is_permission_error(other));
        assert!(translate_script_error(other).contains("-1728"));
    }
}
