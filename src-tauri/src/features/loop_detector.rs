//! Tool-call loop detection — three patterns, four-state escalation.
//!
//! Replaces the legacy `should_nudge` text-heuristic (which misfired on natural
//! assistant prose like "I'll process the files now.") and the frontend-side
//! hash check in `App.tsx::recordToolCallAndDetectLoop` (which lived in JS,
//! couldn't see backend state, and could contradict the Rust loop logic).
//!
//! Three patterns, each independently configurable:
//!
//! 1. **Exact repeat** — the same canonical `(tool_name, args)` fingerprint
//!    appears `exact_repeat_threshold` times within the window.
//! 2. **Ping-pong** — two distinct fingerprints alternate `ping_pong_threshold`
//!    times within the window. Catches "try-ffmpeg-A, try-ffmpeg-B, try-A,
//!    try-B" thrash that the exact-repeat detector misses.
//! 3. **No progress** — the same tool name has been called `no_progress_threshold`
//!    times with no successful (non-error) tool_result in between. Catches
//!    failing-command spirals where args vary slightly each time.
//!
//! Four-state escalation:
//! - `Ok` — no signal, continue normally.
//! - `Warning` — pattern detected once. Caller may inject a nudge.
//! - `Block` — pattern persists. Caller MUST stop auto-execute and surface the
//!   proposal for explicit user approval.
//! - `Break` — clear-cut runaway. Caller MUST drop tool definitions from the
//!   next request (force a text-only reassessment).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

/// Per-detection threshold configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopDetectorConfig {
    pub window: usize,
    pub exact_repeat_threshold: usize,
    pub ping_pong_threshold: usize,
    pub no_progress_threshold: usize,
}

impl Default for LoopDetectorConfig {
    fn default() -> Self {
        Self {
            window: 6,
            exact_repeat_threshold: 3,
            ping_pong_threshold: 3,
            no_progress_threshold: 4,
        }
    }
}

/// Result of recording an observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "reason")]
pub enum LoopVerdict {
    /// No loop signal — continue normally.
    Ok,
    /// Pattern detected once. Nudge the model but keep going.
    Warning(String),
    /// Pattern persistent — pause auto-execute, require explicit user approval.
    Block(String),
    /// Runaway loop — drop tools from the next request entirely.
    Break(String),
}

#[allow(dead_code)] // Frontend reads kind/reason directly via JSON; these
                    // helpers are kept as the public Rust-side API.
impl LoopVerdict {
    pub fn is_ok(&self) -> bool {
        matches!(self, LoopVerdict::Ok)
    }
    pub fn should_pause_auto_execute(&self) -> bool {
        matches!(self, LoopVerdict::Block(_) | LoopVerdict::Break(_))
    }
    pub fn should_drop_tools(&self) -> bool {
        matches!(self, LoopVerdict::Break(_))
    }
    pub fn reason(&self) -> Option<&str> {
        match self {
            LoopVerdict::Ok => None,
            LoopVerdict::Warning(r) | LoopVerdict::Block(r) | LoopVerdict::Break(r) => Some(r),
        }
    }
}

/// Internal observation. We track only what's needed for the three detectors.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Observation {
    tool_name: String,
    /// Canonical-JSON fingerprint of (tool_name, args).
    fingerprint: String,
    /// Whether the tool execution succeeded (None = not yet executed).
    success: Option<bool>,
}

/// Which detector fired. Currently the escalation key carries the pattern as
/// a string prefix; this enum is kept for future typed pattern reporting and
/// to document the three pattern names in one place.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Pattern {
    Exact,
    PingPong,
    NoProgress,
}

#[derive(Debug, Default)]
pub struct LoopDetector {
    config: LoopDetectorConfig,
    observations: VecDeque<Observation>,
    /// Warning count per fingerprint — escalates Warning → Block on second hit.
    warning_seen: std::collections::HashMap<String, usize>,
}

