mod agent;
mod config;
mod explorer;
mod llm;

use agent::AgentStepResult;

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
    llm::list_models(&url).await
}

#[tauri::command]
fn get_ollama_url() -> String {
    config::get_ollama_url()
}

#[tauri::command]
fn set_ollama_url(url: String) -> Result<(), String> {
    config::set_ollama_url(url)
}

// --- Agent Commands ---

#[tauri::command]
fn init_agent_conversation(
    context_path: String,
    selected_files: Vec<String>,
    user_prompt: String,
) -> Vec<llm::Message> {
    let system_prompt = agent::get_initial_system_prompt(&context_path, &selected_files);
    vec![
        llm::Message {
            role: "system".to_string(),
            content: system_prompt,
            tool_calls: None,
        },
        llm::Message {
            role: "user".to_string(),
            content: user_prompt,
            tool_calls: None,
        },
    ]
}

#[tauri::command]
async fn run_agent_step(
    model: String,
    history: Vec<llm::Message>,
) -> Result<AgentStepResult, String> {
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
            init_agent_conversation,
            run_agent_step,
            execute_powershell,
            write_file_list
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
