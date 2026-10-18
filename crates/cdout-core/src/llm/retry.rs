/// Outcome of an error classification. Drives the agent's streaming
/// context-recovery decision (see `agent::call_stream_with_context_recovery`).
#[derive(Debug, PartialEq, Eq)]
pub enum ErrorClass {
    /// 429 / 503 / network / capacity — back off and retry.
    TransientRetryable,
    /// 429 you-owe-money, plan does not include this model, billing issues — fail fast.
    BusinessQuota,
    /// Prompt too long for the model's context window — caller can trim history and retry once.
    ContextWindowExceeded,
    /// 2xx with empty body — provider hiccup, re-roll once without backoff.
    EmptyCompletion,
    /// Auth, bad request, unknown — surface to user, no retry.
    Terminal,
}

pub fn classify(err: &str) -> ErrorClass {
    if is_business_quota_429(err) {
        ErrorClass::BusinessQuota
    } else if is_context_window_exceeded(err) {
        ErrorClass::ContextWindowExceeded
    } else if is_empty_completion(err) {
        ErrorClass::EmptyCompletion
    } else if is_transient_retryable(err) {
        ErrorClass::TransientRetryable
    } else {
        ErrorClass::Terminal
    }
}

/// Quota / billing / plan errors that 3 retries will never fix.
pub fn is_business_quota_429(err: &str) -> bool {
    let l = err.to_lowercase();
    l.contains("insufficient_balance")
        || l.contains("insufficient_quota")
        || l.contains("insufficient balance")
        || l.contains("insufficient credit")
        || l.contains("plan does not include")
        || l.contains("billing")
        || l.contains("payment required")
        || l.contains("quota exceeded")
        || l.contains("credit balance")
        || l.contains("402")
}

/// Context-length / prompt-too-long errors — caller may trim history.
///
/// Intentionally avoids matching the bare substring `max_tokens` — Anthropic
/// and OpenAI emit it in many errors that have nothing to do with input length
/// (`Invalid 'max_tokens': must be a positive integer`,
/// `max_tokens must be at most 8192`, etc.). Treating those as
/// `ContextWindowExceeded` would trigger the history trim ladder pointlessly.
pub fn is_context_window_exceeded(err: &str) -> bool {
    let l = err.to_lowercase();
    l.contains("context length")
        || l.contains("context window")
        || l.contains("prompt is too long")
        || l.contains("maximum context")
        || l.contains("token limit")
        || (l.contains("400") && (l.contains("too long") || l.contains("too large")))
}

/// 2xx body parsed as empty — single retry without backoff.
pub fn is_empty_completion(err: &str) -> bool {
    let l = err.to_lowercase();
    l.contains("empty response")
        || l.contains("empty completion")
        || l.contains("empty assistant message")
}

/// Standard transient: 429-rate-limit, 503, network blips. These are worth backing off.
pub fn is_transient_retryable(err: &str) -> bool {
    let l = err.to_lowercase();
    // Reject things classify() already routed to a more specific bucket.
    if is_business_quota_429(&l) || is_context_window_exceeded(&l) {
        return false;
    }
    l.contains("429")
        || l.contains("rate limit")
        || l.contains("rate_limit")
        || l.contains("503")
        || l.contains("overloaded")
        || l.contains("capacity")
        || l.contains("connection reset")
        || l.contains("connection closed")
        || l.contains("broken pipe")
        || l.contains("timed out")
        || l.contains("temporarily unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- classify boundary tests ---

    #[test]
    fn classify_business_quota_takes_precedence_over_429() {
        // A 429 with billing wording should be quota, not transient.
        assert_eq!(
            classify("HTTP 429: insufficient_balance"),
            ErrorClass::BusinessQuota
        );
        assert_eq!(
            classify("Anthropic API Error (429): plan does not include this model"),
            ErrorClass::BusinessQuota
        );
    }

    #[test]
    fn classify_transient_429() {
        assert_eq!(
            classify("OpenAI API Error (429): rate limit exceeded"),
            ErrorClass::TransientRetryable
        );
        assert_eq!(
            classify("HTTP 429 Too Many Requests"),
            ErrorClass::TransientRetryable
        );
    }

    #[test]
    fn classify_context_window() {
        assert_eq!(
            classify("400 Bad Request: prompt is too long"),
            ErrorClass::ContextWindowExceeded
        );
        assert_eq!(
            classify("context length exceeded"),
            ErrorClass::ContextWindowExceeded
        );
        assert_eq!(
            classify("Maximum context of 200000 tokens exceeded"),
            ErrorClass::ContextWindowExceeded
        );
    }

    #[test]
    fn classify_empty_completion() {
        assert_eq!(
            classify("empty response from provider"),
            ErrorClass::EmptyCompletion
        );
        assert_eq!(classify("empty completion"), ErrorClass::EmptyCompletion);
    }

    #[test]
    fn classify_terminal() {
        assert_eq!(classify("401 Unauthorized"), ErrorClass::Terminal);
        assert_eq!(classify("invalid api key"), ErrorClass::Terminal);
        assert_eq!(
            classify("400 Bad Request: invalid model"),
            ErrorClass::Terminal
        );
    }

    #[test]
    fn classify_max_tokens_param_errors_are_terminal_not_context_window() {
        // Reviewer-flagged regression. These are parameter-validation errors,
        // NOT input-length errors; treating them as context-window would
        // trigger pointless history trims.
        assert_eq!(
            classify("Invalid 'max_tokens': must be a positive integer"),
            ErrorClass::Terminal
        );
        assert_eq!(
            classify("max_tokens parameter is missing"),
            ErrorClass::Terminal
        );
        assert_eq!(
            classify("max_tokens must be at most 8192 for model claude-haiku"),
            ErrorClass::Terminal
        );
    }

    #[test]
    fn classify_real_context_overflow_still_caught() {
        assert_eq!(
            classify("prompt is too long: 250000 tokens > 200000 context"),
            ErrorClass::ContextWindowExceeded
        );
        assert_eq!(
            classify("context length exceeded"),
            ErrorClass::ContextWindowExceeded
        );
        assert_eq!(
            classify("token limit reached"),
            ErrorClass::ContextWindowExceeded
        );
        assert_eq!(
            classify("400 Bad Request: prompt too long for this model"),
            ErrorClass::ContextWindowExceeded
        );
    }

    #[test]
    fn classify_503() {
        assert_eq!(classify("HTTP 503"), ErrorClass::TransientRetryable);
        assert_eq!(classify("503 overloaded"), ErrorClass::TransientRetryable);
    }

    #[test]
    fn classify_network_errors() {
        assert_eq!(
            classify("connection reset by peer"),
            ErrorClass::TransientRetryable
        );
        assert_eq!(classify("broken pipe"), ErrorClass::TransientRetryable);
        assert_eq!(
            classify("operation timed out"),
            ErrorClass::TransientRetryable
        );
    }

    #[test]
    fn classify_402_payment() {
        assert_eq!(
            classify("HTTP 402 Payment Required"),
            ErrorClass::BusinessQuota
        );
    }
}
