//! Agent CLIs the user already has installed and signed in (Claude Code,
//! Codex) used as model providers, so an existing subscription works with no
//! API key.
//!
//! cdout never reads, stores or forwards their credentials. It runs the
//! official, unmodified binary in headless mode; the binary signs in through
//! its own flow and usage is billed to the user's own plan. The CLI's own
//! tools are switched off (Claude Code) or confined to a read-only sandbox in
//! an empty directory (Codex), so every command still comes back to cdout as a
//! proposal and waits for approval like any other provider.
//!
//! Each step is one stateless CLI run: the conversation is rendered as a
//! transcript on stdin and the system prompt goes in a file. Tool calls travel
//! as text: the system prompt gets a protocol section, the model answers with
//! a fenced `tool` block, and [`parse_tool_blocks`] turns that into a native
//! `tool_calls` entry, so the agent loop downstream is unchanged.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::llm::{FunctionCall, Message, ToolCall, ToolDefinition};
use crate::utils::{cancel, paths};

/// A model slug's provider half, e.g. `claude-code:sonnet`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    ClaudeCode,
    Codex,
}

impl Backend {
    pub const ALL: [Backend; 2] = [Backend::ClaudeCode, Backend::Codex];

    pub fn prefix(self) -> &'static str {
        match self {
            Backend::ClaudeCode => "claude-code:",
            Backend::Codex => "codex:",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Backend::ClaudeCode => "Claude Code",
            Backend::Codex => "Codex",
        }
    }

    fn binary(self) -> &'static str {
        match self {
            Backend::ClaudeCode => "claude",
            Backend::Codex => "codex",
        }
    }

    /// What to type in a terminal to sign in.
    pub fn sign_in_command(self) -> &'static str {
        match self {
            Backend::ClaudeCode => "claude",
            Backend::Codex => "codex login",
        }
    }

    pub fn install_url(self) -> &'static str {
        match self {
            Backend::ClaudeCode => "https://code.claude.com/docs/en/setup",
            Backend::Codex => "https://github.com/openai/codex",
        }
    }

    /// Slugs offered in the picker once signed in. Claude Code takes model
    /// aliases, which always mean the latest of that family; `codex:default`
    /// is whatever model the user's Codex config selects.
    pub fn models(self) -> &'static [&'static str] {
        match self {
            Backend::ClaudeCode => &[
                "claude-code:sonnet",
                "claude-code:opus",
                "claude-code:haiku",
            ],
            Backend::Codex => &["codex:default"],
        }
    }
}

/// The backend and model for a slug, if it names a CLI backend.
pub fn route(model: &str) -> Option<(Backend, &str)> {
    Backend::ALL
        .iter()
        .find_map(|b| model.strip_prefix(b.prefix()).map(|rest| (*b, rest)))
}

// --- Detection ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CliStatus {
    pub backend: Backend,
    pub name: &'static str,
    pub installed: bool,
    pub signed_in: bool,
    /// "Max plan", "ChatGPT account", … or why it cannot be used.
    pub detail: String,
    pub sign_in_command: &'static str,
    pub install_url: &'static str,
}

impl CliStatus {
    fn new(backend: Backend, installed: bool, signed_in: bool, detail: impl Into<String>) -> Self {
        Self {
            backend,
            name: backend.name(),
            installed,
            signed_in,
            detail: detail.into(),
            sign_in_command: backend.sign_in_command(),
            install_url: backend.install_url(),
        }
    }
}

/// Detection spawns each CLI once, so results are reused for a short while:
/// the model list is polled every few seconds while nothing is usable, which
/// is exactly when someone is signing in, so this stays short.
const STATUS_TTL: Duration = Duration::from_secs(10);
const STATUS_TIMEOUT: Duration = Duration::from_secs(15);
static STATUS_CACHE: Mutex<Option<(Instant, Vec<CliStatus>)>> = Mutex::new(None);

/// Every known backend, installed or not, in [`Backend::ALL`] order.
pub async fn statuses() -> Vec<CliStatus> {
    if let Some((at, cached)) = STATUS_CACHE.lock().unwrap().as_ref() {
        if at.elapsed() < STATUS_TTL {
            return cached.clone();
        }
    }
    let (claude, codex) = tokio::join!(detect(Backend::ClaudeCode), detect(Backend::Codex));
    let all = vec![claude, codex];
    *STATUS_CACHE.lock().unwrap() = Some((Instant::now(), all.clone()));
    all
}

