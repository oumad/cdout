//! Tauri shell. All behaviour lives in `cdout_core`; the commands below only
//! adapt it to Tauri IPC, and the rest of this file is window, tray and
//! hotkey glue.

use std::sync::Arc;

use cdout_core::api;
use cdout_core::features::agent::AgentStepResult;
use cdout_core::features::{explorer, loop_detector, sessions, skills};
use cdout_core::llm::{Message, StreamChunk};
use cdout_core::tools;
use cdout_core::utils::{cancel, config};

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Bundled skills ship as Tauri resources; user skills come from the data dir.
fn load_skills(app_handle: &tauri::AppHandle) -> skills::LoadedSkills {
    skills::load_all_skills(app_handle.path().resource_dir().ok())
}

#[tauri::command]
fn get_explorer_status() -> Result<explorer::ExplorerState, String> {
    explorer::get_active_explorer_info().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_explorer_debug() -> Result<explorer::ExplorerDebugInfo, String> {
    explorer::get_explorer_debug_info().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_approval_mode() -> config::ApprovalMode {
    config::get_approval_mode()
}

#[tauri::command]
fn set_approval_mode(mode: config::ApprovalMode) -> Result<(), String> {
    config::set_approval_mode(mode)
}

/// Classify a proposed command so the frontend can decide whether it may run
/// without a click. Kept in the backend beside the tool that executes it —
/// a renderer-side allowlist would be one XSS away from being advisory.
#[tauri::command]
fn classify_command(command: String) -> tools::risk::CommandRisk {
    tools::risk::classify(&command)
}

#[tauri::command]
fn open_file_access_settings() -> Result<(), String> {
    api::open_file_access_settings()
}

#[tauri::command]
fn get_platform_info(list_file_path: Option<String>) -> api::PlatformInfo {
    api::platform_info(list_file_path.as_deref())
}

#[tauri::command]
async fn get_ollama_models() -> Result<Vec<String>, String> {
    api::available_models().await
}

#[tauri::command]
async fn get_provider_status() -> Result<api::ProviderStatus, String> {
    api::provider_status().await
}

#[tauri::command]
fn get_ollama_url() -> Result<String, String> {
    Ok(config::get_ollama_url())
}

#[tauri::command]
fn set_ollama_url(url: String) -> Result<(), String> {
    api::set_ollama_url(&url)
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
    api::set_openrouter_key(&key)
}

#[tauri::command]
async fn set_anthropic_key(key: String) -> Result<(), String> {
    api::set_anthropic_key(&key)
}

/// Masked view of stored API keys — the renderer never sees plaintext.
#[tauri::command]
async fn get_api_keys() -> Result<api::MaskedApiKeys, String> {
    Ok(api::masked_api_keys())
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

#[tauri::command]
fn has_legacy_credentials() -> Result<bool, String> {
    api::has_legacy_credentials()
}

#[tauri::command]
fn cleanup_legacy_credentials(remove_third_party: bool) -> Result<(), String> {
    api::cleanup_legacy_credentials(remove_third_party)
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
    api::init_agent_conversation(
        &context_path,
        &selected_files,
        &user_prompt,
        model.as_deref(),
        &load_skills(&app_handle),
    )
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
    api::create_session(
        &user_prompt,
        &context_path,
        &selected_files,
        &model,
        &load_skills(&app_handle),
    )
}

#[tauri::command]
fn save_session_messages(
    id: String,
    messages: Vec<Message>,
) -> Result<sessions::SessionMeta, String> {
    api::save_session_messages(&id, messages)
}

#[tauri::command]
fn delete_session(id: String) -> Result<(), String> {
    sessions::delete_session(&id)
}

#[tauri::command]
fn rename_session(id: String, title: String) -> Result<sessions::SessionMeta, String> {
    sessions::rename_session(&id, &title)
}

#[tauri::command]
fn list_skills(app_handle: tauri::AppHandle) -> Result<api::ListSkillsResult, String> {
    Ok(api::list_skills(load_skills(&app_handle)))
}

#[tauri::command]
async fn run_agent_step_stream(
    model: String,
    history: Vec<Message>,
    drop_tools: Option<bool>,
    on_chunk: tauri::ipc::Channel<StreamChunk>,
) -> Result<AgentStepResult, String> {
    let sink = Arc::new(move |chunk: StreamChunk| {
        let _ = on_chunk.send(chunk);
    });
    api::run_agent_step(model, history, drop_tools.unwrap_or(false), sink).await
}

#[tauri::command]
fn cancel_stream() {
    cancel::request();
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
    api::execute_shell_command(&command, cwd.as_deref())
}

#[tauri::command]
fn reset_loop_detector() {
    loop_detector::reset_global();
}

#[tauri::command]
fn get_hotkey() -> Result<String, String> {
    Ok(config::get_hotkey())
}

/// Point the global shortcut at `hotkey`, replacing whatever was registered.
///
/// `unregister_all` rather than unregistering a remembered shortcut: the app
/// only ever owns one, and this way a half-failed previous attempt cannot
/// leave a stale binding alive that still opens the spotlight.
fn register_hotkey(app: &tauri::AppHandle, hotkey: &str) -> Result<(), String> {
    let shortcut = hotkey
        .parse::<Shortcut>()
        .map_err(|e| format!("Invalid hotkey '{hotkey}': {e}"))?;

    let _ = app.global_shortcut().unregister_all();

    let app_for_handler = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                if let Some(spotlight) = app_for_handler.get_webview_window("spotlight") {
                    center_on_cursor_monitor(&spotlight);
                    show_window(&spotlight);
                }
            }
        })
        .map_err(|e| {
            // Almost always "another app already owns this chord". Say so,
            // because the OS gives no other feedback.
            format!(
                "Could not register '{hotkey}': {e}. Another application may                  already be using that combination."
            )
        })
}

