use crate::auth::cli_credentials;
use crate::llm::clients::{anthropic, antigravity, gemini, ollama, openai};
use crate::llm::{Message, ToolDefinition};
use crate::utils::config;
use std::time::Duration;

/// Max retry attempts for transient errors (429, 503, network).
const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff.
const RETRY_BASE_DELAY: Duration = Duration::from_secs(2);

/// Check if an error is retryable (rate limit, overloaded, or transient network error).
fn is_retryable(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("503")
        || lower.contains("overloaded")
        || lower.contains("capacity")
        || lower.contains("connection reset")
        || lower.contains("connection closed")
        || lower.contains("broken pipe")
        || lower.contains("timed out")
}

/// Retry wrapper with exponential backoff + jitter for transient errors.
/// Non-retryable errors (auth, bad request, etc.) fail immediately.
async fn with_retry<F, Fut>(description: &str, f: F) -> Result<Message, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<Message, String>>,
{
    let mut last_err = String::new();

    for attempt in 0..=MAX_RETRIES {
        match f().await {
            Ok(msg) => return Ok(msg),
            Err(e) => {
                if attempt < MAX_RETRIES && is_retryable(&e) {
                    let delay_secs = RETRY_BASE_DELAY.as_secs() * 2u64.pow(attempt);
                    // Add jitter: 0-1 second
                    let jitter_ms = (rand::random::<u64>() % 1000) as u64;
                    let delay = Duration::from_secs(delay_secs) + Duration::from_millis(jitter_ms);

                    eprintln!(
                        "[{}] Retryable error (attempt {}/{}), retrying in {:.1}s: {}",
                        description,
                        attempt + 1,
                        MAX_RETRIES,
                        delay.as_secs_f64(),
                        e
                    );
                    tokio::time::sleep(delay).await;
                    last_err = e;
                    continue;
                }
                return Err(e);
            }
        }
    }

    Err(format!(
        "{} failed after {} retries. Last error: {}",
        description, MAX_RETRIES, last_err
    ))
}

pub async fn route_chat(
    model: &str,
    history: Vec<Message>,
    tools: Option<Vec<ToolDefinition>>,
) -> Result<Message, String> {
    let model = model.to_string();
    let history = history.clone();
    let tools = tools.clone();

    if model.starts_with("claude:") {
        let model_name = model
            .strip_prefix("claude:")
            .unwrap_or("claude-sonnet-4-20250514")
            .to_string();
        let h = history.clone();
        let t = tools.clone();
        with_retry("Claude", || {
            let mn = model_name.clone();
            let h = h.clone();
            let t = t.clone();
            async move {
                let creds = cli_credentials::get_valid_claude_code_credentials().await?;
                anthropic::chat_anthropic_stream(&mn, h, &creds.access_token, t, |_| {}).await
            }
        })
        .await
    } else if model.starts_with("codex:") {
        let creds = cli_credentials::read_codex_credentials()
            .ok_or("Codex CLI credentials not found. Set OPENAI_API_KEY or install Codex CLI.")?;
        let model_name = model.strip_prefix("codex:").unwrap_or("gpt-4o").to_string();
        let key = creds.api_key.clone();
        with_retry("Codex", || {
            let mn = model_name.clone();
            let h = history.clone();
            let k = key.clone();
            async move { openai::chat_openai(&mn, h, &k).await }
        })
        .await
    } else if model.starts_with("antigravity") {
        // Antigravity has its own retry logic for 503, so no wrapper here
        antigravity::chat_stream("gemini-3-flash", history, tools, |_| {}).await
    } else if model.starts_with("openai:") {
        let keys = config::load_api_keys();
        let key = keys
            .openai
            .ok_or("LLM Error: No OpenAI API Key set. Please configure it in Settings.")?;
        let model_name = model
            .strip_prefix("openai:")
            .unwrap_or("gpt-4")
            .to_string();
        with_retry("OpenAI", || {
            let mn = model_name.clone();
            let h = history.clone();
            let k = key.clone();
            async move { openai::chat_openai(&mn, h, &k).await }
        })
        .await
    } else if model.starts_with("gemini:") {
        let keys = config::load_api_keys();
        let key = keys
            .gemini
            .ok_or("LLM Error: No Gemini API Key set. Please configure it in Settings.")?;
        let model_name = model
            .strip_prefix("gemini:")
            .unwrap_or("gemini-1.5-pro")
            .to_string();
        with_retry("Gemini", || {
            let mn = model_name.clone();
            let h = history.clone();
            let k = key.clone();
            async move { gemini::chat_gemini(&mn, h, &k).await }
        })
        .await
    } else {
        let ollama_url = config::get_ollama_url();
        let m = model.to_string();
        with_retry("Ollama", || {
            let url = ollama_url.clone();
            let m = m.clone();
            let h = history.clone();
            let t = tools.clone();
            async move { ollama::chat(&url, &m, h, t).await }
        })
        .await
    }
}