/// Drop cached detection results (the user says they just signed in).
pub fn forget_statuses() {
    *STATUS_CACHE.lock().unwrap() = None;
}

async fn detect(backend: Backend) -> CliStatus {
    let Some(bin) = find_binary(backend.binary()) else {
        return CliStatus::new(backend, false, false, "Not installed");
    };
    let args: &[&str] = match backend {
        Backend::ClaudeCode => &["auth", "status"],
        Backend::Codex => &["login", "status"],
    };
    let mut cmd = command(&bin);
    cmd.args(args);
    let output = match tokio::time::timeout(STATUS_TIMEOUT, cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            return CliStatus::new(backend, true, false, format!("Could not run it: {e}"))
        }
        Err(_) => return CliStatus::new(backend, true, false, "Did not answer in time"),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let (signed_in, detail) = match backend {
        Backend::ClaudeCode => parse_claude_auth(&stdout),
        Backend::Codex => parse_codex_login(output.status.success(), &stdout, &stderr),
    };
    CliStatus::new(backend, true, signed_in, detail)
}

/// `claude auth status` prints JSON: `{"loggedIn": true, "authMethod":
/// "claude.ai", "subscriptionType": "max", …}`.
pub fn parse_claude_auth(stdout: &str) -> (bool, String) {
    let Ok(v) = serde_json::from_str::<Value>(stdout.trim()) else {
        return (false, "Not signed in".into());
    };
    if v.get("loggedIn").and_then(Value::as_bool) != Some(true) {
        return (false, "Not signed in".into());
    }
    let detail = match v.get("subscriptionType").and_then(Value::as_str) {
        Some(plan) if !plan.is_empty() => {
            let mut c = plan.chars();
            let plan = c
                .next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>());
            format!("{} plan", plan.unwrap_or_default())
        }
        _ if v.get("authMethod").and_then(Value::as_str) == Some("claude.ai") => {
            "Claude account".into()
        }
        _ => "API key".into(),
    };
    (true, detail)
}

/// `codex login status` exits 0 and says "Logged in using ChatGPT" (or "…an
/// API key") when signed in; the text may land on either stream.
pub fn parse_codex_login(success: bool, stdout: &str, stderr: &str) -> (bool, String) {
    let text = format!("{stdout}\n{stderr}");
    if !success || !text.contains("Logged in") {
        return (false, "Not signed in".into());
    }
    if text.contains("ChatGPT") {
        (true, "ChatGPT account".into())
    } else {
        (true, "API key".into())
    }
}

// --- Processes ---

/// Locate a CLI the way a terminal would. A GUI app's PATH can be shorter than
/// the user's shell's (always on macOS), so the usual install locations are
/// searched too.
fn find_binary(name: &str) -> Option<PathBuf> {
    let home = dirs::home_dir();
    #[cfg(windows)]
    {
        let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        if let Some(h) = &home {
            dirs.push(h.join(".local").join("bin"));
        }
        if let Some(appdata) = dirs::data_dir() {
            dirs.push(appdata.join("npm"));
        }
        // npm installs a `.cmd` shim next to the real launcher; Rust runs
        // `.cmd` files through cmd.exe with proper argument escaping.
        dirs.iter()
            .flat_map(|d| ["exe", "cmd"].map(|ext| d.join(format!("{name}.{ext}"))))
            .find(|p| p.is_file())
    }
    #[cfg(not(windows))]
    {
        let mut dirs: Vec<PathBuf> = std::env::split_paths(child_path()).collect();
        if let Some(h) = &home {
            dirs.push(h.join(".local").join("bin"));
            dirs.push(h.join(".claude").join("local"));
        }
        dirs.iter().map(|d| d.join(name)).find(|p| p.is_file())
    }
}

/// PATH for the CLIs we spawn: the login shell's, so tools installed through
/// Homebrew, nvm or the official installers resolve (npm-installed CLIs are
/// `#!/usr/bin/env node` scripts and need `node` on it too).
#[cfg(not(windows))]
fn child_path() -> &'static str {
    use std::sync::OnceLock;
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let login = std::process::Command::new(crate::platform::shell_path())
            .args(["-l", "-c", "printf %s \"$PATH\""])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        let current = std::env::var("PATH").unwrap_or_default();
        let mut parts: Vec<String> = Vec::new();
        for p in login
            .split(':')
            .chain(current.split(':'))
            .chain(["/opt/homebrew/bin", "/usr/local/bin"])
        {
            if !p.is_empty() && !parts.iter().any(|q| q == p) {
                parts.push(p.to_string());
            }
        }
        parts.join(":")
    })
}

