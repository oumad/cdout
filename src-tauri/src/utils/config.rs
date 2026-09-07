use crate::constants::APP_DATA_DIR_NAME;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
/// Default global hotkey. Public so the hotkey-registration path in `lib.rs`
/// can fall back to it when a user-set value fails to parse.
/// Default spotlight hotkey. macOS gets Cmd rather than Ctrl: Ctrl+Alt+letter
/// is unidiomatic there and collides with the system's Ctrl-based text
/// navigation bindings.
#[cfg(target_os = "macos")]
pub const DEFAULT_HOTKEY: &str = "Cmd+Alt+A";
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+A";
const CONFIG_FILE_NAME: &str = "cdout_config.json";

// Pre-rename (shuttle-io era) locations, kept only for the migration below
// and the legacy-credential probes in `lib.rs`.
pub const LEGACY_APP_DATA_DIR_NAME: &str = "shuttle-io";
const LEGACY_CONFIG_FILE_NAME: &str = "shuttle_config.json";

/// How much the agent may run without a click.
///
/// The default is [`ApprovalMode::ReadOnly`] rather than `Ask` because the
/// classifier behind it is an allowlist: a command it does not recognise is
/// treated as mutating, so a gap costs an extra prompt rather than an
/// unwanted execution. Most agent turns are probes (`ffprobe`, `ls`,
/// `Test-Path`), and clicking Approve on those is pure friction.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// Approve every command by hand.
    Ask,
    /// Run provably read-only commands unattended; ask for anything else.
    #[default]
    ReadOnly,
    /// Run everything unattended. Recognisably destructive commands still
    /// stop for approval — that guard is not user-disableable.
    Auto,
}

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
    #[serde(default)]
    pub approval_mode: ApprovalMode,
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
    base.join(APP_DATA_DIR_NAME).join(CONFIG_FILE_NAME)
}

/// Migration for the shuttle-io → cdout rename: move the old data directory
/// (API keys, sessions, user skills, config) to its new home, then rename the
/// config file inside it. Called from `run()` before the Tauri builder.
///
/// Resilient by design: the fast path is one atomic `fs::rename`, but if that
/// fails (a locked file under the dir — antivirus scan, old app instance) or
/// startup code from an earlier failed attempt already created the new dir,
/// it falls back to a per-file merge that runs again on every launch until
/// the old directory is empty. Files are only ever MOVED, never deleted or
/// overwritten — a conflict at the destination leaves the original in place.
///
/// Runs in both `config_dir` and `data_local_dir` because `get_config_path`
/// falls back to the latter — wherever the old build could have written,
/// the migration looks.
pub fn migrate_legacy_data_dir() {
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(b) = dirs::config_dir() {
        bases.push(b);
    }
    if let Some(b) = dirs::data_local_dir() {
        if !bases.contains(&b) {
            bases.push(b);
        }
    }
    for base in bases {
        migrate_legacy_data_dir_in(&base);
    }
}

fn migrate_legacy_data_dir_in(base: &Path) {
    let old_dir = base.join(LEGACY_APP_DATA_DIR_NAME);
    let new_dir = base.join(APP_DATA_DIR_NAME);
    if old_dir.is_dir() {
        if !new_dir.exists() {
            if fs::rename(&old_dir, &new_dir).is_err() {
                merge_dir_no_clobber(&old_dir, &new_dir);
            }
        } else {
            merge_dir_no_clobber(&old_dir, &new_dir);
        }
    }
    // Separate step so a legacy-named config file that travelled with the
    // directory move (or survived an earlier partial migration) still gets
    // picked up.
    let old_cfg = new_dir.join(LEGACY_CONFIG_FILE_NAME);
    let new_cfg = new_dir.join(CONFIG_FILE_NAME);
    if old_cfg.is_file() && !new_cfg.exists() {
        if let Err(e) = fs::rename(&old_cfg, &new_cfg) {
            eprintln!(
                "[migration] Could not rename {} to {}: {}",
                old_cfg.display(),
                new_cfg.display(),
                e
            );
        }
    }
}

