use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const DEFAULT_HOTKEY: &str = "Ctrl+Alt+A";
const CONFIG_FILE_NAME: &str = "shuttle_config.json";

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AppConfig {
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default)]
    pub selected_model: Option<String>,
}

fn default_ollama_url() -> String {
    DEFAULT_OLLAMA_URL.to_string()
}

fn default_hotkey() -> String {
    DEFAULT_HOTKEY.to_string()
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
        hotkey: DEFAULT_HOTKEY.to_string(),
        selected_model: None,
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

pub fn get_hotkey() -> String {
    load_config().hotkey
}

pub fn set_hotkey(hotkey: String) -> Result<(), String> {
    let mut config = load_config();
    config.hotkey = hotkey;
    save_config(&config)
}

pub fn get_selected_model() -> Option<String> {
    load_config().selected_model
}

pub fn set_selected_model(model: String) -> Result<(), String> {
    let mut config = load_config();
    config.selected_model = Some(model);
    save_config(&config)
}

// --- API Keys Management ---

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiKeys {
    pub openai: Option<String>,
    pub gemini: Option<String>,
}

fn get_api_keys_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("shuttle-io");
    std::fs::create_dir_all(&path).ok();
    path.push("api_keys.json");
    path
}

pub fn save_api_keys(keys: &ApiKeys) -> Result<(), String> {
    let path = get_api_keys_path();
    let json = serde_json::to_string_pretty(keys).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

pub fn load_api_keys() -> ApiKeys {
    let path = get_api_keys_path();
    if let Ok(data) = fs::read_to_string(path) {
        serde_json::from_str(&data).unwrap_or(ApiKeys {
            openai: None,
            gemini: None,
        })
    } else {
        ApiKeys {
            openai: None,
            gemini: None,
        }
    }
}

pub fn set_openai_key(key: String) -> Result<(), String> {
    let mut keys = load_api_keys();
    keys.openai = Some(key);
    save_api_keys(&keys)
}

pub fn set_gemini_key(key: String) -> Result<(), String> {
    let mut keys = load_api_keys();
    keys.gemini = Some(key);
    save_api_keys(&keys)
}
