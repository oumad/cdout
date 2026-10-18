//! Shell-agnostic entry points.
//!
//! Each function here is the body of what used to be a Tauri command, moved
//! verbatim so the Tauri shell and the native shell share one implementation.
//! Shells add only transport (IPC, UI-thread messaging) and window handling.
//! Return types serialise to the exact JSON shapes the React frontend reads.

use std::sync::Arc;

use serde::Serialize;

use crate::constants;
use crate::features::agent::{self, AgentStepResult, ChunkSink};
use crate::features::loop_detector;
use crate::features::prompts;
use crate::features::sessions;
use crate::features::skills::{self, LoadedSkills};
use crate::llm::clients::{cli, ollama};
use crate::llm::Message;
use crate::platform;
use crate::tools;
use crate::utils::{cancel, config};

// --- Platform ---

/// Everything the UI needs to describe the host OS: which file manager to
/// name in labels, what the shell tool is called, how to render modifier keys.
/// Sent once at startup rather than sniffed from the user agent, so the
/// frontend and the system prompt can never disagree about the platform.
#[derive(Serialize, Clone, Debug)]
pub struct PlatformInfo {
    pub os: &'static str,
    pub os_name: &'static str,
    pub file_manager: &'static str,
    pub shell_name: &'static str,
    pub shell_tool_name: &'static str,
    /// This platform's built-in hotkey, so the UI can offer a Reset without
    /// hardcoding a value that differs per OS.
    pub default_hotkey: &'static str,
    /// Prose telling the model how to read a written-out file list, already
    /// substituted with `path` — the UI pastes this into its "files have
    /// changed" context updates.
    pub read_list_hint: String,
}

pub fn platform_info(list_file_path: Option<&str>) -> PlatformInfo {
    PlatformInfo {
        os: platform::OS_KEY,
        os_name: platform::OS_NAME,
        file_manager: platform::FILE_MANAGER,
        shell_name: platform::SHELL.name,
        shell_tool_name: platform::SHELL.tool_name,
        default_hotkey: config::DEFAULT_HOTKEY,
        read_list_hint: platform::with_list_file(
            platform::SHELL.read_list_hint,
            list_file_path.unwrap_or("{LIST_FILE}"),
        ),
    }
}

/// Open the OS pane where the user grants cdout permission to inspect the
/// file manager. On macOS a denied Apple-events prompt never reappears, so
/// without a deep link the only fix is a five-click hunt through System
/// Settings — the single most likely reason file context stays empty.
pub fn open_file_access_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation")
            .status()
            .map_err(|e| format!("Failed to open System Settings: {e}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Explorer introspection needs no grant, so there is no pane to open.
        Err(format!(
            "{} needs no permission grant on {}",
            platform::FILE_MANAGER,
            platform::OS_NAME
        ))
    }
}

// --- Models and providers ---

/// Verified OpenRouter model slugs (mid-2026). Pinned list — the model picker
/// surfaces these whenever the user has an OpenRouter key configured.
/// Refresh once a quarter or after OR announces deprecations.
pub const OPENROUTER_PAID_MODELS: &[&str] = &[
    "anthropic/claude-opus-4.7",
    "anthropic/claude-sonnet-4.6",
    "anthropic/claude-haiku-4.5",
    "openai/gpt-5",
    "openai/gpt-4.1",
    "google/gemini-3-pro-preview",
    "google/gemini-3-flash-preview",
    "google/gemini-2.5-flash",
];

/// Optional free-tier slugs (FP4 quantization risk + 20 req/min cap).
pub const OPENROUTER_FREE_MODELS: &[&str] =
    &["qwen/qwen3-coder-1m:free", "google/gemini-2.5-flash-lite"];

/// Direct Anthropic models — only surfaced when the user has an Anthropic
/// API key configured. They keep the prompt-caching path that OpenRouter's
/// OpenAI-compat wire drops.
pub const ANTHROPIC_DIRECT_MODELS: &[&str] = &[
    "anthropic:claude-opus-4-7",
    "anthropic:claude-sonnet-4-6",
    "anthropic:claude-haiku-4-5",
];

fn key_is_set(key: &Option<String>) -> bool {
    key.as_ref().is_some_and(|k| !k.trim().is_empty())
}