fn command(bin: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        // Claude Code refuses to start inside another Claude Code session,
        // which is what cdout looks like when launched from one.
        .env_remove("CLAUDECODE");
    #[cfg(windows)]
    {
        // No console window flashing up over the user's desktop.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        cmd.env("PATH", child_path());
        // Own process group, so Stop takes the whole tree down.
        cmd.process_group(0);
    }
    cmd
}

/// An empty directory the CLIs run in, so they pick up no project files or
/// instructions from wherever cdout was started.
fn work_dir() -> Result<PathBuf, String> {
    let dir = paths::app_data_dir().join("cli-work");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    Ok(dir)
}

struct RemoveOnDrop(PathBuf);
impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A TOML string literal for a path, for Codex's `-c key=value` overrides.
/// Forward slashes work on Windows and need no escaping.
fn toml_path(p: &Path) -> String {
    let s = p
        .display()
        .to_string()
        .replace('\\', "/")
        .replace('"', "\\\"");
    format!("\"{s}\"")
}

/// Codex streams nothing until an answer is complete, and a high reasoning
/// effort can think for minutes, so the stall limit is generous.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
const POLL: Duration = Duration::from_millis(250);

/// One agent step through a CLI. Streams visible text to `on_chunk` and
/// returns the assistant message, with tool calls parsed out of its text.
pub async fn chat_stream(
    backend: Backend,
    model: &str,
    history: Vec<Message>,
    tools: Option<Vec<ToolDefinition>>,
    on_chunk: impl Fn(String) + Send + 'static,
) -> Result<Message, String> {
    let name = backend.name();
    let bin = find_binary(backend.binary()).ok_or_else(|| {
        format!(
            "{name} is not installed. Install it ({}), sign in, then pick it again.",
            backend.install_url()
        )
    })?;
    let tools = tools.unwrap_or_default();
    let (system, transcript) = render_conversation(&history, &tools);

    let dir = work_dir()?;
    let system_file = dir.join(format!("system-{:08x}.txt", rand::random::<u32>()));
    std::fs::write(&system_file, &system)
        .map_err(|e| format!("{}: {}", system_file.display(), e))?;
    let _cleanup = RemoveOnDrop(system_file.clone());

    let mut cmd = command(&bin);
    cmd.current_dir(&dir).stdin(Stdio::piped());
    match backend {
        Backend::ClaudeCode => {
            cmd.args([
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                // No built-in tools, MCP servers, settings, hooks or skills:
                // it is a model here, not an agent.
                "--tools",
                "",
                "--strict-mcp-config",
                "--setting-sources",
                "",
                "--disable-slash-commands",
                "--no-session-persistence",
                "--system-prompt-file",
            ])
            .arg(&system_file);
            if model != "default" {
                cmd.args(["--model", model]);
            }
        }
        Backend::Codex => {
            cmd.args([
                "exec",
                "--json",
                "--skip-git-repo-check",
                "--ephemeral",
                "--sandbox",
                "read-only",
                "--cd",
            ])
            .arg(&dir)
            // Replaces Codex's coding-agent instructions with cdout's prompt.
            .arg("--config")
            .arg(format!(
                "model_instructions_file={}",
                toml_path(&system_file)
            ));
            if model != "default" {
                cmd.args(["--model", model]);
            }
            cmd.arg("-");
        }
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start {name}: {e}"))?;
    let pid = child.id();
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(transcript.as_bytes())
            .await
            .map_err(|e| format!("Could not send the conversation to {name}: {e}"))?;
        // Dropping stdin closes it, which is what tells the CLI to start.
    }
    let stderr_task = child.stderr.take().map(|mut s| {
        tokio::spawn(async move {
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf).await;
            buf
        })
    });
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{name} produced no output stream"))?;
    let mut lines = BufReader::new(stdout).lines();

    let kill = |pid: Option<u32>| {
        if let Some(pid) = pid {
            let _ = crate::platform::kill_process_tree(pid);
        }
    };

    let mut filter = ToolBlockFilter::default();
    let mut text = String::new();
    let mut fallback: Option<String> = None;
    let mut failure: Option<String> = None;
    let mut last_data = Instant::now();
    loop {
        if cancel::is_cancelled() {
            kill(pid);
            let (content, _) = parse_tool_blocks(&text, &[]);
            return Ok(assistant(content, Vec::new()));
        }
        let line = match tokio::time::timeout(POLL, lines.next_line()).await {
            Err(_) => {
                if last_data.elapsed() > IDLE_TIMEOUT {
                    kill(pid);
                    return Err(format!(
                        "{name} stalled — no output for {}s.",
                        IDLE_TIMEOUT.as_secs()
                    ));
                }
                continue;
            }
            Ok(Err(e)) => return Err(format!("Reading {name}'s output failed: {e}")),
            Ok(Ok(None)) => break,
            Ok(Ok(Some(line))) => line,
        };
        last_data = Instant::now();
        let event = match backend {
            Backend::ClaudeCode => parse_claude_line(&line),
            Backend::Codex => parse_codex_line(&line),
        };
        match event {
            Event::Text(t) => {
                // Codex delivers whole messages; keep them apart.
                if backend == Backend::Codex && !text.is_empty() {
                    text.push_str("\n\n");
                    emit(&on_chunk, filter.push("\n\n"));
                }
                text.push_str(&t);
                emit(&on_chunk, filter.push(&t));
            }
            Event::Final(t) => fallback = Some(t),
            Event::Failed(e) => failure = Some(e),
            Event::Ignore => {}
        }
    }

    let status = child.wait().await.ok();
    let stderr = match stderr_task {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };
    if let Some(e) = failure {
        return Err(explain_failure(backend, &e));
    }
    if text.is_empty() {
        if let Some(t) = fallback {
            emit(&on_chunk, filter.push(&t));
            text = t;
        }
    }
    emit(&on_chunk, filter.finish());
    if text.trim().is_empty() && !status.is_some_and(|s| s.success()) {
        let mut tail: Vec<&str> = stderr.trim().lines().rev().take(6).collect();
        tail.reverse();
        let tail = tail.join("\n");
        return Err(explain_failure(
            backend,
            if tail.is_empty() {
                "exited without an answer"
            } else {
                &tail
            },
        ));
    }

    let (content, calls) = parse_tool_blocks(&text, &tools);
    Ok(assistant(content, calls))
}

