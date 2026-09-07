mod constants;
mod features;
mod llm;
mod platform;
mod tools;
mod utils;

use features::agent;
use features::explorer;
use features::loop_detector;
use features::prompts;
use features::sessions;
use features::skills;
use llm::clients::ollama;
use llm::{Message, StreamChunk};
use utils::config;

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use features::agent::AgentStepResult;

#[tauri::command]
fn get_explorer_status() -> Result<explorer::ExplorerState, String> {
    explorer::get_active_explorer_info().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_explorer_debug() -> Result<explorer::ExplorerDebugInfo, String> {
    explorer::get_explorer_debug_info().map_err(|e| e.to_string())
}

/// Everything the UI needs to describe the host OS: which file manager to
/// name in labels, what the shell tool is called, how to render modifier keys.
/// Sent once at startup rather than sniffed from the user agent, so the
/// frontend and the system prompt can never disagree about the platform.
#[derive(serde::Serialize)]
struct PlatformInfo {
    os: &'static str,
    os_name: &'static str,
    file_manager: &'static str,
    shell_name: &'static str,
    shell_tool_name: &'static str,
    /// Prose telling the model how to read a written-out file list, already
    /// substituted with `path` — the frontend pastes this into its
    /// "files have changed" context updates.
    read_list_hint: String,
}

#[tauri::command]
fn get_platform_info(list_file_path: Option<String>) -> PlatformInfo {
    PlatformInfo {
        os: platform::OS_KEY,
        os_name: platform::OS_NAME,
        file_manager: platform::FILE_MANAGER,
        shell_name: platform::SHELL.name,
        shell_tool_name: platform::SHELL.tool_name,
        read_list_hint: platform::with_list_file(
            platform::SHELL.read_list_hint,
            list_file_path.as_deref().unwrap_or("{LIST_FILE}"),
        ),
    }
}

/// Verified OpenRouter model slugs (mid-2026). Pinned list — `useModels`
/// surfaces these whenever the user has an OpenRouter key configured.
/// Refresh once a quarter or after OR announces deprecations.
const OPENROUTER_PAID_MODELS: &[&str] = &[
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
const OPENROUTER_FREE_MODELS: &[&str] = &[
    "qwen/qwen3-coder-1m:free",
    "google/gemini-2.5-flash-lite",
];

/// Direct Anthropic models — only surfaced when the user has an Anthropic
/// API key configured. They keep the prompt-caching path that OpenRouter's
/// OpenAI-compat wire drops.
const ANTHROPIC_DIRECT_MODELS: &[&str] = &[
    "anthropic:claude-opus-4-7",
    "anthropic:claude-sonnet-4-6",
    "anthropic:claude-haiku-4-5",
];

#[tauri::command]
async fn get_ollama_models() -> Result<Vec<String>, String> {
    let url = config::get_ollama_url();
    let mut models: Vec<String> = ollama::list_models(&url)
        .await
        .map_err(|e| format!("Ollama connection failed: {}", e))
        .unwrap_or_default()
        .into_iter()
        .map(|m| format!("ollama:{}", m))
        .collect();

    let api_keys = config::load_api_keys();

    if api_keys
        .openrouter
        .as_ref()
        .is_some_and(|k| !k.trim().is_empty())
    {
        for m in OPENROUTER_PAID_MODELS {
            models.push((*m).to_string());
        }
        if config::get_show_free_openrouter_models() {
            for m in OPENROUTER_FREE_MODELS {
                models.push((*m).to_string());
            }
        }
    }

    if api_keys
        .anthropic
        .as_ref()
        .is_some_and(|k| !k.trim().is_empty())
    {
        for m in ANTHROPIC_DIRECT_MODELS {
            models.push((*m).to_string());
        }
    }

    Ok(models)
}

/// Snapshot of which providers are usable right now — drives the first-run
/// onboarding. Unlike `get_ollama_models` (which swallows the Ollama
/// connection error via `unwrap_or_default`, so the UI can't tell "not
/// installed" from "installed, zero models"), this reports reachability
/// honestly so the setup card can say the right thing.
#[derive(serde::Serialize)]
struct ProviderStatus {
    ollama_reachable: bool,
    ollama_model_count: usize,
    openrouter_set: bool,
    anthropic_set: bool,
    /// True when at least one model can actually be selected: a cloud key is
    /// set, or Ollama has ≥1 pulled model.
    any_usable: bool,
}

#[tauri::command]
async fn get_provider_status() -> Result<ProviderStatus, String> {
    let url = config::get_ollama_url();
    let (ollama_reachable, ollama_model_count) = match ollama::list_models(&url).await {
        Ok(models) => (true, models.len()),
        Err(_) => (false, 0),
    };

    let keys = config::load_api_keys();
    let openrouter_set = keys
        .openrouter
        .as_ref()
        .is_some_and(|k| !k.trim().is_empty());
    let anthropic_set = keys
        .anthropic
        .as_ref()
        .is_some_and(|k| !k.trim().is_empty());

    Ok(ProviderStatus {
        ollama_reachable,
        ollama_model_count,
        openrouter_set,
        anthropic_set,
        any_usable: ollama_model_count > 0 || openrouter_set || anthropic_set,
    })
}

#[tauri::command]
fn get_ollama_url() -> Result<String, String> {
    Ok(config::get_ollama_url())
}

#[tauri::command]
fn set_ollama_url(url: String) -> Result<(), String> {
    let trimmed = url.trim().to_string();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }
    config::set_ollama_url(trimmed)
}

#[tauri::command]
fn get_selected_model() -> Result<Option<String>, String> {
    Ok(config::get_selected_model())
}

#[tauri::command]
fn set_selected_model(model: String) -> Result<(), String> {
    config::set_selected_model(model)
}

#[tauri::command]
async fn set_openrouter_key(key: String) -> Result<(), String> {
    let trimmed = key.trim().to_string();
    if trimmed.is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    config::set_openrouter_key(trimmed)
}

#[tauri::command]
async fn set_anthropic_key(key: String) -> Result<(), String> {
    let trimmed = key.trim().to_string();
    if trimmed.is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    config::set_anthropic_key(trimmed)
}

/// Masked view of stored API keys. Never returns the full plaintext key to
/// the renderer — that would land in React state and be DOM-recoverable via
/// devtools. The preview is the last 4 characters (e.g. "...3fa1") which is
/// enough for the user to recognize which key is set without re-exposing it.
#[derive(serde::Serialize)]
struct MaskedApiKeys {
    openrouter_set: bool,
    openrouter_preview: Option<String>,
    anthropic_set: bool,
    anthropic_preview: Option<String>,
}

fn mask_key(key: &str) -> Option<String> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    if key.len() <= 4 {
        return Some("****".to_string());
    }
    let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    Some(format!("…{}", tail))
}

