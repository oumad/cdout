//! Three-branch model dispatch.
//!
//! - `ollama:*` -> local Ollama (non-negotiable for local privacy)
//! - `anthropic:*` -> direct Anthropic API for prompt-caching path (optional;
//!   requires an API key in Settings)
//! - everything else -> OpenRouter (the default cloud gateway)
//!
//! The retry wrapper + history-recovery layer is provider-agnostic and lives
//! in `llm::retry` + `llm::history`. This file is just the dispatch table.

use crate::llm::clients::{anthropic, ollama, openrouter};
use crate::llm::{Message, ToolDefinition};
use crate::utils::config;

#[derive(Debug, PartialEq, Eq)]
enum Provider {
    Ollama(String),
    AnthropicDirect(String),
    OpenRouter(String),
}

fn route_provider(model: &str) -> Provider {
    if let Some(rest) = model.strip_prefix("ollama:") {
        Provider::Ollama(rest.to_string())
    } else if let Some(rest) = model.strip_prefix("anthropic:") {
        // Allow either "anthropic:claude-opus-4-7" (bare) or
        // "anthropic:anthropic/claude-opus-4-7" (OR-style slug) — strip the
        // redundant prefix so we always call the API with the bare model id.
        let bare = rest.strip_prefix("anthropic/").unwrap_or(rest);
        Provider::AnthropicDirect(bare.to_string())
    } else {
        Provider::OpenRouter(model.to_string())
    }
}

fn ollama_url() -> String {
    config::get_ollama_url()
}

fn openrouter_key() -> Result<String, String> {
    config::load_api_keys()
        .openrouter
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| {
            "OpenRouter API key not configured. Add one in Settings → Cloud Providers, or use an ollama:* model."
                .to_string()
        })
}

fn anthropic_key() -> Result<String, String> {
    config::load_api_keys()
        .anthropic
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| {
            "Anthropic API key not configured. Add one in Settings → Cloud Providers, or pick the same model through OpenRouter."
                .to_string()
        })
}

pub async fn route_chat_stream(
    model: &str,
    history: Vec<Message>,
    tools: Option<Vec<ToolDefinition>>,
    on_chunk: impl Fn(String) + Send + 'static,
) -> Result<Message, String> {
    // Strip the internal `synthetic` flag once, centrally, before ANY provider
    // sees the payload. Synthetic nudges travel as role:user so the model
    // treats them as user input, but the `synthetic` marker must never reach a
    // provider API (some reject unknown fields). Doing it here means every
    // client — ollama (serializes raw Message), openrouter, anthropic — is
    // covered without each having to remember.
    let history = Message::strip_synthetic_for_api(&history);

    // Streaming calls are NOT retried: the callback may already have emitted
    // partial text, so a wholesale re-send would duplicate output. Transient
    // failures surface as an error; context-window overflow is recovered by
    // the trim ladder in `agent::call_stream_with_context_recovery`. The
    // per-chunk idle timeout in stream_util handles stalled streams.
    match route_provider(model) {
        Provider::Ollama(name) => {
            let url = ollama_url();
            ollama::chat_stream(&url, &name, history, tools, on_chunk).await
        }
        Provider::AnthropicDirect(name) => {
            let key = anthropic_key()?;
            anthropic::chat_anthropic_stream(&name, history, &key, tools, on_chunk).await
        }
        Provider::OpenRouter(name) => {
            let key = openrouter_key()?;
            openrouter::chat_openrouter_stream(&name, history, &key, tools, on_chunk).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_ollama_prefix() {
        assert_eq!(
            route_provider("ollama:llama3"),
            Provider::Ollama("llama3".to_string())
        );
    }

    #[test]
    fn routes_anthropic_direct_prefix() {
        assert_eq!(
            route_provider("anthropic:claude-opus-4-7"),
            Provider::AnthropicDirect("claude-opus-4-7".to_string())
        );
    }

    #[test]
    fn routes_anthropic_strips_redundant_or_style_slug() {
        // anthropic:anthropic/claude-opus-4-7 → claude-opus-4-7 (one prefix)
        assert_eq!(
            route_provider("anthropic:anthropic/claude-opus-4-7"),
            Provider::AnthropicDirect("claude-opus-4-7".to_string())
        );
    }

    #[test]
    fn routes_bare_anthropic_slug_to_openrouter() {
        // OR-style slug without the anthropic: prefix should go via OpenRouter.
        assert_eq!(
            route_provider("anthropic/claude-opus-4-7"),
            Provider::OpenRouter("anthropic/claude-opus-4-7".to_string())
        );
    }

    #[test]
    fn routes_openai_slug_to_openrouter() {
        assert_eq!(
            route_provider("openai/gpt-5"),
            Provider::OpenRouter("openai/gpt-5".to_string())
        );
    }

    #[test]
    fn routes_google_slug_to_openrouter() {
        assert_eq!(
            route_provider("google/gemini-3-pro-preview"),
            Provider::OpenRouter("google/gemini-3-pro-preview".to_string())
        );
    }
}