fn emit(on_chunk: &impl Fn(String), text: String) {
    if !text.is_empty() {
        on_chunk(text);
    }
}

fn assistant(content: String, calls: Vec<ToolCall>) -> Message {
    Message {
        role: "assistant".into(),
        content,
        tool_calls: (!calls.is_empty()).then_some(calls),
        synthetic: false,
    }
}

/// Prefix the CLI's name and, when the message reads like a sign-in problem,
/// say how to fix it.
fn explain_failure(backend: Backend, raw: &str) -> String {
    let lower = raw.to_lowercase();
    let auth = [
        "login",
        "log in",
        "logged in",
        "sign in",
        "authenticat",
        "api key",
        "unauthorized",
    ]
    .iter()
    .any(|k| lower.contains(k));
    if auth {
        format!(
            "{}: {}\nSign in by running `{}` in a terminal, then try again.",
            backend.name(),
            raw.trim(),
            backend.sign_in_command()
        )
    } else {
        format!("{}: {}", backend.name(), raw.trim())
    }
}

// --- Output parsing ---

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// Text to append to the answer.
    Text(String),
    /// The whole answer, used only if nothing was streamed.
    Final(String),
    Failed(String),
    Ignore,
}

/// One line of `claude -p --output-format stream-json --include-partial-messages`.
pub fn parse_claude_line(line: &str) -> Event {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return Event::Ignore;
    };
    match v.get("type").and_then(Value::as_str) {
        Some("stream_event") => {
            let delta = &v["event"]["delta"];
            if v["event"]["type"] == "content_block_delta" && delta["type"] == "text_delta" {
                if let Some(t) = delta["text"].as_str() {
                    return Event::Text(t.to_string());
                }
            }
            Event::Ignore
        }
        Some("result") => {
            let text = v["result"].as_str().unwrap_or_default().to_string();
            if v["is_error"].as_bool() == Some(true) {
                let msg = if text.is_empty() {
                    v["subtype"].as_str().unwrap_or("failed").to_string()
                } else {
                    text
                };
                Event::Failed(msg)
            } else {
                Event::Final(text)
            }
        }
        _ => Event::Ignore,
    }
}