impl LoopDetector {
    pub fn new(config: LoopDetectorConfig) -> Self {
        Self {
            config,
            observations: VecDeque::new(),
            warning_seen: std::collections::HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(LoopDetectorConfig::default())
    }

    /// Reset all state — call on new user prompt or session restart.
    pub fn reset(&mut self) {
        self.observations.clear();
        self.warning_seen.clear();
    }

    /// Record a proposed tool call and return the verdict.
    /// Call BEFORE executing the tool.
    pub fn record_proposal(&mut self, tool_name: &str, args: &Value) -> LoopVerdict {
        let fp = canonical_fingerprint(tool_name, args);
        let obs = Observation {
            tool_name: tool_name.to_string(),
            fingerprint: fp.clone(),
            success: None,
        };
        self.observations.push_back(obs);
        while self.observations.len() > self.config.window {
            self.observations.pop_front();
        }
        self.classify(&fp)
    }

    /// Record the outcome of the most recent tool execution.
    /// Call AFTER the tool has run.
    pub fn record_outcome(&mut self, success: bool) {
        if let Some(last) = self.observations.back_mut() {
            last.success = Some(success);
        }
    }

    fn classify(&mut self, current_fp: &str) -> LoopVerdict {
        let current_tool = self
            .observations
            .back()
            .map(|o| o.tool_name.clone())
            .unwrap_or_default();

        // --- Pattern 1: exact-repeat
        let same_count = self
            .observations
            .iter()
            .filter(|o| o.fingerprint == current_fp)
            .count();
        if same_count >= self.config.exact_repeat_threshold {
            let key = format!("exact::{}", current_fp);
            return self.escalate(
                &key,
                format!(
                    "Same tool call ({} args identical) has appeared {} times in the last {} turns",
                    current_tool,
                    same_count,
                    self.observations.len()
                ),
            );
        }

        // --- Pattern 2: ping-pong (two fingerprints alternating)
        if let Some((reason, pair_key)) = self.detect_ping_pong() {
            let key = format!("pingpong::{}", pair_key);
            return self.escalate(&key, reason);
        }

        // --- Pattern 3: no-progress (same tool name N+ times, all failing).
        //   Important: key by tool_name, not fingerprint — the whole point of
        //   no_progress is that fingerprints drift per attempt. Keying by
        //   fingerprint would never let it escalate past Warning.
        if let Some(reason) = self.detect_no_progress() {
            let key = format!("noprogress::{}", current_tool);
            return self.escalate(&key, reason);
        }

        LoopVerdict::Ok
    }

    fn escalate(&mut self, escalation_key: &str, reason: String) -> LoopVerdict {
        let count = self
            .warning_seen
            .entry(escalation_key.to_string())
            .or_insert(0);
        *count += 1;
        match *count {
            1 => LoopVerdict::Warning(reason),
            2 => LoopVerdict::Block(reason),
            _ => LoopVerdict::Break(reason),
        }
    }

    /// Ping-pong: A,B,A,B,A,B... — two distinct fingerprints alternating.
    /// Returns Some((reason, sorted_pair_key)) if detected. The pair_key is
    /// commutative so swapping A/B doesn't double-count escalations.
    fn detect_ping_pong(&self) -> Option<(String, String)> {
        let need = self.config.ping_pong_threshold * 2;
        if self.observations.len() < need {
            return None;
        }
        let tail: Vec<&Observation> = self
            .observations
            .iter()
            .rev()
            .take(need)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let fp0 = &tail[0].fingerprint;
        let fp1 = &tail[1].fingerprint;
        if fp0 == fp1 {
            return None;
        }
        for (i, obs) in tail.iter().enumerate() {
            let expected = if i % 2 == 0 { fp0 } else { fp1 };
            if &obs.fingerprint != expected {
                return None;
            }
        }
        let pair_key = {
            let (a, b) = if fp0 <= fp1 { (fp0, fp1) } else { (fp1, fp0) };
            format!("{}|{}", a, b)
        };
        let reason = format!(
            "Two tool calls have been alternating ({} ↔ {}) for {} turns — looks like a ping-pong loop",
            tail[0].tool_name, tail[1].tool_name, need
        );
        Some((reason, pair_key))
    }

    /// No-progress: same tool name has been invoked N+ times AND at least N of
    /// those have a recorded failure outcome, with zero recorded successes.
    /// Requires actual failure evidence — "no outcome recorded yet" does not
    /// count as failure (prevents false-positives during mid-loop state).
    fn detect_no_progress(&self) -> Option<String> {
        let name = self.observations.back().map(|o| o.tool_name.clone())?;
        let same_name_obs: Vec<&Observation> = self
            .observations
            .iter()
            .filter(|o| o.tool_name == name)
            .collect();
        if same_name_obs.len() < self.config.no_progress_threshold {
            return None;
        }
        let any_success = same_name_obs.iter().any(|o| o.success == Some(true));
        if any_success {
            return None;
        }
        let failures = same_name_obs
            .iter()
            .filter(|o| o.success == Some(false))
            .count();
        if failures < self.config.no_progress_threshold {
            return None;
        }
        Some(format!(
            "`{}` failed {} times in the last {} turns with no successful execution — agent appears stuck",
            name,
            failures,
            self.observations.len()
        ))
    }
}

/// Build a stable fingerprint string from (tool_name, args).
///
/// Stability requires canonicalising the JSON: serde_json preserves insertion
/// order, so `{a:1, b:2}` and `{b:2, a:1}` would otherwise produce different
/// fingerprints. We walk and sort object keys recursively.
pub fn canonical_fingerprint(tool_name: &str, args: &Value) -> String {
    let canonical = canonicalize_json(args);
    format!(
        "{}::{}",
        tool_name,
        serde_json::to_string(&canonical).unwrap_or_default()
    )
}

// --- Process-wide singleton ---
// cdout has one concurrent agent loop by design (spotlight kicks off the
// main window). A single global detector is the right shape; we'd swap to
// per-session tokens if/when multi-window agent loops land.

static GLOBAL_DETECTOR: OnceLock<Mutex<LoopDetector>> = OnceLock::new();

fn global() -> &'static Mutex<LoopDetector> {
    GLOBAL_DETECTOR.get_or_init(|| Mutex::new(LoopDetector::with_defaults()))
}

