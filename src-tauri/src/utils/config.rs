use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
/// Default global hotkey. Public so the hotkey-registration path in `lib.rs`
/// can fall back to it when a user-set value fails to parse.
pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+A";
const CONFIG_FILE_NAME: &str = "shuttle_config.json";

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AppConfig {
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default)]
    pub selected_model: Option<String>,
    /// When true the model picker also shows OpenRouter's free-tier slugs.
    /// Off by default — free models suffer documented FP4 quantization swaps
    /// that break non-Latin output and have ~20 req/min caps.
    #[serde(default)]
    pub show_free_openrouter_models: bool,
    /// When true the user has acknowledged that cloud prompts route through
    /// openrouter.ai. Toggled by the one-shot migration banner.
    #[serde(default)]
    pub openrouter_disclosure_ack: bool,
}

fn default_ollama_url() -> String {
    DEFAULT_OLLAMA_URL.to_string()
}

fn default_hotkey() -> String {
    DEFAULT_HOTKEY.to_string()
}

fn get_config_path() -> PathBuf {
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
    AppConfig::default_seed()
}

impl AppConfig {
    fn default_seed() -> Self {
        AppConfig {
            ollama_url: DEFAULT_OLLAMA_URL.to_string(),
            hotkey: DEFAULT_HOTKEY.to_string(),
            selected_model: None,
            show_free_openrouter_models: false,
            openrouter_disclosure_ack: false,
        }
    }
}

fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(())
}

// --- Public accessors used by Tauri commands ---

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

pub fn get_show_free_openrouter_models() -> bool {
    load_config().show_free_openrouter_models
}

pub fn set_show_free_openrouter_models(enabled: bool) -> Result<(), String> {
    let mut config = load_config();
    config.show_free_openrouter_models = enabled;
    save_config(&config)
}

pub fn get_openrouter_disclosure_ack() -> bool {
    load_config().openrouter_disclosure_ack
}

pub fn set_openrouter_disclosure_ack(ack: bool) -> Result<(), String> {
    let mut config = load_config();
    config.openrouter_disclosure_ack = ack;
    save_config(&config)
}

// --- API keys ---
//
// OpenRouter is the primary cloud gateway. The Anthropic key is OPTIONAL — set
// only when the user wants the prompt-caching path (OpenRouter's OpenAI-compat
// wire drops cache_control). The frontend exposes both in Settings; the router
// dispatches based on the model prefix.

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ApiKeys {
    pub openrouter: Option<String>,
    pub anthropic: Option<String>,
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
        // serde drops unknown fields silently — old fields (openai, gemini)
        // simply don't deserialize into the new struct.
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        ApiKeys::default()
    }
}

pub fn set_openrouter_key(key: String) -> Result<(), String> {
    let mut keys = load_api_keys();
    keys.openrouter = Some(key);
    save_api_keys(&keys)
}

pub fn set_anthropic_key(key: String) -> Result<(), String> {
    let mut keys = load_api_keys();
    keys.anthropic = Some(key);
    save_api_keys(&keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_openai_gemini_keys_are_dropped_on_load() {
        // Old api_keys.json had {openai, gemini}. New shape ignores them.
        let raw = serde_json::json!({
            "openai": "sk-old",
            "gemini": "AI-old",
            "openrouter": "sk-or-new"
        });
        let parsed: ApiKeys = serde_json::from_value(raw).unwrap();
        assert_eq!(parsed.openrouter.as_deref(), Some("sk-or-new"));
        assert!(parsed.anthropic.is_none());
    }

    #[test]
    fn default_app_config_off() {
        let c = AppConfig::default_seed();
        assert!(!c.show_free_openrouter_models);
        assert!(!c.openrouter_disclosure_ack);
        assert_eq!(c.ollama_url, DEFAULT_OLLAMA_URL);
    }
}
