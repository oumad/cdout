//! Conversation-history recovery on context-window overflow.
//!
//! Two-step ladder that the retry wrapper falls back to when a provider returns
//! `ErrorClass::ContextWindowExceeded`:
//!
//! 1. `fast_trim_tool_results`: truncate the body of older `tool` messages to
//!    `MAX_TOOL_RESULT_CHARS`. Tool outputs (PowerShell dumps, ffmpeg logs) are
//!    usually the largest contributors and the least irreplaceable.
//! 2. `emergency_history_trim`: drop the oldest non-system user/assistant/tool
//!    message pairs until the history shrinks by `EMERGENCY_TRIM_FRACTION`.
//!
//! After every trim, `remove_orphaned_tool_messages` reconciles dangling
//! `tool_use` ids — Anthropic rejects with HTTP 400 if a `tool_use` is in the
//! prefix but the matching `tool_result` was dropped (or vice-versa).

use crate::llm::Message;
use std::collections::HashSet;

/// Cap on tool message body length after fast-trim. Tuned to keep stdout/stderr
/// snippets useful (filenames, exit codes) while shedding large dumps.
const MAX_TOOL_RESULT_CHARS: usize = 200;

/// Keep at least the system message + last N turn pairs intact even under
/// emergency trim. Below this floor the agent loop has nothing to recover from.
const MIN_PROTECTED_MESSAGES: usize = 4;

/// Fraction of total messages to drop in a single emergency trim pass.
const EMERGENCY_TRIM_FRACTION: f32 = 0.4;

/// Pass 1: truncate older `tool` message bodies. Preserves the most recent
/// `keep_recent_tools` tool results in full (they're usually the ones the LLM
/// is currently reasoning about).
pub fn fast_trim_tool_results(messages: &mut [Message], keep_recent_tools: usize) -> usize {
    let tool_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == "tool")
        .map(|(i, _)| i)
        .collect();

    if tool_indices.len() <= keep_recent_tools {
        return 0;
    }

    let cutoff = tool_indices.len().saturating_sub(keep_recent_tools);
    let mut trimmed = 0;
    for &idx in &tool_indices[..cutoff] {
        let msg = &mut messages[idx];
        if msg.content.len() > MAX_TOOL_RESULT_CHARS {
            let original_len = msg.content.len();
            // char-boundary safe truncation
            let mut cut = MAX_TOOL_RESULT_CHARS;
            while cut > 0 && !msg.content.is_char_boundary(cut) {
                cut -= 1;
            }
            msg.content.truncate(cut);
            msg.content
                .push_str(&format!("\n... [trimmed {} chars]", original_len - cut));
            trimmed += 1;
        }
    }
    trimmed
}

/// Pass 2: drop the oldest non-system messages until we've shed
/// `EMERGENCY_TRIM_FRACTION` of the history. Returns the number dropped.
///
/// Never touches the first system message, never trims below
/// `MIN_PROTECTED_MESSAGES`.
pub fn emergency_history_trim(messages: &mut Vec<Message>) -> usize {
    if messages.len() <= MIN_PROTECTED_MESSAGES {
        return 0;
    }

    let total = messages.len();
    let target_drop = ((total as f32) * EMERGENCY_TRIM_FRACTION).ceil() as usize;
    let target_drop = target_drop.min(total - MIN_PROTECTED_MESSAGES);
    if target_drop == 0 {
        return 0;
    }

    // Find the first non-system index (the system message is always preserved).
    let first_non_system = messages
        .iter()
        .position(|m| m.role != "system")
        .unwrap_or(0);

    let drop_end = (first_non_system + target_drop).min(messages.len());
    let dropped = drop_end - first_non_system;
    messages.drain(first_non_system..drop_end);
    dropped
}