/// Every model the user can pick right now: pulled Ollama models, plus the
/// pinned cloud lists for whichever keys are configured. An unreachable
/// Ollama contributes nothing rather than failing the whole list.
pub async fn available_models() -> Result<Vec<String>, String> {
    #[cfg(feature = "fake-llm")]
    if crate::llm::fake::enabled() {
        return Ok(vec![crate::llm::fake::MODEL.to_string()]);
    }

    let url = config::get_ollama_url();
    let mut models: Vec<String> = ollama::list_models(&url)
        .await
        .map_err(|e| format!("Ollama connection failed: {}", e))
        .unwrap_or_default()
        .into_iter()
        .map(|m| format!("ollama:{}", m))
        .collect();

    let api_keys = config::load_api_keys();

    if key_is_set(&api_keys.openrouter) {
        models.extend(OPENROUTER_PAID_MODELS.iter().map(|m| m.to_string()));
        if config::get_show_free_openrouter_models() {
            models.extend(OPENROUTER_FREE_MODELS.iter().map(|m| m.to_string()));
        }
    }

    if key_is_set(&api_keys.anthropic) {
        models.extend(ANTHROPIC_DIRECT_MODELS.iter().map(|m| m.to_string()));
    }

    for status in cli::statuses().await {
        if status.signed_in {
            models.extend(status.backend.models().iter().map(|m| m.to_string()));
        }
    }

    Ok(models)
}

/// Open a terminal window on the CLI's own sign-in flow.
pub fn open_cli_sign_in(backend: cli::Backend) -> Result<(), String> {
    cli::open_sign_in(backend)
}

/// Snapshot of which providers are usable right now — drives the first-run
/// onboarding. Unlike [`available_models`] (which swallows the Ollama
/// connection error, so the UI can't tell "not installed" from "installed,
/// zero models"), this reports reachability honestly so the setup card can
/// say the right thing.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderStatus {
    pub ollama_reachable: bool,
    pub ollama_model_count: usize,
    pub openrouter_set: bool,
    pub anthropic_set: bool,
    /// Every known agent CLI (Claude Code, Codex): installed, signed in.
    pub subscriptions: Vec<cli::CliStatus>,
    /// True when at least one model can actually be selected: a CLI is
    /// signed in, a cloud key is set, or Ollama has ≥1 pulled model.
    pub any_usable: bool,
}

pub async fn provider_status() -> Result<ProviderStatus, String> {
    #[cfg(feature = "fake-llm")]
    if crate::llm::fake::enabled() {
        return Ok(ProviderStatus {
            ollama_reachable: true,
            ollama_model_count: 1,
            openrouter_set: false,
            anthropic_set: false,
            subscriptions: Vec::new(),
            any_usable: true,
        });
    }

    let url = config::get_ollama_url();
    let (ollama_reachable, ollama_model_count) = match ollama::list_models(&url).await {
        Ok(models) => (true, models.len()),
        Err(_) => (false, 0),
    };

    let keys = config::load_api_keys();
    let openrouter_set = key_is_set(&keys.openrouter);
    let anthropic_set = key_is_set(&keys.anthropic);
    let subscriptions = cli::statuses().await;
    let any_usable = ollama_model_count > 0
        || openrouter_set
        || anthropic_set
        || subscriptions.iter().any(|s| s.signed_in);

    Ok(ProviderStatus {
        ollama_reachable,
        ollama_model_count,
        openrouter_set,
        anthropic_set,
        subscriptions,
        any_usable,
    })
}

// --- Settings ---

pub fn set_ollama_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim().to_string();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }
    config::set_ollama_url(trimmed)
}

fn non_empty_key(key: &str) -> Result<String, String> {
    let trimmed = key.trim().to_string();
    if trimmed.is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    Ok(trimmed)
}

pub fn set_openrouter_key(key: &str) -> Result<(), String> {
    config::set_openrouter_key(non_empty_key(key)?)
}

pub fn set_anthropic_key(key: &str) -> Result<(), String> {
    config::set_anthropic_key(non_empty_key(key)?)
}

/// Masked view of stored API keys. Never exposes the full plaintext key to a
/// UI — the preview is the last 4 characters (e.g. "…3fa1"), enough for the
/// user to recognise which key is set without re-exposing it.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct MaskedApiKeys {
    pub openrouter_set: bool,
    pub openrouter_preview: Option<String>,
    pub anthropic_set: bool,
    pub anthropic_preview: Option<String>,
}

pub fn mask_key(key: &str) -> Option<String> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    if key.len() <= 4 {
        return Some("****".to_string());
    }
    let tail: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Some(format!("…{}", tail))
}

pub fn masked_api_keys() -> MaskedApiKeys {
    let keys = config::load_api_keys();
    let openrouter = keys.openrouter.as_deref().unwrap_or("");
    let anthropic = keys.anthropic.as_deref().unwrap_or("");
    MaskedApiKeys {
        openrouter_set: !openrouter.trim().is_empty(),
        openrouter_preview: mask_key(openrouter),
        anthropic_set: !anthropic.trim().is_empty(),
        anthropic_preview: mask_key(anthropic),
    }
}

