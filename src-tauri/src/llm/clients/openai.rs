use crate::llm::{FunctionCall, Message, ToolCall};
use reqwest::Client;
use serde_json::json;

pub async fn chat_openai(
    model: &str,
    messages: Vec<Message>,
    api_key: &str,
) -> Result<Message, String> {
    let client = Client::new();
    let url = "https://api.openai.com/v1/chat/completions";

    let body = json!({
        "model": model,
        "messages": messages,
        "temperature": 0.7
    });

    let res = client
        .post(url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI Request Failed: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        if text.contains("insufficient_quota") {
            return Err("OpenAI Quota Exceeded: Your API credit balance is zero or expired. Please add credits at platform.openai.com/billing.".to_string());
        }
        return Err(format!("OpenAI API Error: {}", text));
    }

    let json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

    parse_openai_response(&json)
}

/// Parse an OpenAI-compatible chat completion response, including tool_calls.
fn parse_openai_response(json: &serde_json::Value) -> Result<Message, String> {
    let message = &json["choices"][0]["message"];

    let content = message["content"].as_str().unwrap_or("").to_string();

    // Parse tool_calls if present
    let tool_calls = message.get("tool_calls").and_then(|tc| {
        tc.as_array().map(|arr| {
            arr.iter()
                .filter_map(|call| {
                    let func = call.get("function")?;
                    let name = func.get("name")?.as_str()?.to_string();
                    let arguments_str = func.get("arguments")?.as_str().unwrap_or("{}");
                    let arguments: serde_json::Value =
                        serde_json::from_str(arguments_str).unwrap_or(json!({}));
                    Some(ToolCall {
                        function: FunctionCall { name, arguments },
                    })
                })
                .collect::<Vec<_>>()
        })
    });

    // Only set tool_calls if non-empty
    let tool_calls = tool_calls.and_then(|v| if v.is_empty() { None } else { Some(v) });

    Ok(Message {
        role: "assistant".to_string(),
        content,
        tool_calls,
    })
}
