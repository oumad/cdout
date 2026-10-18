//! Drive the agent CLIs signed in on this machine through cdout's real agent
//! loop, for checking the subscription backends end to end.
//!
//!     cargo run -p cdout-core --example cli_probe                     # every signed-in CLI
//!     cargo run -p cdout-core --example cli_probe -- claude-code:haiku # one model
//!
//! Prints what detection reports, then per model runs three agent steps
//! against a made-up folder (nothing is executed): a request that needs a
//! command, a follow-up after a fake successful result, and an ambiguous
//! request that should come back as a question. Exits 1 if a step fails, the
//! first answer is not a command, the third is not a question, or raw tool
//! JSON leaked into the streamed text. Uses a throwaway data dir.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use cdout_core::api;
use cdout_core::features::agent::{AgentResponse, AgentStepResult, ChunkSink};
use cdout_core::features::skills;
use cdout_core::llm::clients::cli;
use cdout_core::llm::{FunctionCall, Message, StreamChunk, ToolCall};

#[derive(Default)]
struct Capture(Mutex<String>);

impl ChunkSink for Capture {
    fn send(&self, chunk: StreamChunk) {
        if let StreamChunk::TextDelta { text } = chunk {
            self.0.lock().unwrap().push_str(&text);
        }
    }
}

#[tokio::main]
async fn main() {
    let data = std::env::temp_dir().join("cdout-cli-probe");
    std::env::set_var("CDOUT_DATA_DIR", &data);

    println!("== detection");
    let statuses = cli::statuses().await;
    for s in &statuses {
        println!(
            "{:<12} installed={:<5} signed_in={:<5} {}",
            s.name, s.installed, s.signed_in, s.detail
        );
    }

    let models: Vec<String> = match std::env::args().nth(1) {
        Some(m) => vec![m],
        None => statuses
            .iter()
            .filter(|s| s.signed_in)
            .flat_map(|s| s.backend.models().first().map(|m| m.to_string()))
            .collect(),
    };
    if models.is_empty() {
        println!("no signed-in CLI to probe");
        return;
    }

    let mut failures = Vec::new();
    for model in &models {
        println!("\n== {model}");
        if let Err(e) = probe(model).await {
            println!("FAIL: {e}");
            failures.push(format!("{model}: {e}"));
        }
    }
    if !failures.is_empty() {
        println!("\n{} failure(s)", failures.len());
        std::process::exit(1);
    }
    println!("\nall probes passed");
}

async fn step(model: &str, history: Vec<Message>) -> Result<(AgentStepResult, String), String> {
    let sink = Arc::new(Capture::default());
    let started = Instant::now();
    let result = api::run_agent_step(model.to_string(), history, false, sink.clone()).await?;
    let streamed = sink.0.lock().unwrap().clone();
    println!(
        "  {:.1}s, {} chars streamed",
        started.elapsed().as_secs_f32(),
        streamed.len()
    );
    if streamed.contains("```tool") {
        return Err(format!("tool JSON leaked into the stream: {streamed:?}"));
    }
    Ok((result, streamed))
}

fn describe(r: &AgentResponse) -> String {
    match r {
        AgentResponse::Text(t) => format!("text: {}", t.trim()),
        AgentResponse::CommandProposal(c) => format!("command: {c}"),
        AgentResponse::ToolProposals(p) => p
            .iter()
            .map(|p| format!("{}: {}", p.tool_name, p.command))
            .collect::<Vec<_>>()
            .join(" | "),
    }
}

async fn probe(model: &str) -> Result<(), String> {
    let loaded = skills::load_all_skills_embedded();
    let files = vec![
        "C:/clips/intro.mov".to_string(),
        "C:/clips/outro.mov".to_string(),
    ];

    println!("- step 1: needs a command");
    let history = api::init_agent_conversation(
        "C:/clips",
        &files,
        "Convert the selected videos to mp4, keeping the originals.",
        Some(model),
        &loaded,
    )?;
    let (first, streamed) = step(model, history).await?;
    println!("  streamed: {}", streamed.trim().replace('\n', " "));
    println!("  {}", describe(&first.response));
    let AgentResponse::CommandProposal(_) = first.response else {
        return Err("step 1 did not propose a command".into());
    };

    println!("- step 2: after a successful result");
    let mut history = first.updated_history;
    let call = history
        .last()
        .and_then(|m| m.tool_calls.as_ref())
        .and_then(|c| c.first())
        .cloned();
    history.push(Message {
        role: "tool".into(),
        content:
            "[Exit code: 0 — Success]\nC:/clips/intro.mp4\nC:/clips/outro.mp4\nBoth files exist."
                .into(),
        tool_calls: call.map(|c| {
            vec![ToolCall {
                id: c.id,
                function: FunctionCall {
                    name: c.function.name,
                    arguments: serde_json::json!({}),
                },
            }]
        }),
        synthetic: false,
    });
    let (second, _) = step(model, history).await?;
    println!("  {}", describe(&second.response));

    println!("- step 3: ambiguous, should ask");
    let history = api::init_agent_conversation(
        "C:/photos",
        &["C:/photos/IMG_0001.jpg".to_string()],
        "Rename these photos.",
        Some(model),
        &loaded,
    )?;
    let (third, _) = step(model, history).await?;
    println!("  {}", describe(&third.response));
    match &third.response {
        AgentResponse::ToolProposals(p) if p.iter().any(|p| p.question_data.is_some()) => Ok(()),
        _ => Err("step 3 did not ask a question".into()),
    }
}