// --- Legacy credentials ---

/// Migration-banner trigger: did the user have any of the now-removed CLI
/// credentials/Antigravity OAuth/openai/gemini keys present on disk before
/// upgrading? The UI shows the OpenRouter onboarding banner one time.
///
/// Side effect: if the legacy `api_keys.json` still mentions `openai` /
/// `gemini` fields, REWRITE it in the new shape so those plaintext keys
/// don't sit on disk indefinitely. The serde deserializer already drops the
/// unknown fields; we just persist the cleaned struct back.
pub fn has_legacy_credentials() -> Result<bool, String> {
    let cfg = dirs::config_dir();
    let mut found = false;

    // Claude Code's and Codex's own credential files are not legacy: those
    // CLIs are supported providers, and their files are theirs to manage.
    if let Some(c) = cfg.as_ref() {
        // Probe both the current data dir and the pre-rename one: the
        // shuttle-io → cdout migration merges old → new on every launch,
        // but a locked file can leave legacy artefacts stranded in the old
        // directory.
        for dir in [
            c.join(constants::APP_DATA_DIR_NAME),
            c.join(config::LEGACY_APP_DATA_DIR_NAME),
        ] {
            if dir.join("antigravity_credentials.json").exists() {
                found = true;
            }

            // api_keys.json that still mentions openai/gemini → rewrite in
            // place through the new struct shape (drops the legacy fields) so
            // those plaintext keys don't sit on disk, wherever the file is.
            let path = dir.join("api_keys.json");
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if raw.contains("\"openai\"") || raw.contains("\"gemini\"") {
                    found = true;
                    if let Ok(cleaned) = serde_json::from_str::<config::ApiKeys>(&raw) {
                        let rewrite = serde_json::to_string_pretty(&cleaned)
                            .map_err(|e| e.to_string())
                            .and_then(|json| {
                                std::fs::write(&path, json).map_err(|e| e.to_string())
                            });
                        if let Err(e) = rewrite {
                            eprintln!(
                                "[migration] Failed to purge legacy api_keys.json fields: {}",
                                e
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(found)
}

/// Remove cdout's own legacy credential file (`antigravity_credentials.json`).
/// Other tools' files are never touched.
pub fn cleanup_legacy_credentials() -> Result<(), String> {
    let mut errors: Vec<String> = Vec::new();

    if let Some(c) = dirs::config_dir() {
        // Both dirs, same reason as in `has_legacy_credentials`.
        for dir_name in [
            constants::APP_DATA_DIR_NAME,
            config::LEGACY_APP_DATA_DIR_NAME,
        ] {
            let path = c.join(dir_name).join("antigravity_credentials.json");
            if path.exists() {
                if let Err(e) = std::fs::remove_file(&path) {
                    errors.push(format!("{}: {}", path.display(), e));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Some files could not be removed: {}",
            errors.join("; ")
        ))
    }
}

// --- Skills ---

#[derive(Serialize, Clone, Debug)]
pub struct ListSkillsResult {
    pub skills: Vec<skills::Skill>,
    pub quarantined: Vec<skills::QuarantinedSkill>,
}

pub fn list_skills(loaded: LoadedSkills) -> ListSkillsResult {
    ListSkillsResult {
        skills: loaded.skills,
        quarantined: loaded.quarantined,
    }
}

// --- Conversation and sessions ---

/// Seed a conversation: the system prompt (with file-manager context and the
/// skills section) followed by the user's first message.
pub fn init_agent_conversation(
    context_path: &str,
    selected_files: &[String],
    user_prompt: &str,
    model: Option<&str>,
    loaded: &LoadedSkills,
) -> Result<Vec<Message>, String> {
    let skills_section = skills::get_skills_prompt_section(&loaded.skills);
    // Small local models (Ollama) often don't emit native tool_calls reliably
    // and benefit from the longer prose tool catalog + WRONG/CORRECT example.
    // Cloud providers (Anthropic, OpenAI, OpenRouter) ship native tool schemas
    // alongside the prompt so we can stay terse.
    let sends_native_tool_specs = !model.map(|m| m.starts_with("ollama:")).unwrap_or(false);
    let system_prompt = prompts::build_system_prompt(
        context_path,
        selected_files,
        &skills_section,
        sends_native_tool_specs,
    )?;
    Ok(vec![
        Message {
            role: "system".to_string(),
            content: system_prompt,
            tool_calls: None,
            synthetic: false,
        },
        Message {
            role: "user".to_string(),
            content: user_prompt.to_string(),
            tool_calls: None,
            synthetic: false,
        },
    ])
}

/// Create and persist a session seeded by [`init_agent_conversation`].
pub fn create_session(
    user_prompt: &str,
    context_path: &str,
    selected_files: &[String],
    model: &str,
    loaded: &LoadedSkills,
) -> Result<sessions::Session, String> {
    let initial = init_agent_conversation(
        context_path,
        selected_files,
        user_prompt,
        Some(model),
        loaded,
    )?;
    sessions::create_session(
        user_prompt,
        context_path,
        selected_files.len(),
        model,
        initial,
    )
}

pub fn save_session_messages(
    id: &str,
    messages: Vec<Message>,
) -> Result<sessions::SessionMeta, String> {
    sessions::save_messages(id, messages, chrono::Utc::now().timestamp_millis())
}

/// Write a long selection to a temp file the model can read with the shell,
/// instead of inlining hundreds of paths into the prompt.
pub fn write_file_list(files: &[String]) -> Result<String, String> {
    use std::io::Write;
    let temp_path = std::env::temp_dir().join("cdout_files.txt");
    let mut file = std::fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    for f in files {
        writeln!(file, "{}", f).map_err(|e| format!("Failed to write to temp file: {}", e))?;
    }
    Ok(temp_path.to_string_lossy().to_string())
}

// --- Agent ---

/// One streaming agent step. Clears any earlier cancel request, resets the
/// loop detector when the conversation has no assistant turn yet, then runs
/// the step, sending chunks to `sink`.
///
/// `drop_tools` is set when the previous turn's loop verdict was `Break`:
/// tool definitions are withheld so the model can only explain or stop.
pub async fn run_agent_step(
    model: String,
    history: Vec<Message>,
    drop_tools: bool,
    sink: Arc<dyn ChunkSink>,
) -> Result<AgentStepResult, String> {
    cancel::reset();
    let assistant_turns = history.iter().filter(|m| m.role == "assistant").count();
    if assistant_turns == 0 {
        loop_detector::reset_global();
    }
    agent::run_agent_step_stream(model, history, drop_tools, sink).await
}

/// Run an approved shell command and feed the outcome to the loop detector
/// so its no-progress signal works. Blocking: call from a worker thread.
pub fn execute_shell_command(command: &str, cwd: Option<&str>) -> Result<String, String> {
    let registry = tools::build_default_registry();
    let args = serde_json::json!({ "command": command });
    let result = registry.validate_and_execute(constants::TOOL_NAME, &args, cwd)?;
    loop_detector::record_outcome_global(!result.is_error);
    Ok(result.output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_key_shapes() {
        assert_eq!(mask_key(""), None);
        assert_eq!(mask_key("   "), None);
        assert_eq!(mask_key("abcd"), Some("****".to_string()));
        assert_eq!(mask_key("sk-or-123456"), Some("…3456".to_string()));
        assert_eq!(mask_key("  sk-or-123456  "), Some("…3456".to_string()));
    }

    #[test]
    fn platform_info_substitutes_list_path() {
        let info = platform_info(Some("C:/tmp/list.txt"));
        assert!(info.read_list_hint.contains("C:/tmp/list.txt"));
        assert!(!info.read_list_hint.contains("{LIST_FILE}"));
        let unsubstituted = platform_info(None);
        assert!(unsubstituted.read_list_hint.contains("{LIST_FILE}"));
        assert_eq!(info.shell_tool_name, platform::SHELL.tool_name);
    }

    #[test]
    fn ollama_url_must_be_http() {
        assert!(set_ollama_url("localhost:11434").is_err());
        assert!(set_ollama_url("ftp://x").is_err());
    }

    #[test]
    fn empty_keys_are_rejected() {
        assert!(non_empty_key("   ").is_err());
        assert_eq!(non_empty_key(" k ").unwrap(), "k");
    }

    #[test]
    fn init_conversation_seeds_system_then_user() {
        let loaded = LoadedSkills {
            skills: Vec::new(),
            quarantined: Vec::new(),
        };
        let msgs = init_agent_conversation(
            "C:/work",
            &["a.mp4".to_string()],
            "rename them",
            Some("ollama:x"),
            &loaded,
        )
        .unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "system");
        assert!(msgs[0].content.contains("C:/work"));
        assert_eq!(msgs[1].role, "user");
        assert_eq!(msgs[1].content, "rename them");
    }
}
