//! OpenRouter client — one OpenAI-compatible endpoint, 400+ models.
//!
//! Replaces the previous hand-rolled clients for OpenAI, Gemini, Antigravity,
//! and the Claude-Code OAuth path. The native Anthropic client (`anthropic.rs`)
//! stays for users who want first-class prompt caching, which OpenRouter's
//! OpenAI-compat wire drops on the floor.
//!
//! Design choices (see verdicts in the migration research):
//! - Pin provider routing on every request to dodge silent FP4/Int4 swaps and
//!   provider quality drift. `allow_fallbacks: false` for predictability.
//! - Always send `usage: { include: true }` so future cost tracking is one
//!   parser change away.
//! - No `HTTP-Referer` / `X-Title` headers — privacy default; we don't broadcast
//!   shuttle-io's request volume to the OpenRouter leaderboard.
//! - Distinguish opaque OpenRouter infra failures from credential issues so the
//!   UI can show "service issue, not your key" (Feb 2026 OR bug pattern).

use crate::llm::stream_util::{next_chunk_with_timeout, STREAM_IDLE_TIMEOUT};
use crate::llm::{FunctionCall, Message, ToolCall, ToolDefinition};
use serde_json::json;
use std::pin::pin;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// Provider-routing preferences sent on every request. Hard-coded for now —
/// the user can adjust via a future Settings toggle when the need arises.
fn provider_preferences() -> serde_json::Value {
    json!({
        "allow_fallbacks": false,
        "quantizations": ["fp8", "bf16", "fp16"],
        "data_collection": "deny"
    })
}

fn build_body(
    model: &str,
    messages: &[Message],
    tools: &Option<Vec<ToolDefinition>>,
    stream: bool,
) -> serde_json::Value {
    // Strip the synthetic flag so it never reaches an upstream provider.
    let api_messages = Message::strip_synthetic_for_api(messages);
    let mut body = json!({
        "model": model,
        "messages": api_messages,
        "temperature": 0.7,
        "stream": stream,
        "provider": provider_preferences(),
        "usage": { "include": true }
    });
    if let Some(t) = tools {
        body["tools"] = json!(t);
    }
    body
}

/// Classify a failing OpenRouter HTTP response. Some failure modes are well
/// known and need user-facing distinction (e.g. the Feb 2026 bug where OR
/// returned 401 "User not found" during infra outages — users blame the app
/// when they shouldn't).
fn explain_failure(status: reqwest::StatusCode, body: &str) -> String {
    let s = status.as_u16();
    if s == 401 {
        // OpenRouter has been observed returning 401 with cryptic body during
        // infra incidents. Surface both reads.
        if body.to_lowercase().contains("user not found")
            || body.to_lowercase().contains("invalid api key")
        {
            format!(
                "OpenRouter authentication failed ({}). Either your key is invalid \
                 OR OpenRouter is experiencing a service issue (they have been known \
                 to return 401 during outages — check https://status.openrouter.ai). Body: {}",
                s, body
            )
        } else {
            format!("OpenRouter Auth Error ({}): {}", s, body)
        }
    } else if s == 402 {
        format!(
            "OpenRouter Payment Required (402): credits exhausted. Top up at \
             https://openrouter.ai/credits. Body: {}",
            body
        )
    } else if s == 429 {
        format!("OpenRouter Rate Limit (429): {}", body)
    } else if s >= 500 {
        format!(
            "OpenRouter Service Issue ({}): not a credential problem — the gateway \
             returned a {} status. Body: {}",
            s, s, body
        )
    } else {
        format!("OpenRouter API Error ({}): {}", s, body)
    }
}

// --- Streaming ---

