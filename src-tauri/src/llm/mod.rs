use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod clients;
pub mod router;

// Streaming types for Tauri Channel
#[derive(Serialize, Clone)]
#[serde(tag = "kind")]
pub enum StreamChunk {
    TextDelta { text: String },
    Done { message: Message },
    Error { error: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCall {
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
