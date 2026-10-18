//! Pure text rules shared by the agent loop and the UI.

/// Mirrors `COMPLETION_KEYWORDS` in `src/constants.ts`.
pub const COMPLETION_KEYWORDS: &[&str] = &[
    "task complete",
    "all done",
    "successfully processed all",
    "completed successfully",
    "successfully",
    "has been",
    "have been",
    "done",
];
pub const COMPLETION_SHORT_KEYWORDS: &[&str] = &["complete", "finished"];
pub const COMPLETION_SHORT_MAX_LEN: usize = 200;

/// Port of `isTaskComplete` (`src/utils/agent.ts`). The length check counts
/// UTF-16 units in JavaScript; chars are the closest Rust equivalent.
pub fn is_task_complete(content: &str) -> bool {
    let lower = content.to_lowercase();
    COMPLETION_KEYWORDS.iter().any(|kw| lower.contains(kw))
        || COMPLETION_SHORT_KEYWORDS
            .iter()
            .any(|kw| lower.contains(kw) && lower.chars().count() < COMPLETION_SHORT_MAX_LEN)
}

/// The shell tool appends `[Exit code: N — Failed]` to a failed command's
/// output; this matches `/\[Exit code:\s*-?\d+\s*—\s*Failed\]/`.
pub fn output_reports_failure(output: &str) -> bool {
    let mut rest = output;
    while let Some(i) = rest.find("[Exit code:") {
        let after = &rest[i + "[Exit code:".len()..];
        if matches_failure_tail(after) {
            return true;
        }
        rest = &rest[i + 1..];
    }
    false
}

fn matches_failure_tail(s: &str) -> bool {
    let s = s.trim_start();
    let s = s.strip_prefix('-').unwrap_or(s);
    let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return false;
    }
    let s = s[digits..].trim_start();
    let Some(s) = s.strip_prefix('—') else {
        return false;
    };
    s.trim_start().starts_with("Failed]")
}

/// The "files have changed" note, inline-list variant.
pub fn context_update_inline(path: &str, files: &[String]) -> String {
    let list = if files.is_empty() {
        "No specific files selected".to_string()
    } else {
        files
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{}. {}", i + 1, f))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "[CONTEXT UPDATE - Files have changed]\n\nWorking directory: {path}\nSelected files ({} total):\n{list}\n\nPlease use these updated files for any subsequent operations.",
        files.len()
    )
}

/// The "files have changed" note, written-to-file variant for large selections.
pub fn context_update_file(path: &str, count: usize, list_path: &str, read_hint: &str) -> String {
    format!(
        "[CONTEXT UPDATE - Files have changed]\n\nWorking directory: {path}\nSelected files: {count} files (too many to list inline)\n\nThe complete file list has been written to: {list_path}\n\n{read_hint}\n\nPlease use these updated files for any subsequent operations."
    )
}

pub fn loop_detected_note(reason: &str) -> String {
    format!("[Loop detected] {reason} — stop, reassess, and either ask a clarifying question with ask_user_question or wait for the user's guidance.")
}

pub fn auto_paused_note(failures: u32) -> String {
    format!("[Auto-execute paused] {failures} commands failed in a row. Stop, re-read the errors above, and either explain what's going wrong or wait for the user's guidance instead of trying more variants.")
}

pub const AUTO_CONTINUE_NUDGE: &str = "If the task is complete, reply with exactly 'Task Complete' and stop. Otherwise, continue ONLY if there is genuinely more work to do — do not re-run, re-verify, or rephrase steps that already produced the expected output. CRITICAL: if you claimed to create files, you MUST verify each one exists BEFORE declaring Task Complete.";

pub const INTERRUPTED_NOTE: &str = "[Interrupted by user] The user stopped this action. STOP what you are doing and wait for the user to tell you how to proceed.";

pub const QUESTION_DISMISSED: &str = "[User dismissed the question without answering.]";
pub const COMMAND_DISMISSED: &str = "[User dismissed the proposed command without running it.]";
pub const COMMAND_REJECTED_NO_FEEDBACK: &str =
    "[User rejected the command without additional feedback.]";

pub fn command_rejected(feedback: &str) -> String {
    let fb = feedback.trim();
    if fb.is_empty() {
        COMMAND_REJECTED_NO_FEEDBACK.to_string()
    } else {
        format!("[User rejected the command] {fb}")
    }
}

