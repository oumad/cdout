mod explorer;
mod llm;
mod agent;

use agent::{AgentStepResult, AgentResponse};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_explorer_status() -> Result<explorer::ExplorerState, String> {
    explorer::get_active_explorer_info().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_ollama_models() -> Result<Vec<String>, String> {
    llm::list_models().await
}

// --- Agent Commands ---

#[tauri::command]
fn init_agent_conversation(context_path: String, selected_files: Vec<String>, user_prompt: String) -> Vec<llm::Message> {
    let system_prompt = agent::get_initial_system_prompt(&context_path, &selected_files);
    vec![
        llm::Message { role: "system".to_string(), content: system_prompt, tool_calls: None },
        llm::Message { role: "user".to_string(), content: user_prompt, tool_calls: None },
    ]
}

#[tauri::command]
async fn run_agent_step(model: String, history: Vec<llm::Message>) -> Result<AgentStepResult, String> {
    agent::run_agent_step(model, history).await
}

#[tauri::command]
async fn execute_powershell(command: String, cwd: Option<String>) -> Result<String, String> {
    // This is called when user APPROVES the proposal
    Ok(agent::run_powershell_command(&command, cwd.as_deref()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet, 
            get_explorer_status, 
            get_ollama_models,
            init_agent_conversation,
            run_agent_step,
            execute_powershell
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
