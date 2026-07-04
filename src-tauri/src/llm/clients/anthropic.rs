use crate::llm::stream_util::{next_chunk_with_timeout, STREAM_IDLE_TIMEOUT};
use crate::llm::{FunctionCall, Message, ToolCall, ToolDefinition};
use serde_json::json;
use std::pin::pin;

/// Anthropic allows up to 4 `cache_control` breakpoints per request. The system
/// block consumes one, leaving 3 for rolling tool_result breakpoints across turns.
const MAX_TOOL_RESULT_CACHE_BREAKPOINTS: usize = 3;

/// Server-side compaction (anthropic-beta: compact-2026-01-12) is supported on
/// 4.6+ Sonnet/Opus and 4.5+ Haiku. Older models error if the header is sent.
fn supports_compaction(model: &str) -> bool {
    let m = model.to_lowercase();
    m.contains("claude-opus-4-6")
        || m.contains("claude-opus-4-7")
        || m.contains("claude-opus-4-8")
        || m.contains("claude-sonnet-4-6")
        || m.contains("claude-sonnet-4-7")
        || m.contains("claude-sonnet-4-8")
        || m.contains("claude-haiku-4-5")
        || m.contains("claude-haiku-4-6")
}

/// Stamp `cache_control: ephemeral` on the last N tool_result blocks across
/// `api_messages`. As the conversation grows the breakpoints move forward,
/// so each new turn finds the previous turn's prefix still cached.
fn add_cache_breakpoints_to_tool_results(messages: &mut [serde_json::Value], max: usize) {
    let mut tool_result_indices: Vec<(usize, usize)> = Vec::new();
    for (msg_idx, msg) in messages.iter().enumerate() {
        if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
            for (block_idx, block) in content.iter().enumerate() {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                    tool_result_indices.push((msg_idx, block_idx));
                }
            }
        }
    }

    let to_stamp: Vec<(usize, usize)> = tool_result_indices
        .into_iter()
        .rev()
        .take(max)
        .collect();

    for (msg_idx, block_idx) in to_stamp {
        if let Some(content) = messages[msg_idx]
            .get_mut("content")
            .and_then(|c| c.as_array_mut())
        {
            if let Some(block) = content.get_mut(block_idx) {
                if let Some(obj) = block.as_object_mut() {
                    obj.insert("cache_control".to_string(), json!({"type": "ephemeral"}));
                }
            }
        }
    }
}