/// One line of `codex exec --json`.
pub fn parse_codex_line(line: &str) -> Event {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return Event::Ignore;
    };
    match v.get("type").and_then(Value::as_str) {
        Some("item.completed") => match v["item"]["type"].as_str() {
            Some("agent_message") => v["item"]["text"]
                .as_str()
                .map(|t| Event::Text(t.to_string()))
                .unwrap_or(Event::Ignore),
            Some("error") => Event::Failed(
                v["item"]["message"]
                    .as_str()
                    .unwrap_or("failed")
                    .to_string(),
            ),
            _ => Event::Ignore,
        },
        Some("error") => Event::Failed(v["message"].as_str().unwrap_or("failed").to_string()),
        Some("turn.failed") => Event::Failed(
            v["error"]["message"]
                .as_str()
                .unwrap_or("the turn failed")
                .to_string(),
        ),
        _ => Event::Ignore,
    }
}

// --- The text tool protocol ---

const FENCE: &str = "```tool";

/// The system prompt (every system message) plus the tool protocol, and the
/// rest of the conversation as a transcript.
pub fn render_conversation(history: &[Message], tools: &[ToolDefinition]) -> (String, String) {
    let mut system: Vec<&str> = Vec::new();
    let mut turns: Vec<String> = Vec::new();
    for m in history {
        match m.role.as_str() {
            "system" => system.push(&m.content),
            "assistant" => {
                let mut t = format!("[assistant]\n{}", m.content.trim());
                for call in m.tool_calls.iter().flatten() {
                    let block = serde_json::json!({
                        "name": call.function.name,
                        "arguments": call.function.arguments,
                    });
                    t.push_str(&format!("\n{FENCE}\n{block}\n```"));
                }
                turns.push(t);
            }
            "tool" => {
                let name = m
                    .tool_calls
                    .as_ref()
                    .and_then(|c| c.first())
                    .map(|c| c.function.name.as_str())
                    .unwrap_or("tool");
                turns.push(format!("[tool result: {name}]\n{}", m.content.trim()));
            }
            _ => turns.push(format!("[user]\n{}", m.content.trim())),
        }
    }
    let mut system = system.join("\n\n");
    system.push_str("\n\n");
    system.push_str(&tool_protocol(tools));

    let transcript = format!(
        "The conversation so far, oldest first. Each entry starts with its role in brackets.\n\n{}\n\n\
         Write the assistant's next message only, without a role line.",
        turns.join("\n\n")
    );
    (system, transcript)
}

fn tool_protocol(tools: &[ToolDefinition]) -> String {
    if tools.is_empty() {
        return "TOOLS: none are available for this message. Answer in plain text.".into();
    }
    let mut s = format!(
        "HOW TO CALL A TOOL:\n\
         You are running inside cdout, which runs tools for you after the user approves them. \
         You cannot run anything yourself and have no other tools. To call a tool, write at most \
         a sentence or two, then exactly one fenced block tagged `tool` holding a JSON object with \
         \"name\" and \"arguments\", and stop:\n\n\
         {FENCE}\n{{\"name\": \"<tool name>\", \"arguments\": {{ ... }}}}\n```\n\n\
         The result comes back in the next turn as a [tool result] entry. One tool call per \
         message. When no tool is needed, reply in plain text with no tool block.\n\nTOOLS:"
    );
    for t in tools {
        s.push_str(&format!(
            "\n- {}: {}\n  arguments (JSON Schema): {}",
            t.function.name, t.function.description, t.function.parameters
        ));
    }
    s
}

/// Pull tool calls out of an answer. Blocks fenced as `tool` (or `json`, if
/// the JSON names a known tool) become calls and are removed from the text;
/// anything else, including blocks that fail to parse, stays as written.
pub fn parse_tool_blocks(text: &str, tools: &[ToolDefinition]) -> (String, Vec<ToolCall>) {
    let known = |n: &str| tools.iter().any(|t| t.function.name == n);
    let mut out = String::new();
    let mut calls = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("```") {
        let after = &rest[open + 3..];
        let info_end = after.find('\n').unwrap_or(after.len());
        let info = after[..info_end].trim();
        let body_start = (info_end + 1).min(after.len());
        let Some(close) = after[body_start..].find("```") else {
            break;
        };
        let body = &after[body_start..body_start + close];
        let block_end = open + 3 + body_start + close + 3;
        let call = (info == "tool" || info == "json")
            .then(|| parse_call(body))
            .flatten()
            .filter(|c| known(&c.function.name));
        match call {
            Some(c) => {
                out.push_str(&rest[..open]);
                calls.push(c);
            }
            None => out.push_str(&rest[..block_end]),
        }
        rest = &rest[block_end..];
    }
    out.push_str(rest);
    (out.trim().to_string(), calls)
}

