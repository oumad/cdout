use super::{Tool, ToolResult};
use serde_json::{json, Value};

pub struct AskUserQuestionTool;

impl Tool for AskUserQuestionTool {
    fn name(&self) -> &str {
        "ask_user_question"
    }

    fn description(&self) -> &str {
        "Ask the user a multiple-choice question when their request is ambiguous. \
        Use this BEFORE proposing a PowerShell command if the answer would meaningfully \
        change the command — e.g. rename pattern (date prefix vs sequence), output format \
        (mp4 vs mov), or whether to overwrite originals. Do not use for trivial choices. \
        The user picks one option and the answer is fed back as the tool result."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The full question to ask the user. Be concise and specific."
                },
                "header": {
                    "type": "string",
                    "description": "Optional short label (≤12 chars) shown as a tag, e.g. 'Format', 'Pattern'."
                },
                "options": {
                    "type": "array",
                    "description": "2-6 answer options.",
                    "minItems": 2,
                    "maxItems": 6,
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": {
                                "type": "string",
                                "description": "The option text shown to the user (1-6 words)."
                            },
                            "description": {
                                "type": "string",
                                "description": "Optional context explaining what choosing this option does."
                            }
                        },
                        "required": ["label"]
                    }
                },
                "multi_select": {
                    "type": "boolean",
                    "description": "If true, the user can pick multiple options. Default false."
                }
            },
            "required": ["question", "options"]
        })
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn validate_input(&self, arguments: &Value) -> Result<(), String> {
        let question = arguments
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'question' parameter — expected a string")?;
        if question.trim().is_empty() {
            return Err("Question cannot be empty".to_string());
        }

        let options = arguments
            .get("options")
            .and_then(|v| v.as_array())
            .ok_or("Missing or invalid 'options' parameter — expected an array")?;
        if !(2..=6).contains(&options.len()) {
            return Err(format!(
                "'options' must have 2-6 entries, got {}",
                options.len()
            ));
        }
        for (i, opt) in options.iter().enumerate() {
            let label = opt
                .get("label")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("Option {} is missing a 'label' string", i + 1))?;
            if label.trim().is_empty() {
                return Err(format!("Option {} has an empty label", i + 1));
            }
        }
        Ok(())
    }

    fn execute(&self, _arguments: &Value, _cwd: Option<&str>) -> ToolResult {
        // The frontend intercepts ask_user_question proposals and returns the
        // user's selection as a tool_result message — execute is never called
        // server-side. If it is, surface a clear error so the bug is obvious.
        ToolResult {
            output: "ask_user_question must be answered through the UI, not executed."
                .to_string(),
            is_error: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_minimal_input() {
        let t = AskUserQuestionTool;
        let args = json!({
            "question": "Which format?",
            "options": [{"label": "mp4"}, {"label": "mov"}]
        });
        assert!(t.validate_input(&args).is_ok());
    }

    #[test]
    fn rejects_too_few_options() {
        let t = AskUserQuestionTool;
        let args = json!({
            "question": "Which?",
            "options": [{"label": "only one"}]
        });
        let err = t.validate_input(&args).unwrap_err();
        assert!(err.contains("2-6"));
    }

    #[test]
    fn rejects_empty_question() {
        let t = AskUserQuestionTool;
        let args = json!({
            "question": "   ",
            "options": [{"label": "a"}, {"label": "b"}]
        });
        assert!(t.validate_input(&args).is_err());
    }

    #[test]
    fn rejects_option_without_label() {
        let t = AskUserQuestionTool;
        let args = json!({
            "question": "Which?",
            "options": [{"label": "a"}, {"description": "no label"}]
        });
        assert!(t.validate_input(&args).is_err());
    }
}
