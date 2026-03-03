use crate::auth::cli_credentials;
use crate::llm::clients::{anthropic, antigravity, gemini, ollama, openai};
use crate::llm::{Message, ToolDefinition};
use crate::utils::config;

pub async fn route_chat(
    model: &str,
    history: Vec<Message>,
    tools: Option<Vec<ToolDefinition>>,
) -> Result<Message, String> {
    if model.starts_with("claude:") {
        let creds = cli_credentials::read_claude_code_credentials()
            .ok_or("Claude Code credentials not found. Run `claude` to log in.")?;
        let model_name = model.strip_prefix("claude:").unwrap_or("claude-sonnet-4-20250514");
        anthropic::chat_anthropic_stream(model_name, history, &creds.access_token, tools, |_| {})
            .await
    } else if model.starts_with("codex:") {
        let creds = cli_credentials::read_codex_credentials()
            .ok_or("Codex CLI credentials not found. Set OPENAI_API_KEY or install Codex CLI.")?;
        let model_name = model.strip_prefix("codex:").unwrap_or("gpt-4o");
        openai::chat_openai(model_name, history, &creds.api_key).await
    } else if model.starts_with("antigravity") {
        antigravity::chat_stream("gemini-3-flash", history, tools, |_| {}).await
    } else if model.starts_with("openai:") {
        let keys = config::load_api_keys();
        let key = keys
            .openai
            .ok_or("LLM Error: No OpenAI API Key set. Please configure it in Settings.")?;
        let model_name = model.strip_prefix("openai:").unwrap_or("gpt-4");
        openai::chat_openai(model_name, history, &key).await
    } else if model.starts_with("gemini:") {
        let keys = config::load_api_keys();
        let key = keys
            .gemini
            .ok_or("LLM Error: No Gemini API Key set. Please configure it in Settings.")?;
        let model_name = model.strip_prefix("gemini:").unwrap_or("gemini-1.5-pro");
        gemini::chat_gemini(model_name, history, &key).await
    } else {
        let ollama_url = config::get_ollama_url();
        ollama::chat(&ollama_url, model, history, tools).await
    }
}

pub async fn route_chat_stream(
    model: &str,
    history: Vec<Message>,
    tools: Option<Vec<ToolDefinition>>,
    on_chunk: impl Fn(String) + Send + 'static,
) -> Result<Message, String> {
    if model.starts_with("claude:") {
        let creds = cli_credentials::read_claude_code_credentials()
            .ok_or("Claude Code credentials not found. Run `claude` to log in.")?;
        let model_name = model.strip_prefix("claude:").unwrap_or("claude-sonnet-4-20250514");
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