/// After any trim, walk the conversation and drop:
/// - `tool_result` messages whose `tool_use_id` no longer has a matching
///   `tool_use` block in any preceding assistant message.
/// - `tool_calls` on assistant messages whose corresponding `tool_result` was
///   dropped (we strip the orphaned tool_call rather than rebuild the message).
///
/// Anthropic returns HTTP 400 if either side of the pair is missing.
pub fn remove_orphaned_tool_messages(messages: &mut Vec<Message>) -> usize {
    // Pass 1: collect every tool_use id that survives (from any assistant
    // message's tool_calls), and every tool_result id we see.
    let surviving_tool_use_ids: HashSet<String> = messages
        .iter()
        .filter(|m| m.role == "assistant")
        .flat_map(|m| m.tool_calls.as_deref().unwrap_or(&[]))
        .filter_map(|tc| tc.id.clone())
        .collect();

    let mut orphaned_count = 0;

    // Drop tool messages whose id isn't represented by any surviving tool_use.
    let pre_len = messages.len();
    messages.retain(|m| {
        if m.role != "tool" {
            return true;
        }
        // If the tool message carries an id via its first tool_call, check it.
        let id = m
            .tool_calls
            .as_ref()
            .and_then(|tc| tc.first())
            .and_then(|tc| tc.id.clone());
        match id {
            Some(id) => surviving_tool_use_ids.contains(&id),
            None => true, // No id — assume legacy single-tool layout, keep it.
        }
    });
    orphaned_count += pre_len - messages.len();

    // Pass 2: also drop tool_calls on assistant messages whose tool_result
    // disappeared (rare but happens when the assistant message survived but the
    // tool message it triggered did not).
    let surviving_tool_result_ids: HashSet<String> = messages
        .iter()
        .filter(|m| m.role == "tool")
        .flat_map(|m| m.tool_calls.as_deref().unwrap_or(&[]))
        .filter_map(|tc| tc.id.clone())
        .collect();

    for msg in messages.iter_mut() {
        if msg.role != "assistant" {
            continue;
        }
        if let Some(tool_calls) = msg.tool_calls.as_mut() {
            let before = tool_calls.len();
            tool_calls.retain(|tc| {
                tc.id
                    .as_ref()
                    .is_none_or(|id| surviving_tool_result_ids.contains(id))
            });
            orphaned_count += before - tool_calls.len();
            if tool_calls.is_empty() {
                msg.tool_calls = None;
            }
        }
    }

    // Pass 3: drop assistant messages that now have neither text content nor
    // tool_calls. Anthropic rejects these with HTTP 400 ("text content blocks
    // must contain non-empty text content"). This is the common shape after an
    // emergency_history_trim that dropped a tool_result whose preceding
    // assistant emitted only a tool_use with empty text.
    let pre_assistant = messages.len();
    messages.retain(|m| {
        !(m.role == "assistant" && m.content.trim().is_empty() && m.tool_calls.is_none())
    });
    let dropped_empty_assistants = pre_assistant - messages.len();
    orphaned_count += dropped_empty_assistants;

    // Pass 4: if we dropped any empty assistants, re-run the tool-result
    // orphan check — those tool messages may now point at vanished ids.
    if dropped_empty_assistants > 0 {
        let surviving_tool_use_ids: std::collections::HashSet<String> = messages
            .iter()
            .filter(|m| m.role == "assistant")
            .flat_map(|m| m.tool_calls.as_deref().unwrap_or(&[]))
            .filter_map(|tc| tc.id.clone())
            .collect();
        let pre = messages.len();
        messages.retain(|m| {
            if m.role != "tool" {
                return true;
            }
            let id = m
                .tool_calls
                .as_ref()
                .and_then(|tc| tc.first())
                .and_then(|tc| tc.id.clone());
            match id {
                Some(id) => surviving_tool_use_ids.contains(&id),
                None => true,
            }
        });
        orphaned_count += pre - messages.len();
    }

    orphaned_count
}