#[tauri::command]
fn set_hotkey(hotkey: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    // Take effect immediately. Persisting without re-registering would leave
    // the UI showing a hotkey that does nothing until the next launch, which
    // reads as a broken setting rather than a pending one.
    register_hotkey(&app_handle, &hotkey)?;
    // Only reaches disk once the OS has accepted it, so a rejected chord
    // cannot become the value loaded at next startup.
    config::set_hotkey(hotkey)
}

#[tauri::command]
fn write_file_list(files: Vec<String>) -> Result<String, String> {
    api::write_file_list(&files)
}

#[tauri::command]
fn spotlight_submit(
    prompt: String,
    model: String,
    // `auto_approve` is true when the user submitted with the modifier held:
    // run this whole task unattended without waiting for the first proposal.
    auto_approve: bool,
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
                    "model": model,
                    "auto_approve": auto_approve
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
            open_file_access_settings,
            get_approval_mode,
            set_approval_mode,
            classify_command,
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

            tray.on_menu_event(move |app_handle: &tauri::AppHandle, event| {
                match event.id().as_ref() {
                    "show" => {
                        if let Some(w) = app_handle.get_webview_window("main") {
                            show_window(&w);
                        }
                    }
                    "quit" => {
                        app_handle.exit(0);
                    }
                    _ => {}
                }
            })
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
            // A corrupt, unsupported or already-taken hotkey in config must
            // NOT brick startup: fall back to the built-in default, which is
            // a compile-time-known-good literal.
            let hotkey_str = config::get_hotkey();
            if let Err(e) = register_hotkey(app.handle(), &hotkey_str) {
                eprintln!("[hotkey] {e}; falling back to '{}'", config::DEFAULT_HOTKEY);
                if let Err(e) = register_hotkey(app.handle(), config::DEFAULT_HOTKEY) {
                    // Nothing left to try — the spotlight is still reachable
                    // from the tray, so run on rather than aborting launch.
                    eprintln!("[hotkey] default also failed: {e}");
                }
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

#[cfg(test)]
mod tests {
    #[test]
    fn default_hotkey_parses() {
        // `run()` falls back to this default when the configured hotkey fails
        // to register, so a malformed default would leave no hotkey at all.
        // The macOS default differs from the Windows one, which is exactly
        // the kind of divergence that goes unnoticed until launch day.
        use cdout_core::utils::config::DEFAULT_HOTKEY;
        use tauri_plugin_global_shortcut::Shortcut;
        DEFAULT_HOTKEY
            .parse::<Shortcut>()
            .unwrap_or_else(|e| panic!("DEFAULT_HOTKEY '{DEFAULT_HOTKEY}' must parse: {e}"));
    }
}
