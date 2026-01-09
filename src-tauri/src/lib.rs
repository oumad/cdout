mod explorer;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_explorer_status() -> Result<explorer::ExplorerState, String> {
    explorer::get_active_explorer_info().map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, get_explorer_status])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