pub async fn chat_openrouter_stream(
    model: &str,
    messages: Vec<Message>,
    api_key: &str,
    tools: Option<Vec<ToolDefinition>>,
    callback: impl Fn(String) + Send + 'static,
) -> Result<Message, String> {
    let client = crate::llm::http::shared_client();
    let body = build_body(model, &messages, &tools, true);

    let res = client
        .post(OPENROUTER_URL)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenRouter request failed: {}", e))?;
    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(explain_failure(status, &text));
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

            // OpenRouter interleaves `: ping` keepalive comments — skip them.
            if line.is_empty() || line.starts_with(':') || line == "data: [DONE]" {
                continue;
            }

            let json_str = match line.strip_prefix("data: ") {
                Some(s) => s,
                None => continue,
            };
            let data: serde_json::Value = match serde_json::from_str(json_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Provider failures are sometimes delivered as mid-stream `error`
            // frames or as a final chunk with finish_reason: "error". Both
            // would otherwise be silently dropped by the delta-parsing path
            // below, returning Ok with empty content.
            if let Some(err) = data.get("error") {
                return Err(format!("OpenRouter stream error: {}", err));
            }
            if data["choices"][0].get("finish_reason").and_then(|v| v.as_str()) == Some("error") {
                return Err(format!(
                    "OpenRouter stream finished with error: {}",
                    data["choices"][0]
                ));
            }

            let delta = &data["choices"][0]["delta"];

            // Text content
            if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                if !text.is_empty() {
                    full_content.push_str(text);
                    callback(text.to_string());
                }
            }

            // Tool call deltas (OpenAI shape)
            if let Some(tc_arr) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tc_arr {
                    let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    let entry = tool_calls_acc
                        .entry(idx)
                        .or_insert_with(|| (String::new(), String::new(), String::new()));
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

    let tool_calls = collect_tool_calls(tool_calls_acc);

    // Stream closed cleanly but produced nothing. Surface as a real error so
    // the user sees "OpenRouter returned empty response" instead of a blank
    // assistant turn. The classifier maps "empty response" → EmptyCompletion,
    // which the non-streaming retry path would re-roll once; streaming doesn't
    // retry but at least the user sees an actionable message.
    if full_content.is_empty() && tool_calls.is_none() {
        return Err("OpenRouter empty response: stream closed with no content or tool_calls. Likely an upstream provider hiccup — try again.".to_string());
    }

    Ok(Message {
        role: "assistant".to_string(),
        content: full_content,
        tool_calls,
        synthetic: false,
    })
}

fn collect_tool_calls(
    acc: std::collections::HashMap<usize, (String, String, String)>,
) -> Option<Vec<ToolCall>> {
    if acc.is_empty() {
        return None;
    }
    let mut calls: Vec<(usize, ToolCall)> = acc
        .into_iter()
        .map(|(idx, (id, name, args_str))| {
            let arguments: serde_json::Value =
                serde_json::from_str(&args_str).unwrap_or(json!({}));
            (
                idx,
                ToolCall {
                    id: if id.is_empty() { None } else { Some(id) },
                    function: FunctionCall { name, arguments },
                },
            )
        })
        .collect();
    calls.sort_by_key(|(idx, _)| *idx);
    Some(calls.into_iter().map(|(_, tc)| tc).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_includes_provider_pinning() {
        let body = build_body("anthropic/claude-sonnet-4.6", &[], &None, true);
        let pin = &body["provider"];
        assert_eq!(pin["allow_fallbacks"], false);
        let quant = pin["quantizations"].as_array().unwrap();
        assert!(quant.iter().any(|q| q == "fp8"));
        assert!(quant.iter().any(|q| q == "bf16"));
        assert!(quant.iter().any(|q| q == "fp16"));
        assert_eq!(pin["data_collection"], "deny");
    }

    #[test]
    fn body_requests_usage_for_cost_tracking() {
        let body = build_body("any/model", &[], &None, false);
        assert_eq!(body["usage"]["include"], true);
    }

    #[test]
    fn body_propagates_tools_when_present() {
        let tool = ToolDefinition {
            r#type: "function".to_string(),
            function: crate::llm::ToolFunction {
                name: "run_powershell".to_string(),
                description: "test".to_string(),
                parameters: json!({}),
            },
        };
        let body = build_body("x/y", &[], &Some(vec![tool]), false);
        assert!(body["tools"].is_array());
    }

    #[test]
    fn body_omits_tools_when_none() {
        let body = build_body("x/y", &[], &None, false);
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn explain_failure_5xx_is_service_issue() {
        let msg = explain_failure(reqwest::StatusCode::BAD_GATEWAY, "upstream timeout");
        assert!(msg.contains("Service Issue"));
        assert!(msg.contains("not a credential problem"));
    }

    #[test]
    fn explain_failure_401_user_not_found_warns_about_outage() {
        let msg = explain_failure(reqwest::StatusCode::UNAUTHORIZED, "user not found");
        assert!(msg.contains("status.openrouter.ai"));
    }

    #[test]
    fn explain_failure_402_points_to_billing() {
        let msg = explain_failure(
            reqwest::StatusCode::PAYMENT_REQUIRED,
            "{\"error\":\"out of credits\"}",
        );
        assert!(msg.contains("Payment Required"));
        assert!(msg.contains("openrouter.ai/credits"));
    }

    /// Helper: simulate parsing a single SSE `data:` JSON line by mirroring
    /// the per-line logic from chat_openrouter_stream. Returns either an
    /// error (matching the early-return branches) or Ok(()) if the line was
    /// parsed normally.
    fn classify_sse_line(line: &str) -> Result<(), String> {
        let json_str = match line.strip_prefix("data: ") {
            Some(s) => s,
            None => return Ok(()),
        };
        let data: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
        if let Some(err) = data.get("error") {
            return Err(format!("OpenRouter stream error: {}", err));
        }
        if data["choices"][0].get("finish_reason").and_then(|v| v.as_str()) == Some("error") {
            return Err(format!(
                "OpenRouter stream finished with error: {}",
                data["choices"][0]
            ));
        }
        Ok(())
    }

    #[test]
    fn stream_error_frame_is_surfaced() {
        let line = r#"data: {"error":{"message":"upstream timeout","code":502}}"#;
        let r = classify_sse_line(line);
        assert!(r.is_err(), "stream error frame must surface");
        assert!(r.unwrap_err().contains("upstream timeout"));
    }

    #[test]
    fn stream_finish_reason_error_is_surfaced() {
        let line = r#"data: {"choices":[{"finish_reason":"error","delta":{}}]}"#;
        let r = classify_sse_line(line);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("stream finished with error"));
    }

    #[test]
    fn normal_delta_passes_classification() {
        let line = r#"data: {"choices":[{"delta":{"content":"hi"}}]}"#;
        assert!(classify_sse_line(line).is_ok());
    }

    #[test]
    fn collect_tool_calls_sorts_by_index() {
        let mut acc = std::collections::HashMap::new();
        acc.insert(
            1,
            (
                "id1".to_string(),
                "second".to_string(),
                "{\"x\":2}".to_string(),
            ),
        );
        acc.insert(
            0,
            (
                "id0".to_string(),
                "first".to_string(),
                "{\"x\":1}".to_string(),
            ),
        );
        let out = collect_tool_calls(acc).unwrap();
        assert_eq!(out[0].function.name, "first");
        assert_eq!(out[1].function.name, "second");
    }
}
