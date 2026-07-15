use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod clients;
pub mod history;
pub mod http;
pub mod retry;
pub mod router;
pub mod stream_util;

// Streaming types for Tauri Channel
#[derive(Serialize, Clone)]
#[serde(tag = "kind")]
pub enum StreamChunk {
    TextDelta { text: String },
    Done { message: Message },
    Error { error: String },
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Marks cdout-injected continuation/loop/interrupt nudges that need
    /// to look like role:user to the LLM API but should render as system
    /// notes in the UI. Defaults to false and is dropped from JSON when false
    /// so it doesn't leak to provider APIs that reject unknown fields.
    #[serde(default, skip_serializing_if = "is_false")]
    pub synthetic: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Message {
    /// Make a copy of `messages` with the `synthetic` flag cleared. Use this
    /// for any outbound LLM request — provider APIs may reject unknown fields,
    /// and synthetic is purely an internal-UI concept.
    pub fn strip_synthetic_for_api(messages: &[Message]) -> Vec<Message> {
        messages
            .iter()
            .map(|m| {
                let mut m = m.clone();
                m.synthetic = false;
                m
            })
            .collect()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCall {
    #[serde(default)]
    pub id: Option<String>,
    pub function: FunctionCall,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: Value, // Arguments are JSON object
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolDefinition {
    pub r#type: String, // usually "function"
    pub function: ToolFunction,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON schema for parameters
}
