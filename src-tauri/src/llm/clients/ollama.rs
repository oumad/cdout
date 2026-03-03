use crate::llm::{Message, ToolDefinition};
use futures_util::StreamExt;
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

pub async fn chat_stream(
    ollama_url: &str,
    model: &str,
    messages: Vec<Message>,
    tools: Option<Vec<ToolDefinition>>,
    callback: impl Fn(String) + Send + 'static,
) -> Result<Message, String> {
    let client = reqwest::Client::new();

    let request_body = ChatRequest {
        model: model.to_string(),
        messages,
        stream: true,
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

    let mut stream = response.bytes_stream();
    let mut full_content = String::new();
    let mut final_message: Option<Message> = None;
    let mut buffer = String::new();

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| format!("Stream error: {}", e))?;
        let s = String::from_utf8_lossy(&chunk);
        buffer.push_str(&s);

        // Ollama streams NDJSON — one JSON object per line
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            if let Ok(resp) = serde_json::from_str::<ChatResponse>(&line) {
                if resp.done {
                    // Final chunk contains the complete message with tool_calls
                    final_message = Some(resp.message);
                } else {
                    let text = &resp.message.content;
                    if !text.is_empty() {
                        full_content.push_str(text);
                        callback(text.clone());
                    }
                }
            }
        }
    }

    // If we got a final message (done:true), use it — it has tool_calls
    if let Some(mut msg) = final_message {
        // The final message may have empty content; merge streamed content
        if msg.content.is_empty() && !full_content.is_empty() {
            msg.content = full_content;
        }
        Ok(msg)
    } else {
        // Fallback: build message from accumulated content
        Ok(Message {
            role: "assistant".to_string(),
            content: full_content,
            tool_calls: None,
        })
    }
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