fn parse_call(body: &str) -> Option<ToolCall> {
    let v: Value = serde_json::from_str(body.trim()).ok()?;
    let obj = v.as_object()?;
    let name = obj.get("name")?.as_str()?.to_string();
    let arguments = match obj.get("arguments") {
        Some(Value::String(s)) => serde_json::from_str(s).ok()?,
        Some(a @ Value::Object(_)) => a.clone(),
        // `{"name": "run_shell", "command": "…"}`: the arguments flattened in.
        _ => {
            let mut flat = obj.clone();
            flat.remove("name");
            Value::Object(flat)
        }
    };
    Some(ToolCall {
        id: Some(format!("cli_{:08x}", rand::random::<u32>())),
        function: FunctionCall { name, arguments },
    })
}

/// Hides ` ```tool ` blocks from streamed text, so the transcript shows the
/// model's sentence and then the approval card, never the raw JSON. Holds
/// back a trailing partial fence until the next delta decides it.
#[derive(Default)]
pub struct ToolBlockFilter {
    pending: String,
    in_block: bool,
}

impl ToolBlockFilter {
    pub fn push(&mut self, delta: &str) -> String {
        self.pending.push_str(delta);
        let mut out = String::new();
        loop {
            if self.in_block {
                match self.pending.find("```") {
                    Some(i) => {
                        self.pending.drain(..i + 3);
                        self.in_block = false;
                    }
                    None => {
                        let keep = partial_suffix(&self.pending, "```");
                        let cut = self.pending.len() - keep;
                        self.pending.drain(..cut);
                        return out;
                    }
                }
            } else {
                match self.pending.find(FENCE) {
                    Some(i) => {
                        out.push_str(&self.pending[..i]);
                        self.pending.drain(..i + FENCE.len());
                        self.in_block = true;
                    }
                    None => {
                        let keep = partial_suffix(&self.pending, FENCE);
                        let cut = self.pending.len() - keep;
                        out.push_str(&self.pending[..cut]);
                        self.pending.drain(..cut);
                        return out;
                    }
                }
            }
        }
    }

    pub fn finish(&mut self) -> String {
        let rest = std::mem::take(&mut self.pending);
        if self.in_block {
            String::new()
        } else {
            rest
        }
    }
}