/// Move every file under `old` to the same relative path under `new`,
/// skipping (and keeping) anything that already exists at the destination.
/// Emptied old directories are removed afterwards; anything locked or
/// conflicting stays put and is retried on the next launch.
fn merge_dir_no_clobber(old: &Path, new: &Path) {
    let entries = match fs::read_dir(old) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("[migration] Could not read {}: {}", old.display(), e);
            return;
        }
    };
    for entry in entries.flatten() {
        let src = entry.path();
        let dst = new.join(entry.file_name());
        if src.is_dir() {
            if fs::create_dir_all(&dst).is_ok() {
                merge_dir_no_clobber(&src, &dst);
            }
        } else if !dst.exists() {
            if let Err(e) = fs::rename(&src, &dst) {
                eprintln!(
                    "[migration] Could not move {} to {}: {}",
                    src.display(),
                    dst.display(),
                    e
                );
            }
        }
    }
    // Only succeeds once the directory is empty — intentionally best-effort.
    fs::remove_dir(old).ok();
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
            approval_mode: ApprovalMode::default(),
        }
    }
}

pub fn get_approval_mode() -> ApprovalMode {
    load_config().approval_mode
}

pub fn set_approval_mode(mode: ApprovalMode) -> Result<(), String> {
    let mut config = load_config();
    config.approval_mode = mode;
    save_config(&config)
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
    path.push(APP_DATA_DIR_NAME);
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

    /// Unique sandbox per test — tests share one process (same pid), so the
    /// test name has to be part of the path to keep parallel runs isolated.
    fn migration_sandbox(test_name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "cdout_migration_{}_{}",
            test_name,
            std::process::id()
        ));
        fs::remove_dir_all(&base).ok();
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn migrates_legacy_dir_contents_and_config_file() {
        let base = migration_sandbox("full");
        let old = base.join(LEGACY_APP_DATA_DIR_NAME);
        fs::create_dir_all(old.join("sessions")).unwrap();
        fs::write(
            old.join(LEGACY_CONFIG_FILE_NAME),
            r#"{"hotkey":"Ctrl+Alt+A"}"#,
        )
        .unwrap();
        fs::write(old.join("api_keys.json"), r#"{"openrouter":"sk-or-x"}"#).unwrap();
        fs::write(old.join("sessions").join("123_ab.json"), "{}").unwrap();

        migrate_legacy_data_dir_in(&base);

        let new = base.join(APP_DATA_DIR_NAME);
        assert!(!old.exists(), "old dir should have been moved away");
        assert_eq!(
            fs::read_to_string(new.join(CONFIG_FILE_NAME)).unwrap(),
            r#"{"hotkey":"Ctrl+Alt+A"}"#
        );
        assert!(!new.join(LEGACY_CONFIG_FILE_NAME).exists());
        assert!(new.join("api_keys.json").is_file());
        assert!(new.join("sessions").join("123_ab.json").is_file());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn migration_merges_into_existing_new_dir_without_clobbering() {
        let base = migration_sandbox("both_exist");
        let old = base.join(LEGACY_APP_DATA_DIR_NAME);
        let new = base.join(APP_DATA_DIR_NAME);
        fs::create_dir_all(old.join("sessions")).unwrap();
        fs::write(old.join("api_keys.json"), "old").unwrap();
        fs::write(old.join("sessions").join("s1.json"), "{}").unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("api_keys.json"), "new").unwrap();

        migrate_legacy_data_dir_in(&base);

        // Conflicting file: destination wins, original stays put.
        assert_eq!(
            fs::read_to_string(new.join("api_keys.json")).unwrap(),
            "new"
        );
        assert_eq!(
            fs::read_to_string(old.join("api_keys.json")).unwrap(),
            "old"
        );
        // Non-conflicting file is moved over, including nested dirs.
        assert!(new.join("sessions").join("s1.json").is_file());
        assert!(!old.join("sessions").join("s1.json").exists());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn default_approval_mode_is_read_only() {
        // Deliberate: the classifier is an allowlist, so its failure mode is
        // an extra prompt rather than an unwanted execution. Flipping this to
        // Auto would make a fresh install run model-written commands
        // unattended on first launch.
        assert_eq!(
            AppConfig::default_seed().approval_mode,
            ApprovalMode::ReadOnly
        );
        assert_eq!(ApprovalMode::default(), ApprovalMode::ReadOnly);
    }

    #[test]
    fn approval_mode_survives_a_roundtrip_and_older_configs() {
        // A config written before this field existed must still load.
        let legacy = r#"{"ollama_url":"http://x","hotkey":"Ctrl+Alt+A"}"#;
        let cfg: AppConfig = serde_json::from_str(legacy).unwrap();
        assert_eq!(cfg.approval_mode, ApprovalMode::ReadOnly);

        let json = serde_json::to_string(&AppConfig {
            approval_mode: ApprovalMode::Auto,
            ..AppConfig::default_seed()
        })
        .unwrap();
        assert!(json.contains("\"approval_mode\":\"auto\""));
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.approval_mode, ApprovalMode::Auto);
    }

    #[test]
    fn default_hotkey_parses() {
        // `lib.rs` calls `.expect()` on this at startup, so a malformed
        // default panics the app on launch instead of degrading gracefully.
        // The macOS default differs from the Windows one, which is exactly
        // the kind of divergence that goes unnoticed until launch day.
        use tauri_plugin_global_shortcut::Shortcut;
        DEFAULT_HOTKEY
            .parse::<Shortcut>()
            .unwrap_or_else(|e| panic!("DEFAULT_HOTKEY '{DEFAULT_HOTKEY}' must parse: {e}"));
    }

    #[test]
    fn migration_heals_stranded_state_with_empty_new_dir() {
        // A failed first attempt leaves the old dir intact while startup code
        // has already created an empty new dir (list_sessions / get_api_keys
        // both create_dir_all). The next launch must still move everything.
        let base = migration_sandbox("stranded");
        let old = base.join(LEGACY_APP_DATA_DIR_NAME);
        let new = base.join(APP_DATA_DIR_NAME);
        fs::create_dir_all(old.join("skills")).unwrap();
        fs::write(old.join(LEGACY_CONFIG_FILE_NAME), "cfg").unwrap();
        fs::write(old.join("skills").join("mine.md"), "skill").unwrap();
        fs::create_dir_all(&new).unwrap();

        migrate_legacy_data_dir_in(&base);

        assert!(!old.exists(), "emptied old dir should be removed");
        assert_eq!(
            fs::read_to_string(new.join(CONFIG_FILE_NAME)).unwrap(),
            "cfg"
        );
        assert_eq!(
            fs::read_to_string(new.join("skills").join("mine.md")).unwrap(),
            "skill"
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn migration_is_noop_on_fresh_install() {
        let base = migration_sandbox("fresh");
        migrate_legacy_data_dir_in(&base);
        assert!(!base.join(APP_DATA_DIR_NAME).exists());
        assert!(!base.join(LEGACY_APP_DATA_DIR_NAME).exists());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn migration_never_overwrites_an_existing_new_config_file() {
        let base = migration_sandbox("cfg_conflict");
        let new = base.join(APP_DATA_DIR_NAME);
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join(LEGACY_CONFIG_FILE_NAME), "legacy").unwrap();
        fs::write(new.join(CONFIG_FILE_NAME), "current").unwrap();

        migrate_legacy_data_dir_in(&base);

        assert_eq!(
            fs::read_to_string(new.join(CONFIG_FILE_NAME)).unwrap(),
            "current"
        );
        assert_eq!(
            fs::read_to_string(new.join(LEGACY_CONFIG_FILE_NAME)).unwrap(),
            "legacy"
        );
        fs::remove_dir_all(&base).ok();
    }
}
