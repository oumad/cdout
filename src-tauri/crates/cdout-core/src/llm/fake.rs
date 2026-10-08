//! Scripted, network-free LLM for end-to-end tests of the UI shells.
//!
//! Compiled only with the `fake-llm` feature, and active only when the
//! `CDOUT_FAKE_LLM` environment variable names a scenario. The router then
//! answers every request from here instead of a provider, so a test can drive
//! the real agent loop (proposal parsing, loop detector, approvals, shell
//! execution) deterministically.
//!
//! Scenarios (`CDOUT_FAKE_LLM=<name>`):
//! - `text`: streams "Hello from the fake model." in five deltas.
//! - `command`: streams a sentence, then proposes the platform shell tool with
//!   `echo cdout-fake`.
//! - `question`: proposes `ask_user_question` with two options.
//! - `long`: streams 200 short deltas, for exercising Stop mid-stream.
//!
//! In every scenario, once the conversation's last message is a tool result
//! the fake replies "Task Complete", so multi-step flows terminate.
//! `CDOUT_FAKE_LLM_DELAY_MS` sets the pause between deltas (default 30 ms).

use std::time::Duration;

use serde_json::json;

use super::{FunctionCall, Message, ToolCall};

pub const ENV: &str = "CDOUT_FAKE_LLM";
pub const DELAY_ENV: &str = "CDOUT_FAKE_LLM_DELAY_MS";
/// The single model the fake advertises, so a UI always has something usable.
pub const MODEL: &str = "ollama:fake";
pub const FAKE_COMMAND: &str = "echo cdout-fake";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    Text,
    Command,
    Question,
    Long,
}

impl Scenario {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "text" => Some(Self::Text),
            "command" => Some(Self::Command),
            "question" => Some(Self::Question),
            "long" => Some(Self::Long),
            _ => None,
        }
    }
}

pub fn scenario() -> Option<Scenario> {
    Scenario::parse(&std::env::var(ENV).ok()?)
}

pub fn enabled() -> bool {
    scenario().is_some()
}

fn delay() -> Duration {
    let ms = std::env::var(DELAY_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30);
    Duration::from_millis(ms)
}

/// What the fake says next, before streaming: the text deltas plus any tool
/// calls to attach to the final message. Pure, so it is unit-testable.
pub fn plan_reply(scenario: Scenario, history: &[Message]) -> (Vec<String>, Option<Vec<ToolCall>>) {
    let after_tool = history.last().is_some_and(|m| m.role == "tool");
    if after_tool {
        return (vec!["Task ".into(), "Complete".into()], None);
    }
    match scenario {
        Scenario::Text => (
            ["Hello ", "from ", "the ", "fake ", "model."]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            None,
        ),
        Scenario::Command => (
            vec!["I'll run ".into(), "a command.".into()],
            Some(vec![ToolCall {
                id: Some("fake-call-1".into()),
                function: FunctionCall {
                    name: crate::constants::TOOL_NAME.to_string(),
                    arguments: json!({ "command": FAKE_COMMAND }),
                },
            }]),
        ),
        Scenario::Question => (
            Vec::new(),
            Some(vec![ToolCall {
                id: Some("fake-question-1".into()),
                function: FunctionCall {
                    name: crate::constants::ASK_USER_QUESTION_TOOL.to_string(),
                    arguments: json!({
                        "question": "Which format?",
                        "options": [{ "label": "mp4" }, { "label": "mov" }]
                    }),
                },
            }]),
        ),
        Scenario::Long => ((0..200).map(|i| format!("word{i} ")).collect(), None),
    }
}

/// Stream the planned reply through `on_chunk`, honouring the global cancel
/// flag the same way real providers do: stop emitting and return what was
/// produced so far.
pub async fn chat_stream(
    scenario: Scenario,
    history: &[Message],
    on_chunk: impl Fn(String) + Send + 'static,
) -> Result<Message, String> {
    let (deltas, tool_calls) = plan_reply(scenario, history);
    let pause = delay();
    let mut content = String::new();
    for d in deltas {
        if crate::utils::cancel::is_cancelled() {
            return Ok(Message {
                role: "assistant".into(),
                content,
                tool_calls: None,
                synthetic: false,
            });
        }
        tokio::time::sleep(pause).await;
        content.push_str(&d);
        on_chunk(d);
    }
    Ok(Message {
        role: "assistant".into(),
        content,
        tool_calls,
        synthetic: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str) -> Message {
        Message {
            role: role.into(),
            content: String::new(),
            tool_calls: None,
            synthetic: false,
        }
    }

    #[test]
    fn parses_scenarios() {
        assert_eq!(Scenario::parse("TEXT"), Some(Scenario::Text));
        assert_eq!(Scenario::parse(" command "), Some(Scenario::Command));
        assert_eq!(Scenario::parse("question"), Some(Scenario::Question));
        assert_eq!(Scenario::parse("long"), Some(Scenario::Long));
        assert_eq!(Scenario::parse("nope"), None);
    }

    #[test]
    fn command_scenario_proposes_the_shell_tool() {
        let (deltas, calls) = plan_reply(Scenario::Command, &[msg("system"), msg("user")]);
        assert!(!deltas.is_empty());
        let calls = calls.unwrap();
        assert_eq!(calls[0].function.name, crate::constants::TOOL_NAME);
        assert_eq!(calls[0].function.arguments["command"], FAKE_COMMAND);
    }

    #[test]
    fn any_scenario_completes_after_a_tool_result() {
        for s in [
            Scenario::Text,
            Scenario::Command,
            Scenario::Question,
            Scenario::Long,
        ] {
            let (deltas, calls) = plan_reply(s, &[msg("user"), msg("assistant"), msg("tool")]);
            assert_eq!(deltas.concat(), "Task Complete");
            assert!(calls.is_none());
        }
    }

    #[tokio::test]
    async fn streams_all_deltas_in_order() {
        std::env::set_var(DELAY_ENV, "0");
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&seen);
        let out = chat_stream(Scenario::Text, &[msg("user")], move |d| {
            sink.lock().unwrap().push(d)
        })
        .await
        .unwrap();
        assert_eq!(out.content, "Hello from the fake model.");
        assert_eq!(seen.lock().unwrap().len(), 5);
    }
}
