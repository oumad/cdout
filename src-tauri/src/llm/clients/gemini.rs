use crate::llm::Message;
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
