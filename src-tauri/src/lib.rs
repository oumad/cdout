mod auth;
mod constants;
mod features;
mod llm;
mod utils;

use auth::antigravity;
use features::agent;
use features::explorer;
use features::prompts;
use llm::clients::ollama;
use llm::Message;
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

#[tauri::command]
async fn get_ollama_models() -> Result<Vec<String>, String> {
    let url = config::get_ollama_url();
    let mut models = ollama::list_models(&url)
        .await
        .map_err(|e| format!("Ollama connection failed: {}", e))
        .unwrap_or_default();

    if antigravity::load_credentials().is_some() {
        models.push("antigravity (gemini-3-flash)".to_string());
    }

    let api_keys = config::load_api_keys();
    if api_keys.openai.as_ref().is_some_and(|k| !k.is_empty()) {
        models.push("openai:gpt-4o".to_string());
    }
    if api_keys.gemini.as_ref().is_some_and(|k| !k.is_empty()) {
        models.push("gemini:gemini-1.5-pro".to_string());
    }

    Ok(models)
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
async fn login_antigravity() -> Result<String, String> {
    antigravity::perform_login()
        .await
        .map(|c| c.email.unwrap_or_else(|| "Logged in".to_string()))
}

#[tauri::command]
async fn get_antigravity_status() -> Result<Option<String>, String> {
    Ok(antigravity::load_credentials().and_then(|c| c.email))
}

#[tauri::command]
async fn set_openai_key(key: String) -> Result<(), String> {
    let trimmed = key.trim().to_string();
    if trimmed.is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    config::set_openai_key(trimmed)
}

#[tauri::command]
async fn set_gemini_key(key: String) -> Result<(), String> {
    let trimmed = key.trim().to_string();
    if trimmed.is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    config::set_gemini_key(trimmed)
}

#[tauri::command]
async fn get_api_keys() -> Result<config::ApiKeys, String> {
    Ok(config::load_api_keys())
}

// --- Agent Commands ---

#[tauri::command]
fn init_agent_conversation(
    context_path: String,
    selected_files: Vec<String>,
    user_prompt: String,
) -> Result<Vec<Message>, String> {
    let system_prompt = prompts::get_initial_system_prompt(&context_path, &selected_files)?;
    Ok(vec![
        Message {
            role: "system".to_string(),
            content: system_prompt,
            tool_calls: None,
        },
        Message {
            role: "user".to_string(),
            content: user_prompt,
            tool_calls: None,
        },
    ])
}

#[tauri::command]
async fn run_agent_step(model: String, history: Vec<Message>) -> Result<AgentStepResult, String> {
    agent::run_agent_step(model, history).await
}

#[tauri::command]
async fn execute_powershell(command: String, cwd: Option<String>) -> Result<String, String> {
    Ok(agent::run_powershell_command(&command, cwd.as_deref()))
}

#[tauri::command]
fn get_hotkey() -> Result<String, String> {
    Ok(config::get_hotkey())
}

#[tauri::command]
fn set_hotkey(hotkey: String) -> Result<(), String> {
    config::set_hotkey(hotkey)
}

#[tauri::command]
fn write_file_list(files: Vec<String>) -> Result<String, String> {
    use std::io::Write;
    let temp_path = std::env::temp_dir().join("shuttle_files.txt");
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
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_explorer_status,
            get_explorer_debug,
            get_ollama_models,
            get_ollama_url,
            set_ollama_url,
            login_antigravity,
            get_antigravity_status,
            set_openai_key,
            set_gemini_key,
            get_api_keys,
            get_hotkey,
            set_hotkey,
            init_agent_conversation,
            run_agent_step,
            execute_powershell,
            write_file_list,
            spotlight_submit,
        ])
        .setup(|app| {
            let main_window = app.get_webview_window("main").unwrap();
            let spotlight_window = app.get_webview_window("spotlight").unwrap();

            // Only spotlight loses shadow (main keeps DWM border)
            spotlight_window.set_shadow(false).ok();

            // --- System Tray ---
            let show_item = MenuItemBuilder::with_id("show", "Show Shuttle").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit Shuttle").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap())
                .tooltip("Shuttle")
                .menu(&tray_menu)
                .on_menu_event(
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
            let hotkey_str = config::get_hotkey();
            let hotkey_spotlight = spotlight_window.clone();
            let shortcut = hotkey_str
                .parse::<Shortcut>()
                .expect("Invalid hotkey in config");

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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
