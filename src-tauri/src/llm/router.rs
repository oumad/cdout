use crate::llm::clients::{antigravity, gemini, ollama, openai};
use crate::llm::{Message, ToolDefinition};
use crate::utils::config;

pub async fn route_chat(
    model: &str,
    history: Vec<Message>,
    tools: Option<Vec<ToolDefinition>>,
) -> Result<Message, String> {
    if model.starts_with("antigravity") {
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
