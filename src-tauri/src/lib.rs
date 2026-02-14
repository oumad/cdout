mod auth;
mod features;
mod llm;
mod utils;

use auth::antigravity;
use features::agent;
use features::explorer;
use llm::clients::ollama;
use llm::Message;
use utils::config;

// use agent::run_agent_step as run_agent_step_impl;
use features::agent::AgentStepResult;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
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
async fn get_ollama_models() -> Result<Vec<String>, String> {
    let url = config::get_ollama_url();
    let mut models = ollama::list_models(&url).await.unwrap_or_default();

    if antigravity::load_credentials().is_some() {
        models.push("antigravity (gemini-3-flash)".to_string());
    }

    // Add OpenAI/Gemini generic
    let api_keys = config::load_api_keys();
    if api_keys.openai.is_some() && !api_keys.openai.as_ref().unwrap().is_empty() {
        models.push("openai:gpt-4o".to_string());
    }
    if api_keys.gemini.is_some() && !api_keys.gemini.as_ref().unwrap().is_empty() {
        models.push("gemini:gemini-1.5-pro".to_string());
    }

    Ok(models)
}

#[tauri::command]
fn get_ollama_url() -> String {
    config::get_ollama_url()
}

#[tauri::command]
fn set_ollama_url(url: String) -> Result<(), String> {
    config::set_ollama_url(url)
}

#[tauri::command]
async fn login_antigravity() -> Result<String, String> {
    antigravity::perform_login()
        .await
        .map(|c| c.email.unwrap_or_else(|| "Logged in".to_string()))
}

#[tauri::command]
async fn get_antigravity_status() -> Option<String> {
    let creds = antigravity::load_credentials();
    creds.and_then(|c| c.email)
}

#[tauri::command]
async fn set_openai_key(key: String) -> Result<(), String> {
    config::set_openai_key(key)
}

#[tauri::command]
async fn set_gemini_key(key: String) -> Result<(), String> {
    config::set_gemini_key(key)
}

#[tauri::command]
async fn get_api_keys() -> config::ApiKeys {
    config::load_api_keys()
}

// --- Agent Commands ---

#[tauri::command]
fn init_agent_conversation(
    context_path: String,
    selected_files: Vec<String>,
    user_prompt: String,
) -> Vec<Message> {
    let system_prompt = agent::get_initial_system_prompt(&context_path, &selected_files);
    vec![
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
    ]
}

#[tauri::command]
async fn run_agent_step(model: String, history: Vec<Message>) -> Result<AgentStepResult, String> {
    let url = config::get_ollama_url();
    agent::run_agent_step(url, model, history).await
}

#[tauri::command]
async fn execute_powershell(command: String, cwd: Option<String>) -> Result<String, String> {
    // This is called when user APPROVES the proposal
    Ok(agent::run_powershell_command(&command, cwd.as_deref()))
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
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
            init_agent_conversation,
            run_agent_step,
            execute_powershell,
            write_file_list
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
