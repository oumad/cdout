use crate::llm::{Message, ToolDefinition};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct ChatResponse {
    pub model: String,
    pub message: Message,
    pub done: bool,
}

#[derive(Deserialize, Debug)]
struct ModelListResponse {
    models: Vec<ModelInfo>,
}

#[derive(Deserialize, Debug)]
struct ModelInfo {
    name: String,
}

pub async fn chat(
    ollama_url: &str,
    model: &str,
    messages: Vec<Message>,
    tools: Option<Vec<ToolDefinition>>,
) -> Result<Message, String> {
    let client = reqwest::Client::new();

    let request_body = ChatRequest {
        model: model.to_string(),
        messages,
        stream: false, // Agent loop easier without streaming for now
        tools,
    };

    let url = format!("{}/api/chat", ollama_url.trim_end_matches('/'));
    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Ollama API Error ({}): {}", status, error_text));
    }

    let chat_response: ChatResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(chat_response.message)
}

pub async fn list_models(ollama_url: &str) -> Result<Vec<String>, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/tags", ollama_url.trim_end_matches('/'));
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error connecting to Ollama: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Ollama API Error ({}): {}", status, error_text));
    }

    let model_list: ModelListResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse models response: {}", e))?;

    Ok(model_list.models.into_iter().map(|m| m.name).collect())
}