/// Length of the longest proper prefix of `fence` that `s` ends with. The
/// fence is ASCII, so cutting there is always on a char boundary.
fn partial_suffix(s: &str, fence: &str) -> usize {
    (1..fence.len())
        .rev()
        .find(|&n| s.ends_with(&fence[..n]))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ToolFunction;
    use serde_json::json;

    fn tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            r#type: "function".into(),
            function: ToolFunction {
                name: name.into(),
                description: format!("the {name} tool"),
                parameters: json!({"type": "object"}),
            },
        }
    }

    fn msg(role: &str, content: &str) -> Message {
        Message {
            role: role.into(),
            content: content.into(),
            ..Default::default()
        }
    }

    #[test]
    fn routes_cli_prefixes_only() {
        assert_eq!(
            route("claude-code:sonnet"),
            Some((Backend::ClaudeCode, "sonnet"))
        );
        assert_eq!(route("codex:default"), Some((Backend::Codex, "default")));
        assert_eq!(route("anthropic:claude-opus-4-7"), None);
        assert_eq!(route("ollama:llama3"), None);
        for b in Backend::ALL {
            for m in b.models() {
                assert_eq!(route(m).map(|r| r.0), Some(b), "{m}");
            }
        }
    }

    #[test]
    fn claude_auth_status_parsing() {
        let max = r#"{"loggedIn": true, "authMethod": "claude.ai", "subscriptionType": "max"}"#;
        assert_eq!(parse_claude_auth(max), (true, "Max plan".into()));
        let pro = r#"{"loggedIn": true, "authMethod": "claude.ai", "subscriptionType": "pro"}"#;
        assert_eq!(parse_claude_auth(pro), (true, "Pro plan".into()));
        let key = r#"{"loggedIn": true, "authMethod": "api_key"}"#;
        assert_eq!(parse_claude_auth(key), (true, "API key".into()));
        assert!(!parse_claude_auth(r#"{"loggedIn": false}"#).0);
        assert!(!parse_claude_auth("Not logged in. Run claude to sign in.").0);
    }

    #[test]
    fn codex_login_status_parsing() {
        assert_eq!(
            parse_codex_login(true, "", "Logged in using ChatGPT\n"),
            (true, "ChatGPT account".into())
        );
        assert_eq!(
            parse_codex_login(true, "Logged in using an API key - sk-***", ""),
            (true, "API key".into())
        );
        assert!(!parse_codex_login(false, "", "Not logged in").0);
        assert!(!parse_codex_login(true, "", "").0);
    }

    #[test]
    fn claude_stream_lines() {
        // Shapes recorded from Claude Code 2.1.
        let delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"pong"}},"session_id":"x"}"#;
        assert_eq!(parse_claude_line(delta), Event::Text("pong".into()));
        let start = r#"{"type":"stream_event","event":{"type":"message_start","message":{}}}"#;
        assert_eq!(parse_claude_line(start), Event::Ignore);
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":"pong"}"#;
        assert_eq!(parse_claude_line(ok), Event::Final("pong".into()));
        let err = r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login"}"#;
        assert_eq!(
            parse_claude_line(err),
            Event::Failed("Not logged in · Please run /login".into())
        );
        assert_eq!(parse_claude_line("not json"), Event::Ignore);
    }

    #[test]
    fn codex_stream_lines() {
        // Shapes recorded from codex-cli 0.153.
        let msg = r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"pong"}}"#;
        assert_eq!(parse_codex_line(msg), Event::Text("pong".into()));
        assert_eq!(
            parse_codex_line(r#"{"type":"turn.started"}"#),
            Event::Ignore
        );
        let cmd = r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"ls"}}"#;
        assert_eq!(parse_codex_line(cmd), Event::Ignore);
        let failed = r#"{"type":"turn.failed","error":{"message":"usage limit reached"}}"#;
        assert_eq!(
            parse_codex_line(failed),
            Event::Failed("usage limit reached".into())
        );
        let error = r#"{"type":"error","message":"stream disconnected"}"#;
        assert_eq!(
            parse_codex_line(error),
            Event::Failed("stream disconnected".into())
        );
        assert_eq!(
            parse_codex_line("Reading additional input from stdin..."),
            Event::Ignore
        );
    }

    #[test]
    fn tool_blocks_become_calls_and_leave_the_text() {
        let tools = [tool("run_shell"), tool("ask_user_question")];
        let text = "I'll list it.\n\n```tool\n{\"name\": \"run_shell\", \"arguments\": {\"command\": \"ls -la\"}}\n```\n";
        let (content, calls) = parse_tool_blocks(text, &tools);
        assert_eq!(content, "I'll list it.");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "run_shell");
        assert_eq!(calls[0].function.arguments, json!({"command": "ls -la"}));
        assert!(calls[0]
            .id
            .as_deref()
            .is_some_and(|id| id.starts_with("cli_")));
    }

    #[test]
    fn tool_block_variants() {
        let tools = [tool("run_shell")];
        // Arguments as a JSON string, as OpenAI-style models sometimes write.
        let s = "```tool\n{\"name\":\"run_shell\",\"arguments\":\"{\\\"command\\\":\\\"pwd\\\"}\"}\n```";
        assert_eq!(
            parse_tool_blocks(s, &tools).1[0].function.arguments,
            json!({"command": "pwd"})
        );
        // Arguments flattened into the object.
        let s = "```tool\n{\"name\":\"run_shell\",\"command\":\"pwd\"}\n```";
        assert_eq!(
            parse_tool_blocks(s, &tools).1[0].function.arguments,
            json!({"command": "pwd"})
        );
        // A json block naming a known tool counts.
        let s = "```json\n{\"name\":\"run_shell\",\"arguments\":{\"command\":\"pwd\"}}\n```";
        assert_eq!(parse_tool_blocks(s, &tools).1.len(), 1);
    }

    #[test]
    fn other_blocks_are_left_alone() {
        let tools = [tool("run_shell")];
        let s = "Example:\n```powershell\nGet-ChildItem\n```\nand data:\n```json\n{\"a\": 1}\n```";
        let (content, calls) = parse_tool_blocks(s, &tools);
        assert!(calls.is_empty());
        assert_eq!(content, s);
        // Unknown tool names and broken JSON stay as text too.
        let s = "```tool\n{\"name\":\"rm_everything\",\"arguments\":{}}\n```";
        assert!(parse_tool_blocks(s, &tools).1.is_empty());
        let s = "```tool\n{not json\n```";
        assert_eq!(parse_tool_blocks(s, &tools), (s.to_string(), vec![]));
        // With no tools offered nothing is parsed.
        let s = "```tool\n{\"name\":\"run_shell\",\"arguments\":{\"command\":\"x\"}}\n```";
        assert!(parse_tool_blocks(s, &[]).1.is_empty());
        // An unclosed fence is kept verbatim.
        let s = "text ```tool\n{\"name\":";
        assert_eq!(parse_tool_blocks(s, &tools).0, s);
    }

    #[test]
    fn filter_hides_tool_blocks_however_the_deltas_split() {
        let text = "Plan: convert.\n```tool\n{\"name\":\"run_shell\",\"arguments\":{\"command\":\"a ` b\"}}\n```\nDone.";
        let expected = "Plan: convert.\n\nDone.";
        // Every split position, and one character at a time.
        for cut in 0..=text.len() {
            if !text.is_char_boundary(cut) {
                continue;
            }
            let mut f = ToolBlockFilter::default();
            let mut out = f.push(&text[..cut]);
            out.push_str(&f.push(&text[cut..]));
            out.push_str(&f.finish());
            assert_eq!(out, expected, "split at {cut}");
        }
        let mut f = ToolBlockFilter::default();
        let mut out: String = text.chars().map(|c| f.push(&c.to_string())).collect();
        out.push_str(&f.finish());
        assert_eq!(out, expected);
    }

    #[test]
    fn filter_passes_ordinary_text_and_other_fences() {
        let text = "Use ``` for code:\n```powershell\nls\n```\nhé ✔";
        let mut f = ToolBlockFilter::default();
        let mut out: String = text.chars().map(|c| f.push(&c.to_string())).collect();
        out.push_str(&f.finish());
        assert_eq!(out, text);
    }

    #[test]
    fn conversation_rendering() {
        let mut call_msg = msg("assistant", "Listing.");
        call_msg.tool_calls = Some(vec![ToolCall {
            id: Some("cli_1".into()),
            function: FunctionCall {
                name: "run_shell".into(),
                arguments: json!({"command": "ls"}),
            },
        }]);
        let mut result = msg("tool", "[Exit code: 0 — Success]\na.mov");
        result.tool_calls = Some(vec![ToolCall {
            id: Some("cli_1".into()),
            function: FunctionCall {
                name: "run_shell".into(),
                arguments: json!({}),
            },
        }]);
        let history = vec![
            msg("system", "You are cdout."),
            msg("user", "list files"),
            call_msg,
            result,
        ];
        let (system, transcript) = render_conversation(&history, &[tool("run_shell")]);
        assert!(system.starts_with("You are cdout.\n\nHOW TO CALL A TOOL:"));
        assert!(system.contains("- run_shell: the run_shell tool"));
        assert!(transcript.contains("[user]\nlist files"));
        assert!(transcript.contains(
            "[assistant]\nListing.\n```tool\n{\"arguments\":{\"command\":\"ls\"},\"name\":\"run_shell\"}\n```"
        ));
        assert!(transcript.contains("[tool result: run_shell]\n[Exit code: 0 — Success]\na.mov"));
        assert!(!transcript.contains("You are cdout."));
        // A rendered call parses back into the same call.
        let block = &transcript[transcript.find("```tool").unwrap()..];
        let block = &block[..block.find("\n\n").unwrap_or(block.len())];
        let (_, calls) = parse_tool_blocks(block, &[tool("run_shell")]);
        assert_eq!(calls[0].function.arguments, json!({"command": "ls"}));
    }

    #[test]
    fn no_tools_means_plain_text() {
        let (system, _) = render_conversation(&[msg("system", "S")], &[]);
        assert!(system.contains("none are available"));
        assert!(!system.contains("HOW TO CALL A TOOL"));
    }

    #[test]
    fn failures_that_look_like_sign_in_say_how_to_fix_them() {
        let e = explain_failure(Backend::ClaudeCode, "Not logged in · Please run /login");
        assert!(e.contains("running `claude` in a terminal"), "{e}");
        let e = explain_failure(Backend::Codex, "usage limit reached");
        assert_eq!(e, "Codex: usage limit reached");
    }

    #[test]
    fn toml_path_uses_forward_slashes() {
        assert_eq!(
            toml_path(Path::new(r"C:\Users\me\cli-work\system.txt")),
            "\"C:/Users/me/cli-work/system.txt\""
        );
    }
}