/// Convenience: run the full ladder. Returns a tuple of (trimmed_tool_bodies,
/// dropped_messages, reconciled_orphans).
pub fn recover_from_context_overflow(messages: &mut Vec<Message>) -> (usize, usize, usize) {
    let trimmed = fast_trim_tool_results(messages, 2);
    if trimmed > 0 {
        let orphans = remove_orphaned_tool_messages(messages);
        return (trimmed, 0, orphans);
    }
    let dropped = emergency_history_trim(messages);
    let orphans = remove_orphaned_tool_messages(messages);
    (0, dropped, orphans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{FunctionCall, ToolCall};
    use serde_json::json;

    fn sys(s: &str) -> Message {
        Message {
            role: "system".to_string(),
            content: s.to_string(),
            ..Default::default()
        }
    }
    fn user(s: &str) -> Message {
        Message {
            role: "user".to_string(),
            content: s.to_string(),
            ..Default::default()
        }
    }
    fn assistant_tool(id: &str) -> Message {
        Message {
            role: "assistant".to_string(),
            content: String::new(),
            tool_calls: Some(vec![ToolCall {
                id: Some(id.to_string()),
                function: FunctionCall {
                    name: "run_powershell".to_string(),
                    arguments: json!({ "command": "x" }),
                },
            }]),
            ..Default::default()
        }
    }
    fn tool_result(id: &str, body: &str) -> Message {
        Message {
            role: "tool".to_string(),
            content: body.to_string(),
            tool_calls: Some(vec![ToolCall {
                id: Some(id.to_string()),
                function: FunctionCall {
                    name: "run_powershell".to_string(),
                    arguments: json!({}),
                },
            }]),
            ..Default::default()
        }
    }

    #[test]
    fn fast_trim_keeps_recent_tools_intact() {
        let big = "x".repeat(500);
        let mut msgs = vec![
            sys("system"),
            user("u1"),
            tool_result("t1", &big),
            user("u2"),
            tool_result("t2", &big),
            user("u3"),
            tool_result("t3", &big),
        ];
        let trimmed = fast_trim_tool_results(&mut msgs, 2);
        assert_eq!(trimmed, 1, "only the oldest tool should be trimmed");
        assert!(msgs[2].content.len() < 500, "oldest tool truncated");
        assert!(msgs[4].content.len() == 500, "middle tool kept intact");
        assert!(msgs[6].content.len() == 500, "recent tool kept intact");
    }

    #[test]
    fn fast_trim_no_op_when_under_threshold() {
        let mut msgs = vec![sys("s"), user("u"), tool_result("t1", "short")];
        let trimmed = fast_trim_tool_results(&mut msgs, 2);
        assert_eq!(trimmed, 0);
    }

    #[test]
    fn fast_trim_char_boundary_safe() {
        let body = "é".repeat(200); // 400 bytes
        let mut msgs = vec![
            sys("s"),
            tool_result("t1", &body),
            user("u"),
            tool_result("t2", "ok"),
            user("u"),
            tool_result("t3", "ok"),
        ];
        // Should not panic on char boundary
        let _ = fast_trim_tool_results(&mut msgs, 2);
        // And content should still be valid UTF-8
        assert!(msgs[1].content.is_char_boundary(msgs[1].content.len()));
    }

    #[test]
    fn emergency_trim_preserves_system_and_minimum() {
        let mut msgs = vec![
            sys("S"),
            user("u1"),
            assistant_tool("t1"),
            tool_result("t1", "r1"),
            user("u2"),
            assistant_tool("t2"),
            tool_result("t2", "r2"),
            user("u3"),
            assistant_tool("t3"),
            tool_result("t3", "r3"),
        ];
        let dropped = emergency_history_trim(&mut msgs);
        assert!(dropped > 0, "should drop something");
        assert_eq!(msgs[0].role, "system", "system message preserved");
        assert!(
            msgs.len() >= MIN_PROTECTED_MESSAGES,
            "did not violate floor"
        );
    }

    #[test]
    fn emergency_trim_no_op_at_minimum() {
        let mut msgs = vec![sys("S"), user("u1"), user("u2"), user("u3")];
        let dropped = emergency_history_trim(&mut msgs);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn orphan_reconciliation_drops_orphaned_tool_result() {
        let mut msgs = vec![
            sys("S"),
            user("u"),
            // assistant_tool("t1") would normally be here but we dropped it
            tool_result("t1", "r1"),
        ];
        let dropped = remove_orphaned_tool_messages(&mut msgs);
        assert_eq!(dropped, 1, "orphaned tool_result dropped");
        assert!(msgs.iter().all(|m| m.role != "tool"));
    }

    #[test]
    fn orphan_reconciliation_drops_orphaned_tool_call() {
        let mut msgs = vec![
            sys("S"),
            user("u"),
            assistant_tool("t1"),
            // tool_result("t1", ...) is missing — the matching tool_call must be stripped
        ];
        let removed = remove_orphaned_tool_messages(&mut msgs);
        // Pass 1 strips the orphaned tool_call (1), then Pass 3 drops the now-empty
        // assistant message itself (1). Total: 2 removals.
        assert_eq!(removed, 2);
        // The empty assistant was dropped — no assistant messages remain.
        assert!(msgs.iter().all(|m| m.role != "assistant"));
        assert_eq!(msgs.len(), 2); // system + user
    }

    #[test]
    fn orphan_reconciliation_cascades_when_empty_assistant_dropped() {
        // assistant with empty content + tool_use whose tool_result vanished.
        // After Pass 1 the tool_call is stripped (assistant becomes empty + no
        // tool_calls). Pass 3 drops the assistant. Pass 4 re-checks orphans
        // against the surviving assistants — nothing left to clean.
        let mut msgs = vec![
            sys("S"),
            user("u"),
            assistant_tool("t1"),
            // No matching tool_result.
        ];
        remove_orphaned_tool_messages(&mut msgs);
        // Validate the post-state is Anthropic-shippable: every assistant has
        // either content or tool_calls (or both).
        for m in &msgs {
            if m.role == "assistant" {
                assert!(
                    !m.content.trim().is_empty() || m.tool_calls.is_some(),
                    "empty assistant must not survive reconciliation"
                );
            }
        }
    }

    #[test]
    fn orphan_reconciliation_keeps_matched_pairs() {
        let mut msgs = vec![
            sys("S"),
            user("u"),
            assistant_tool("t1"),
            tool_result("t1", "r1"),
        ];
        let pre = msgs.len();
        let removed = remove_orphaned_tool_messages(&mut msgs);
        assert_eq!(removed, 0);
        assert_eq!(msgs.len(), pre);
    }

    #[test]
    fn recover_runs_full_ladder() {
        let big = "x".repeat(500);
        let mut msgs = vec![
            sys("S"),
            user("u1"),
            assistant_tool("t1"),
            tool_result("t1", &big),
            user("u2"),
            assistant_tool("t2"),
            tool_result("t2", &big),
            user("u3"),
            assistant_tool("t3"),
            tool_result("t3", &big),
        ];
        let (trimmed, dropped, orphans) = recover_from_context_overflow(&mut msgs);
        // Should trim, not drop, on first call (3 tools, keep 2 most recent = trim 1).
        assert!(trimmed >= 1);
        assert_eq!(dropped, 0);
        // No orphans because we only truncated bodies, not dropped messages.
        assert_eq!(orphans, 0);
    }
}
