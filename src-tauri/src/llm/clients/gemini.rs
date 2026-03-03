use crate::llm::Message;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::json;

pub async fn chat_gemini(
    model: &str,
    messages: Vec<Message>,
    api_key: &str,
) -> Result<Message, String> {
    // model e.g. "gemini-1.5-pro"
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let client = Client::new();

    // Convert messages to Gemini format
    let contents: Vec<serde_json::Value> = messages
        .iter()
        .map(|msg| {
            json!({
                "role": if msg.role == "assistant" { "model" } else { "user" },
                "parts": [{ "text": msg.content }]
            })
        })
        .collect();

    let body = json!({
        "contents": contents,
        "generationConfig": {
            "temperature": 0.7
        }
    });

    let res = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini Request Failed: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Gemini API Error: {}", text));
    }

    let json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

    let content = json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(Message {
        role: "assistant".to_string(),
        content,
        tool_calls: None,
    })
}

pub async fn chat_gemini_stream(
    model: &str,
    messages: Vec<Message>,
    api_key: &str,
    callback: impl Fn(String) + Send + 'static,
) -> Result<Message, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
        model, api_key
    );

    let client = Client::new();

    let contents: Vec<serde_json::Value> = messages
        .iter()
        .map(|msg| {
            json!({
                "role": if msg.role == "assistant" { "model" } else { "user" },
                "parts": [{ "text": msg.content }]
            })
        })
        .collect();

    let body = json!({
        "contents": contents,
        "generationConfig": {
            "temperature": 0.7
        }
    });

    let res = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini Request Failed: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Gemini API Error: {}", text));
    }

    let mut stream = res.bytes_stream();
    let mut full_content = String::new();
    let mut buffer = String::new();

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| format!("Stream error: {}", e))?;
        let s = String::from_utf8_lossy(&chunk);
        buffer.push_str(&s);

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            if let Some(json_str) = line.strip_prefix("data: ") {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(text) = data["candidates"][0]["content"]["parts"][0]["text"].as_str()
                    {
                        if !text.is_empty() {
                            full_content.push_str(text);
                            callback(text.to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(Message {
        role: "assistant".to_string(),
        content: full_content,
        tool_calls: None, // Gemini via direct API doesn't support tool calls yet
    })
}