pub async fn route_chat_stream(
    model: &str,
    history: Vec<Message>,
    tools: Option<Vec<ToolDefinition>>,
    on_chunk: impl Fn(String) + Send + 'static,
) -> Result<Message, String> {
    // Streaming calls are not retried — the callback has already been invoked
    // with partial text, so retrying would produce duplicate output.
    // The per-chunk idle timeout (in stream_util) handles stalled streams instead.
    if model.starts_with("claude:") {
        let creds = cli_credentials::get_valid_claude_code_credentials().await?;
        let model_name = model
            .strip_prefix("claude:")
            .unwrap_or("claude-sonnet-4-20250514");
        anthropic::chat_anthropic_stream(model_name, history, &creds.access_token, tools, on_chunk)
            .await
    } else if model.starts_with("codex:") {
        let creds = cli_credentials::read_codex_credentials()
            .ok_or("Codex CLI credentials not found. Set OPENAI_API_KEY or install Codex CLI.")?;
        let model_name = model.strip_prefix("codex:").unwrap_or("gpt-4o");
        openai::chat_openai_stream(model_name, history, &creds.api_key, tools, on_chunk).await
    } else if model.starts_with("antigravity") {
        antigravity::chat_stream("gemini-3-flash", history, tools, on_chunk).await
    } else if model.starts_with("openai:") {
        let keys = config::load_api_keys();
        let key = keys
            .openai
            .ok_or("LLM Error: No OpenAI API Key set. Please configure it in Settings.")?;
        let model_name = model.strip_prefix("openai:").unwrap_or("gpt-4");
        openai::chat_openai_stream(model_name, history, &key, tools, on_chunk).await
    } else if model.starts_with("gemini:") {
        let keys = config::load_api_keys();
        let key = keys
            .gemini
            .ok_or("LLM Error: No Gemini API Key set. Please configure it in Settings.")?;
        let model_name = model.strip_prefix("gemini:").unwrap_or("gemini-1.5-pro");
        gemini::chat_gemini_stream(model_name, history, &key, on_chunk).await
    } else {
        let ollama_url = config::get_ollama_url();
        ollama::chat_stream(&ollama_url, model, history, tools, on_chunk).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_429() {
        assert!(is_retryable("OpenAI API Error (429): rate limit exceeded"));
        assert!(is_retryable("HTTP 429 Too Many Requests"));
    }

    #[test]
    fn test_retryable_503() {
        assert!(is_retryable("Anthropic API Error (503): Service Unavailable"));
        assert!(is_retryable("503 overloaded"));
    }

    #[test]
    fn test_retryable_rate_limit() {
        assert!(is_retryable("rate_limit_exceeded"));
        assert!(is_retryable("Rate Limit Reached"));
    }

    #[test]
    fn test_retryable_capacity() {
        assert!(is_retryable("MODEL_CAPACITY_EXHAUSTED"));
        assert!(is_retryable("capacity unavailable"));
    }

    #[test]
    fn test_retryable_network_errors() {
        assert!(is_retryable("connection reset by peer"));
        assert!(is_retryable("Connection Closed before response"));
        assert!(is_retryable("broken pipe"));
        assert!(is_retryable("operation timed out"));
    }

    #[test]
    fn test_not_retryable_auth() {
        assert!(!is_retryable("Anthropic Auth Error: Token expired"));
        assert!(!is_retryable("401 Unauthorized"));
        assert!(!is_retryable("Invalid API key"));
    }

    #[test]
    fn test_not_retryable_bad_request() {
        assert!(!is_retryable("400 Bad Request: invalid model"));
        assert!(!is_retryable("Unsupported model"));
    }

    #[test]
    fn test_not_retryable_quota() {
        assert!(!is_retryable("insufficient_quota"));
        assert!(!is_retryable("billing limit reached"));
    }

    #[tokio::test]
    async fn test_with_retry_immediate_success() {
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();
        let result = with_retry("test", || {
            let cc = cc.clone();
            async move {
                cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(Message {
                    role: "assistant".to_string(),
                    content: "hello".to_string(),
                    tool_calls: None,
                })
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, "hello");
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_non_retryable_fails_immediately() {
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();
        let result = with_retry("test", || {
            let cc = cc.clone();
            async move {
                cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err::<Message, String>("401 Unauthorized".to_string())
            }
        })
        .await;

        assert!(result.is_err());
        // Should NOT retry for auth errors
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