pub async fn chat_anthropic_stream(
    model: &str,
    messages: Vec<Message>,
    access_token: &str,
    tools: Option<Vec<ToolDefinition>>,
    callback: impl Fn(String) + Send + 'static,
) -> Result<Message, String> {
    // Use the shared connection-pooled client. The old per-call builder set a
    // 120s global timeout, which would kill long streaming responses; the
    // shared client has no global timeout — per-chunk idle is enforced by
    // `stream_util::next_chunk_with_timeout` instead.
    let client = crate::llm::http::shared_client();

    // Separate system message from the rest
    let mut system_text = String::new();
    let mut api_messages: Vec<serde_json::Value> = Vec::new();

    for msg in &messages {
        match msg.role.as_str() {
            "system" => {
                if !system_text.is_empty() {
                    system_text.push_str("\n\n");
                }
                system_text.push_str(&msg.content);
            }
            "assistant" => {
                // Build content blocks
                let mut content_blocks: Vec<serde_json::Value> = Vec::new();

                // Anthropic rejects assistant messages with trailing whitespace
                let trimmed = msg.content.trim_end();
                if !trimmed.is_empty() {
                    content_blocks.push(json!({ "type": "text", "text": trimmed }));
                }

                // Convert tool_calls to tool_use blocks
                if let Some(tool_calls) = &msg.tool_calls {
                    for (i, tc) in tool_calls.iter().enumerate() {
                        let tool_id = tc
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("toolu_{}", i));
                        content_blocks.push(json!({
                            "type": "tool_use",
                            "id": tool_id,
                            "name": tc.function.name,
                            "input": tc.function.arguments
                        }));
                    }
                }

                if content_blocks.is_empty() {
                    content_blocks.push(json!({ "type": "text", "text": "" }));
                }

                api_messages.push(json!({
                    "role": "assistant",
                    "content": content_blocks
                }));
            }
            "tool" => {
                // Tool results — correlate with the preceding tool_use id. If
                // the message carries a tool_call with an id, use it directly.
                // Otherwise scan the last assistant message; if there is no
                // preceding tool_use at all, fail loudly rather than fabricate
                // a bogus id that Anthropic 400s on with cryptic wording.
                let tool_use_id = match msg
                    .tool_calls
                    .as_ref()
                    .and_then(|tc| tc.first())
                    .and_then(|tc| tc.id.clone())
                {
                    Some(id) => id,
                    None => match find_last_tool_use_id(&api_messages) {
                        Some(id) => id,
                        None => {
                            return Err(
                                "Conversation has a tool_result with no preceding tool_use. \
                                 This is a client-side correlation bug — please file a report \
                                 with the failing prompt."
                                    .to_string(),
                            );
                        }
                    },
                };
                api_messages.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": msg.content
                    }]
                }));
            }
            _ => {
                // user messages
                api_messages.push(json!({
                    "role": "user",
                    "content": msg.content
                }));
            }
        }
    }

    // Build tools array for Anthropic format
    let api_tools: Option<Vec<serde_json::Value>> = tools.map(|t| {
        t.into_iter()
            .map(|tool| {
                json!({
                    "name": tool.function.name,
                    "description": tool.function.description,
                    "input_schema": tool.function.parameters
                })
            })
            .collect()
    });

    // Rolling cache breakpoints: stamp the most recent tool_result blocks so
    // each subsequent turn finds the previous prefix already cached.
    add_cache_breakpoints_to_tool_results(&mut api_messages, MAX_TOOL_RESULT_CACHE_BREAKPOINTS);

    let compaction_enabled = supports_compaction(model);

    let mut body = json!({
        "model": model,
        "messages": api_messages,
        "max_tokens": 4096,
        "stream": true
    });

    if !system_text.is_empty() {
        // Use structured system prompt with cache_control for Anthropic prompt caching.
        // The system prompt is stable across turns, so caching saves tokens on multi-turn conversations.
        body["system"] = json!([{
            "type": "text",
            "text": system_text,
            "cache_control": { "type": "ephemeral" }
        }]);
    }

    if let Some(tools) = api_tools {
        body["tools"] = json!(tools);
    }

    if compaction_enabled {
        // Server-side context compaction: when the request exceeds 50k input tokens,
        // Anthropic compresses earlier turns into a summary so the agent keeps running
        // without client-side history management.
        body["context_management"] = json!({
            "edits": [{
                "type": "compact_20260112",
                "trigger": { "type": "input_tokens", "value": 50_000 }
            }]
        });
    }

    // Direct Anthropic path is API-key-only. The Claude Code OAuth shortcut
    // was removed in the OpenRouter migration: Anthropic's anti-abuse
    // throttling on consumer OAuth (the bare "Error" 429s shuttle-io kept
    // hitting) wasn't worth keeping alive. Users who want frontier Claude
    // models go through OpenRouter; users who want prompt caching paste a
    // real API key (sk-ant-api-*) into Settings.
    let beta_header = if compaction_enabled {
        "prompt-caching-2024-07-31,compact-2026-01-12"
    } else {
        "prompt-caching-2024-07-31"
    };

    let req = client
        .post("https://api.anthropic.com/v1/messages")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-api-key", access_token)
        .header("anthropic-beta", beta_header);

    let res = req
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Anthropic Request Failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        if status.as_u16() == 401 {
            return Err(
                "Anthropic Auth Error: API key invalid or expired. Update it in Settings → Cloud Providers."
                    .to_string(),
            );
        }
        return Err(format!("Anthropic API Error ({}): {}", status, text));
    }

    let mut stream = pin!(res.bytes_stream());
    let mut full_content = String::new();
    let mut buffer = String::new();

    // Track tool_use blocks being built
    let mut current_tool_id: Option<String> = None;
    let mut current_tool_name: Option<String> = None;
    let mut current_tool_input = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    while let Some(chunk) = next_chunk_with_timeout(&mut stream, STREAM_IDLE_TIMEOUT).await? {
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
                    let event_type = data.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    match event_type {
                        "content_block_start" => {
                            if let Some(block) = data.get("content_block") {
                                let block_type =
                                    block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                if block_type == "tool_use" {
                                    current_tool_id = block
                                        .get("id")
                                        .and_then(|v| v.as_str())
                                        .map(String::from);
                                    current_tool_name = block
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .map(String::from);
                                    current_tool_input.clear();
                                }
                            }
                        }
                        "content_block_delta" => {
                            if let Some(delta) = data.get("delta") {
                                let delta_type =
                                    delta.get("type").and_then(|t| t.as_str()).unwrap_or("");

                                match delta_type {
                                    "text_delta" => {
                                        if let Some(text) =
                                            delta.get("text").and_then(|t| t.as_str())
                                        {
                                            if !text.is_empty() {
                                                full_content.push_str(text);
                                                callback(text.to_string());
                                            }
                                        }
                                    }
                                    "input_json_delta" => {
                                        if let Some(partial) =
                                            delta.get("partial_json").and_then(|p| p.as_str())
                                        {
                                            current_tool_input.push_str(partial);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "content_block_stop" => {
                            // Finalize tool call if we were building one
                            if let (Some(id), Some(name)) =
                                (current_tool_id.take(), current_tool_name.take())
                            {
                                let arguments: serde_json::Value =
                                    serde_json::from_str(&current_tool_input)
                                        .unwrap_or(json!({}));
                                tool_calls.push(ToolCall {
                                    id: Some(id),
                                    function: FunctionCall { name, arguments },
                                });
                                current_tool_input.clear();
                            }
                        }
                        "message_stop" | "message_delta" => {
                            // End of message
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(Message {
        role: "assistant".to_string(),
        content: full_content,
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        synthetic: false,
    })
}

/// Find the last tool_use block's id in api_messages for tool_result correlation.
/// Returns `None` if there is no preceding tool_use — callers can use this to
/// produce a clear error instead of fabricating an id that Anthropic will
/// reject with HTTP 400 ("tool_use_id not found").
fn find_last_tool_use_id(api_messages: &[serde_json::Value]) -> Option<String> {
    for msg in api_messages.iter().rev() {
        if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
            for block in content.iter().rev() {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    if let Some(id) = block.get("id").and_then(|i| i.as_str()) {
                        return Some(id.to_string());
                    }
                }
            }
        }
    }
    None
}