pub fn continue_nudge(shell_tool_name: &str, shell_name: &str) -> String {
    format!("Continue. You stopped without actually generating the command. Now emit the {shell_tool_name} tool call directly, OR write the complete {shell_name} script inside a code block. Do not explain again — produce the code.")
}

/// Spotlight file line: up to two names, then "+N more".
pub fn spotlight_files_line(files: &[String]) -> Option<(String, Option<String>)> {
    match files.len() {
        0 => None,
        1 | 2 => Some((files.join(", "), None)),
        n => Some((files[..2].join(", "), Some(format!(" +{} more", n - 2)))),
    }
}

/// Port of `truncateDisplay` (`ModelSelector.tsx`): keep ~55% of the budget
/// from the head and the rest from the tail, ellipsis in between.
pub fn truncate_middle(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars || max_chars < 5 {
        return s.to_string();
    }
    let keep = max_chars - 1;
    let head = (keep * 55).div_ceil(100);
    let tail = keep - head;
    let h: String = s.chars().take(head).collect();
    let t: String = s.chars().skip(count - tail).collect();
    format!("{h}…{t}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Same cases as src/utils/agent.test.ts.
    #[test]
    fn task_complete_matches_the_frontend() {
        assert!(is_task_complete("Task Complete"));
        assert!(is_task_complete("All done."));
        assert!(is_task_complete("Files have been processed."));
        assert!(is_task_complete("Completed successfully."));
        assert!(!is_task_complete("Running the command now..."));
        assert!(!is_task_complete("Let me check the output first."));
        assert!(!is_task_complete(""));
        assert!(!is_task_complete("   "));
        assert!(is_task_complete("TASK COMPLETE"));
        assert!(is_task_complete("task complete"));
    }

    #[test]
    fn short_keywords_only_count_in_short_text() {
        assert!(is_task_complete("Finished."));
        let long = format!("Finished {}", "x".repeat(300));
        assert!(!is_task_complete(&long));
    }

    #[test]
    fn failure_marker_matches_the_regex() {
        assert!(output_reports_failure("oops\n[Exit code: 1 — Failed]"));
        assert!(output_reports_failure("[Exit code:-2—Failed]"));
        assert!(output_reports_failure("[Exit code:   127   —   Failed]"));
        assert!(!output_reports_failure("[Exit code: 0 — Success]"));
        assert!(!output_reports_failure("[Exit code: x — Failed]"));
        assert!(!output_reports_failure("[Exit code: 1 - Failed]"));
        assert!(!output_reports_failure("all good"));
        assert!(output_reports_failure(
            "[Exit code: zzz] then [Exit code: 3 — Failed]"
        ));
    }

    #[test]
    fn context_updates_match_the_frontend_strings() {
        let inline = context_update_inline("C:\\w", &["a.mp4".into(), "b.mp4".into()]);
        assert_eq!(
            inline,
            "[CONTEXT UPDATE - Files have changed]\n\nWorking directory: C:\\w\nSelected files (2 total):\n1. a.mp4\n2. b.mp4\n\nPlease use these updated files for any subsequent operations."
        );
        let none = context_update_inline("C:\\w", &[]);
        assert!(none.contains("Selected files (0 total):\nNo specific files selected\n"));
        let file = context_update_file("C:\\w", 42, "C:\\t\\l.txt", "HINT");
        assert!(file.contains("Selected files: 42 files (too many to list inline)"));
        assert!(file.contains("written to: C:\\t\\l.txt\n\nHINT\n\n"));
    }

    #[test]
    fn rejection_text() {
        assert_eq!(command_rejected("  "), COMMAND_REJECTED_NO_FEEDBACK);
        assert_eq!(
            command_rejected(" use mov "),
            "[User rejected the command] use mov"
        );
    }

    #[test]
    fn spotlight_files_line_shapes() {
        assert_eq!(spotlight_files_line(&[]), None);
        assert_eq!(
            spotlight_files_line(&["a".into(), "b".into()]),
            Some(("a, b".into(), None))
        );
        assert_eq!(
            spotlight_files_line(&["a".into(), "b".into(), "c".into(), "d".into()]),
            Some(("a, b".into(), Some(" +2 more".into())))
        );
    }

    #[test]
    fn truncate_middle_keeps_ends() {
        assert_eq!(truncate_middle("short", 32), "short");
        let t = truncate_middle("anthropic/claude-sonnet-4.6-very-long-name", 16);
        assert_eq!(t.chars().count(), 16);
        assert!(t.starts_with("anthro"));
        assert!(t.ends_with("name"));
    }
}
