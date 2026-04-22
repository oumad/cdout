use crate::llm::stream_util::{next_chunk_with_timeout, STREAM_IDLE_TIMEOUT};
use crate::llm::{FunctionCall, Message, ToolCall};
use reqwest::Client;
use serde_json::json;
use std::pin::pin;

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

pub async fn chat_openai_stream(
    model: &str,
    messages: Vec<Message>,
    api_key: &str,
    tools: Option<Vec<crate::llm::ToolDefinition>>,
    callback: impl Fn(String) + Send + 'static,
) -> Result<Message, String> {
    let client = Client::new();
    let url = "https://api.openai.com/v1/chat/completions";

    let mut body = json!({
        "model": model,
        "messages": messages,
        "temperature": 0.7,
        "stream": true
    });

    if let Some(t) = &tools {
        body["tools"] = json!(t);
    }

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
            return Err("OpenAI Quota Exceeded: Your API credit balance is zero or expired.".to_string());
        }
        return Err(format!("OpenAI API Error: {}", text));
    }

    let mut stream = pin!(res.bytes_stream());
    let mut full_content = String::new();
    let mut buffer = String::new();

    // Accumulate tool call deltas: index -> (id, name, arguments_str)
    let mut tool_calls_acc: std::collections::HashMap<usize, (String, String, String)> =
        std::collections::HashMap::new();

    while let Some(chunk) = next_chunk_with_timeout(&mut stream, STREAM_IDLE_TIMEOUT).await? {
        let s = String::from_utf8_lossy(&chunk);
        buffer.push_str(&s);

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.is_empty() || line == "data: [DONE]" {
                continue;
            }

            if let Some(json_str) = line.strip_prefix("data: ") {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let delta = &data["choices"][0]["delta"];

                    // Text content delta
                    if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                        if !text.is_empty() {
                            full_content.push_str(text);
                            callback(text.to_string());
                        }
                    }

                    // Tool call deltas
                    if let Some(tc_arr) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                        for tc in tc_arr {
                            let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                            let entry = tool_calls_acc.entry(idx).or_insert_with(|| (String::new(), String::new(), String::new()));

                            // Capture tool call ID from first delta
                            if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                if entry.0.is_empty() {
                                    entry.0 = id.to_string();
                                }
                            }

                            if let Some(func) = tc.get("function") {
                                if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                    entry.1.push_str(name);
                                }
                                if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                    entry.2.push_str(args);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Build tool_calls from accumulated deltas
    let tool_calls = if tool_calls_acc.is_empty() {
        None
    } else {
        let mut calls: Vec<(usize, ToolCall)> = tool_calls_acc
            .into_iter()
            .map(|(idx, (id, name, args_str))| {
                let arguments: serde_json::Value =
                    serde_json::from_str(&args_str).unwrap_or(json!({}));
                (idx, ToolCall {
                    id: if id.is_empty() { None } else { Some(id) },
                    function: FunctionCall { name, arguments },
                })
            })
            .collect();
        calls.sort_by_key(|(idx, _)| *idx);
        Some(calls.into_iter().map(|(_, tc)| tc).collect())
    };

    Ok(Message {
        role: "assistant".to_string(),
        content: full_content,
        tool_calls,
    })
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
                    let id = call.get("id").and_then(|i| i.as_str()).map(String::from);
                    let func = call.get("function")?;
                    let name = func.get("name")?.as_str()?.to_string();
                    let arguments_str = func.get("arguments")?.as_str().unwrap_or("{}");
                    let arguments: serde_json::Value =
                        serde_json::from_str(arguments_str).unwrap_or(json!({}));
                    Some(ToolCall {
                        id,
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
