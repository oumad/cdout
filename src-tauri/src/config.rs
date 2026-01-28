use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const CONFIG_FILE_NAME: &str = "shuttle_config.json";

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AppConfig {
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
}

fn default_ollama_url() -> String {
    DEFAULT_OLLAMA_URL.to_string()
}

fn get_config_path() -> PathBuf {
    // Use app data directory, fallback to current dir
    let base = dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("shuttle-io").join(CONFIG_FILE_NAME)
}

fn load_config() -> AppConfig {
    let path = get_config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str(&content) {
                return config;
            }
        }
    }
    AppConfig {
        ollama_url: DEFAULT_OLLAMA_URL.to_string(),
    }
}

fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&path, content).map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}

pub fn get_ollama_url() -> String {
    load_config().ollama_url
}

pub fn set_ollama_url(url: String) -> Result<(), String> {
    let mut config = load_config();
    config.ollama_url = url;
    save_config(&config)
}