/// Reset the global detector — call on new user-initiated conversation.
pub fn reset_global() {
    if let Ok(mut d) = global().lock() {
        d.reset();
    }
}

/// Record a proposed tool call against the global detector.
pub fn record_proposal_global(tool_name: &str, args: &Value) -> LoopVerdict {
    match global().lock() {
        Ok(mut d) => d.record_proposal(tool_name, args),
        Err(_) => LoopVerdict::Ok,
    }
}

/// Record the outcome of the most recently proposed tool execution.
pub fn record_outcome_global(success: bool) {
    if let Ok(mut d) = global().lock() {
        d.record_outcome(success);
    }
}

/// Test-only serialization handle. Any test that touches the global detector
/// must hold this guard for the duration of the test to prevent cargo's
/// parallel test runner from interleaving state.
#[cfg(test)]
pub fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .iter()
                .map(|(k, v)| (k.clone(), canonicalize_json(v)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(serde_json::Map::from_iter(entries))
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonicalize_json).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fast_config() -> LoopDetectorConfig {
        LoopDetectorConfig {
            window: 6,
            exact_repeat_threshold: 3,
            ping_pong_threshold: 3,
            no_progress_threshold: 4,
        }
    }

    #[test]
    fn fingerprint_is_key_order_independent() {
        let a = canonical_fingerprint("run_powershell", &json!({ "x": 1, "y": 2 }));
        let b = canonical_fingerprint("run_powershell", &json!({ "y": 2, "x": 1 }));
        assert_eq!(a, b);
    }

    #[test]
    fn fingerprint_differs_for_different_args() {
        let a = canonical_fingerprint("run_powershell", &json!({ "command": "dir" }));
        let b = canonical_fingerprint("run_powershell", &json!({ "command": "ls" }));
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_differs_for_different_tools() {
        let args = json!({ "command": "x" });
        let a = canonical_fingerprint("run_powershell", &args);
        let b = canonical_fingerprint("ask_user_question", &args);
        assert_ne!(a, b);
    }

    #[test]
    fn ok_under_threshold() {
        let mut d = LoopDetector::new(fast_config());
        let v = d.record_proposal("run_powershell", &json!({ "command": "echo hi" }));
        assert!(v.is_ok());
    }

    #[test]
    fn exact_repeat_escalates_through_states() {
        let mut d = LoopDetector::new(fast_config());
        let args = json!({ "command": "echo x" });
        d.record_proposal("run_powershell", &args);
        d.record_proposal("run_powershell", &args);
        // Third hit triggers exact_repeat → Warning
        let v1 = d.record_proposal("run_powershell", &args);
        assert!(matches!(v1, LoopVerdict::Warning(_)));
        // Fourth hit → Block
        let v2 = d.record_proposal("run_powershell", &args);
        assert!(matches!(v2, LoopVerdict::Block(_)));
        // Fifth hit → Break
        let v3 = d.record_proposal("run_powershell", &args);
        assert!(matches!(v3, LoopVerdict::Break(_)));
        assert!(v3.should_pause_auto_execute());
        assert!(v3.should_drop_tools());
    }

    #[test]
    fn ping_pong_is_detected() {
        let mut d = LoopDetector::new(fast_config());
        let a = json!({ "command": "ffmpeg -i a.mp4 out.mp4" });
        let b = json!({ "command": "ffmpeg -i b.mp4 out.mp4" });
        d.record_proposal("run_powershell", &a);
        d.record_proposal("run_powershell", &b);
        d.record_proposal("run_powershell", &a);
        d.record_proposal("run_powershell", &b);
        d.record_proposal("run_powershell", &a);
        let v = d.record_proposal("run_powershell", &b);
        // 6 observations alternating A/B → ping_pong_threshold=3 met.
        assert!(!v.is_ok(), "expected ping-pong detection, got {:?}", v);
    }

    #[test]
    fn no_progress_detects_repeated_failures() {
        let mut d = LoopDetector::new(fast_config());
        for i in 0..4 {
            d.record_proposal(
                "run_powershell",
                &json!({ "command": format!("attempt {}", i) }),
            );
            d.record_outcome(false);
        }
        // 4 distinct calls but all failed → no_progress threshold = 4 hit.
        // The 4th call's verdict was at recording-time, so we need to look at
        // the state AFTER recording all 4. The 4th proposal returned Warning
        // because no-progress requires N+1 calls before triggering... actually
        // since we just recorded the 4th, the detector sees 4 same-name failed
        // observations. With threshold=4 it triggers.
        let v = d.record_proposal("run_powershell", &json!({ "command": "attempt 5" }));
        assert!(!v.is_ok(), "expected no-progress detection, got {:?}", v);
    }

    #[test]
    fn no_progress_escalates_past_warning_with_drifting_args() {
        // Reviewer-flagged: with per-fingerprint warning_seen keys, no_progress
        // could only ever return Warning (each attempt has a new fingerprint).
        // With pattern-keyed escalation it should reach Block then Break.
        let mut d = LoopDetector::new(fast_config());
        let mut verdicts: Vec<LoopVerdict> = Vec::new();
        for i in 0..12 {
            let v = d.record_proposal(
                "run_powershell",
                &serde_json::json!({ "command": format!("variant_{}", i) }),
            );
            d.record_outcome(false);
            verdicts.push(v);
        }
        // Once no_progress triggers, escalations should reach Block and Break.
        let any_block = verdicts.iter().any(|v| matches!(v, LoopVerdict::Block(_)));
        let any_break = verdicts.iter().any(|v| matches!(v, LoopVerdict::Break(_)));
        assert!(
            any_block,
            "no_progress should escalate to Block, got {:?}",
            verdicts
        );
        assert!(
            any_break,
            "no_progress should escalate to Break, got {:?}",
            verdicts
        );
    }

    #[test]
    fn no_progress_ignored_after_a_success() {
        let mut d = LoopDetector::new(fast_config());
        d.record_proposal("run_powershell", &json!({ "command": "a" }));
        d.record_outcome(false);
        d.record_proposal("run_powershell", &json!({ "command": "b" }));
        d.record_outcome(true); // success!
        d.record_proposal("run_powershell", &json!({ "command": "c" }));
        d.record_outcome(false);
        let v = d.record_proposal("run_powershell", &json!({ "command": "d" }));
        // 4 same-name observations but one succeeded — should NOT trigger no_progress
        // (exact-repeat and ping-pong also not triggered with 4 distinct args).
        assert!(v.is_ok());
    }

    #[test]
    fn window_eviction_works() {
        let mut d = LoopDetector::new(fast_config());
        let same = json!({ "command": "echo same" });
        d.record_proposal("run_powershell", &same);
        d.record_proposal("run_powershell", &same);
        // Push 6 different proposals to evict the two `same` ones.
        for i in 0..6 {
            d.record_proposal(
                "run_powershell",
                &json!({ "command": format!("unique {}", i) }),
            );
        }
        // Now record `same` once more — count should be 1, not 3.
        let v = d.record_proposal("run_powershell", &same);
        assert!(v.is_ok());
    }

    #[test]
    fn reset_clears_state() {
        let mut d = LoopDetector::new(fast_config());
        let same = json!({ "command": "echo x" });
        d.record_proposal("run_powershell", &same);
        d.record_proposal("run_powershell", &same);
        d.record_proposal("run_powershell", &same);
        d.reset();
        let v = d.record_proposal("run_powershell", &same);
        assert!(v.is_ok());
    }

    #[test]
    fn verdict_helpers() {
        assert!(LoopVerdict::Ok.is_ok());
        assert!(!LoopVerdict::Warning("x".into()).is_ok());
        assert!(LoopVerdict::Block("x".into()).should_pause_auto_execute());
        assert!(LoopVerdict::Break("x".into()).should_drop_tools());
        assert!(!LoopVerdict::Warning("x".into()).should_drop_tools());
    }
}