#[tauri::command]
async fn get_api_keys() -> Result<MaskedApiKeys, String> {
    let keys = config::load_api_keys();
    let openrouter = keys.openrouter.as_deref().unwrap_or("");
    let anthropic = keys.anthropic.as_deref().unwrap_or("");
    Ok(MaskedApiKeys {
        openrouter_set: !openrouter.trim().is_empty(),
        openrouter_preview: mask_key(openrouter),
        anthropic_set: !anthropic.trim().is_empty(),
        anthropic_preview: mask_key(anthropic),
    })
}

#[tauri::command]
fn get_show_free_openrouter_models() -> Result<bool, String> {
    Ok(config::get_show_free_openrouter_models())
}

#[tauri::command]
fn set_show_free_openrouter_models(enabled: bool) -> Result<(), String> {
    config::set_show_free_openrouter_models(enabled)
}

#[tauri::command]
fn get_openrouter_disclosure_ack() -> Result<bool, String> {
    Ok(config::get_openrouter_disclosure_ack())
}

#[tauri::command]
fn set_openrouter_disclosure_ack(ack: bool) -> Result<(), String> {
    config::set_openrouter_disclosure_ack(ack)
}

/// Migration-banner trigger: did the user have any of the now-removed CLI
/// credentials/Antigravity OAuth/openai/gemini keys present on disk before
/// upgrading? Frontend shows the OpenRouter onboarding banner one time.
///
/// Side effect: if the legacy `api_keys.json` still mentions `openai` /
/// `gemini` fields, REWRITE it in the new shape so those plaintext keys
/// don't sit on disk indefinitely. The serde deserializer already drops the
/// unknown fields; we just persist the cleaned struct back.
#[tauri::command]
fn has_legacy_credentials() -> Result<bool, String> {
    let home = dirs::home_dir();
    let cfg = dirs::config_dir();
    let mut found = false;

    if let Some(h) = home.as_ref() {
        if h.join(".claude").join(".credentials.json").exists() {
            found = true;
        }
        if h.join(".codex").join("auth.json").exists() {
            found = true;
        }
    }
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

/// Clean up the legacy on-disk artefacts a user may still have around after
/// the OpenRouter migration. Always removes cdout's own
/// `antigravity_credentials.json`. Removes the third-party CLI creds
/// (~/.claude, ~/.codex) ONLY when `remove_third_party` is true — those
/// belong to other tools, so we ask the user first.
#[tauri::command]
fn cleanup_legacy_credentials(remove_third_party: bool) -> Result<(), String> {
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

    if remove_third_party {
        if let Some(h) = dirs::home_dir() {
            for p in [
                h.join(".claude").join(".credentials.json"),
                h.join(".codex").join("auth.json"),
            ] {
                if p.exists() {
                    if let Err(e) = std::fs::remove_file(&p) {
                        errors.push(format!("{}: {}", p.display(), e));
                    }
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

// --- Agent Commands ---

#[tauri::command]
fn init_agent_conversation(
    context_path: String,
    selected_files: Vec<String>,
    user_prompt: String,
    model: Option<String>,
    app_handle: tauri::AppHandle,
) -> Result<Vec<Message>, String> {
    let resource_dir = app_handle.path().resource_dir().ok();
    let loaded = skills::load_all_skills(resource_dir);
    let skills_section = skills::get_skills_prompt_section(&loaded.skills);
    // Small local models (Ollama) often don't emit native tool_calls reliably
    // and benefit from the longer prose tool catalog + WRONG/CORRECT example.
    // Cloud providers (Anthropic, OpenAI, OpenRouter) ship native tool schemas
    // alongside the prompt so we can stay terse.
    let sends_native_tool_specs = !model
        .as_deref()
        .map(|m| m.starts_with("ollama:"))
        .unwrap_or(false);
    let system_prompt = prompts::build_system_prompt(
        &context_path,
        &selected_files,
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
            content: user_prompt,
            tool_calls: None,
            synthetic: false,
        },
    ])
}

// --- Session commands ---

#[tauri::command]
fn list_sessions() -> Result<Vec<sessions::SessionMeta>, String> {
    sessions::list_sessions()
}

#[tauri::command]
fn load_session(id: String) -> Result<sessions::Session, String> {
    sessions::load_session(&id)
}

#[tauri::command]
fn create_session(
    user_prompt: String,
    context_path: String,
    selected_files: Vec<String>,
    model: String,
    app_handle: tauri::AppHandle,
) -> Result<sessions::Session, String> {
    let initial = init_agent_conversation(
        context_path.clone(),
        selected_files.clone(),
        user_prompt.clone(),
        Some(model.clone()),
        app_handle,
    )?;
    sessions::create_session(
        &user_prompt,
        &context_path,
        selected_files.len(),
        &model,
        initial,
    )
}

#[tauri::command]
fn save_session_messages(
    id: String,
    messages: Vec<Message>,
) -> Result<sessions::SessionMeta, String> {
    sessions::save_messages(&id, messages, chrono::Utc::now().timestamp_millis())
}

#[tauri::command]
fn delete_session(id: String) -> Result<(), String> {
    sessions::delete_session(&id)
}

#[tauri::command]
fn rename_session(id: String, title: String) -> Result<sessions::SessionMeta, String> {
    sessions::rename_session(&id, &title)
}

#[derive(serde::Serialize)]
struct ListSkillsResult {
    skills: Vec<skills::Skill>,
    quarantined: Vec<skills::QuarantinedSkill>,
}

#[tauri::command]
fn list_skills(app_handle: tauri::AppHandle) -> Result<ListSkillsResult, String> {
    let resource_dir = app_handle.path().resource_dir().ok();
    let loaded = skills::load_all_skills(resource_dir);
    Ok(ListSkillsResult {
        skills: loaded.skills,
        quarantined: loaded.quarantined,
    })
}

#[tauri::command]
async fn run_agent_step_stream(
    model: String,
    history: Vec<Message>,
    drop_tools: Option<bool>,
    on_chunk: tauri::ipc::Channel<StreamChunk>,
) -> Result<AgentStepResult, String> {
    utils::cancel::reset();
    // Reset the loop detector when starting a fresh conversation (history is
    // just the system prompt + initial user prompt — no assistant turns yet).
    let assistant_turns = history.iter().filter(|m| m.role == "assistant").count();
    if assistant_turns == 0 {
        loop_detector::reset_global();
    }
    // When the previous turn's loop_verdict was Break, the frontend passes
    // `drop_tools: true`. We force a text-only reassessment by withholding
    // tool definitions — the model can't propose another action, only
    // explain or stop. This is the documented Break contract.
    agent::run_agent_step_stream(model, history, drop_tools.unwrap_or(false), on_chunk).await
}

#[tauri::command]
fn cancel_stream() {
    utils::cancel::request();
}

#[tauri::command]
fn get_running_command() -> Option<tools::shell::RunningCommand> {
    tools::shell::get_running()
}

#[tauri::command]
fn kill_running_command() -> Result<(), String> {
    tools::shell::kill_running()
}

#[tauri::command]
async fn execute_shell_command(command: String, cwd: Option<String>) -> Result<String, String> {
    let registry = tools::build_default_registry();
    let args = serde_json::json!({ "command": command });
    let result = registry.validate_and_execute(constants::TOOL_NAME, &args, cwd.as_deref())?;
    // Feed the outcome back to the loop detector so its no-progress signal works.
    loop_detector::record_outcome_global(!result.is_error);
    Ok(result.output)
}

#[tauri::command]
fn reset_loop_detector() {
    loop_detector::reset_global();
}

#[tauri::command]
fn get_hotkey() -> Result<String, String> {
    Ok(config::get_hotkey())
}

#[tauri::command]
fn set_hotkey(hotkey: String) -> Result<(), String> {
    // Reject an unparseable hotkey at the write boundary so a bad value never
    // reaches disk (the startup path also falls back gracefully, but this
    // gives the user immediate feedback instead of a silent default swap on
    // next launch).
    hotkey
        .parse::<Shortcut>()
        .map_err(|e| format!("Invalid hotkey '{hotkey}': {e}"))?;
    config::set_hotkey(hotkey)
}

#[tauri::command]
fn write_file_list(files: Vec<String>) -> Result<String, String> {
    use std::io::Write;
    let temp_path = std::env::temp_dir().join("cdout_files.txt");
    let mut file = std::fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    for f in &files {
        writeln!(file, "{}", f).map_err(|e| format!("Failed to write to temp file: {}", e))?;
    }
    Ok(temp_path.to_string_lossy().to_string())
}

#[tauri::command]
fn spotlight_submit(
    prompt: String,
    model: String,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    if let Some(spotlight) = app_handle.get_webview_window("spotlight") {
        spotlight.hide().ok();
    }
    if let Some(main_win) = app_handle.get_webview_window("main") {
        center_on_cursor_monitor(&main_win);
        show_window(&main_win);
        main_win
            .emit(
                "spotlight-submitted",
                serde_json::json!({
                    "prompt": prompt,
                    "model": model
                }),
            )
            .ok();
    }
    Ok(())
}

/// Position a window centered on the monitor where the mouse cursor currently is.
fn center_on_cursor_monitor(window: &tauri::WebviewWindow) {
    let cursor = match window.cursor_position() {
        Ok(pos) => pos,
        Err(_) => return,
    };
    let monitors = match window.available_monitors() {
        Ok(m) => m,
        Err(_) => return,
    };
    let win_size = match window.outer_size() {
        Ok(s) => s,
        Err(_) => return,
    };

    for monitor in monitors {
        let pos = monitor.position();
        let size = monitor.size();

        if cursor.x >= pos.x as f64
            && cursor.x < (pos.x + size.width as i32) as f64
            && cursor.y >= pos.y as f64
            && cursor.y < (pos.y + size.height as i32) as f64
        {
            let x = pos.x as f64 + (size.width as f64 - win_size.width as f64) / 2.0;
            let y = pos.y as f64 + (size.height as f64 - win_size.height as f64) / 2.0;

            use tauri::PhysicalPosition;
            window
                .set_position(PhysicalPosition::new(x as i32, y as i32))
                .ok();
            return;
        }
    }
}

fn show_window(window: &tauri::WebviewWindow) {
    window.show().ok();
    window.unminimize().ok();
    window.set_focus().ok();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Must run before anything touches the data dir (config reads, session
    // dir creation), or the freshly created new dir would block the move.
    config::migrate_legacy_data_dir();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_explorer_status,
            get_explorer_debug,
            get_platform_info,
            get_ollama_models,
            get_provider_status,
            get_ollama_url,
            set_ollama_url,
            get_selected_model,
            set_selected_model,
            set_openrouter_key,
            set_anthropic_key,
            get_api_keys,
            get_show_free_openrouter_models,
            set_show_free_openrouter_models,
            get_openrouter_disclosure_ack,
            set_openrouter_disclosure_ack,
            has_legacy_credentials,
            cleanup_legacy_credentials,
            get_hotkey,
            set_hotkey,
            init_agent_conversation,
            list_sessions,
            load_session,
            create_session,
            save_session_messages,
            delete_session,
            rename_session,
            list_skills,
            run_agent_step_stream,
            execute_shell_command,
            reset_loop_detector,
            cancel_stream,
            get_running_command,
            kill_running_command,
            write_file_list,
            spotlight_submit,
        ])
        .setup(|app| {
            let main_window = app.get_webview_window("main").unwrap();
            let spotlight_window = app.get_webview_window("spotlight").unwrap();

            // Only spotlight loses shadow (main keeps DWM border)
            spotlight_window.set_shadow(false).ok();

            // A hotkey-triggered palette is useless if it opens on the Space
            // the user left. No-op on Windows.
            spotlight_window.set_visible_on_all_workspaces(true).ok();

            // --- System Tray ---
            let show_item = MenuItemBuilder::with_id("show", "Show cdout").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit cdout").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            #[allow(unused_mut)]
            let mut tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap())
                .tooltip("cdout")
                .menu(&tray_menu);
            #[cfg(target_os = "macos")]
            {
                // Menu-bar icons must be template images, or the fixed-colour
                // PNG stays dark on a dark menu bar.
                tray = tray.icon_as_template(true);
            }

            tray.on_menu_event(
                    move |app_handle: &tauri::AppHandle, event| match event.id().as_ref() {
                        "show" => {
                            if let Some(w) = app_handle.get_webview_window("main") {
                                show_window(&w);
                            }
                        }
                        "quit" => {
                            app_handle.exit(0);
                        }
                        _ => {}
                    },
                )
                .on_tray_icon_event({
                    let w = main_window.clone();
                    move |_tray, event| {
                        if let tauri::tray::TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            ..
                        } = event
                        {
                            show_window(&w);
                        }
                    }
                })
                .build(app)?;

            // --- Global Hotkey ---
            // A corrupt/unsupported hotkey string in config must NOT brick
            // startup. Fall back to the built-in default; the default is a
            // compile-time-known-good literal so its parse cannot fail.
            let hotkey_str = config::get_hotkey();
            let hotkey_spotlight = spotlight_window.clone();
            let shortcut = match hotkey_str.parse::<Shortcut>() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "[hotkey] Invalid hotkey '{hotkey_str}' in config ({e}); \
                         falling back to default '{}'",
                        config::DEFAULT_HOTKEY
                    );
                    config::DEFAULT_HOTKEY
                        .parse::<Shortcut>()
                        .expect("built-in default hotkey must parse")
                }
            };

            // Unregister first in case a previous instance left it registered
            let _ = app.global_shortcut().unregister(shortcut);

            if let Err(e) = app.global_shortcut().on_shortcut(
                shortcut,
                move |_app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        center_on_cursor_monitor(&hotkey_spotlight);
                        show_window(&hotkey_spotlight);
                    }
                },
            ) {
                eprintln!(
                    "Warning: Failed to register global hotkey '{}': {}",
                    hotkey_str, e
                );
            }

            // --- Close to Tray (main window only) ---
            let close_window = main_window.clone();
            main_window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    close_window.hide().ok();
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // macOS keeps a dock icon for an app whose windows are all
            // hidden, and clicking it emits Reopen rather than re-running
            // setup. Without this, close-to-tray makes the app look dead to
            // anyone who reaches for the dock instead of the menu bar.
            #[cfg(target_os = "macos")]
            if matches!(event, tauri::RunEvent::Reopen { .. }) {
                if let Some(w) = app_handle.get_webview_window("main") {
                    show_window(&w);
                }
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app_handle, event);
        });
}
