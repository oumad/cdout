//! The one place that decides where cdout keeps its data.
//!
//! Every on-disk artefact (config, API keys, sessions, user skills, logs)
//! resolves its directory through [`app_data_dir`], so a single environment
//! variable can redirect all of them. That override exists for isolated runs:
//! development builds and end-to-end tests point `CDOUT_DATA_DIR` at a temp
//! directory so they never read or clobber the installed app's state.

use std::path::PathBuf;

/// Environment variable that overrides the data directory. Must be an
/// absolute path; a relative or empty value is ignored.
pub const DATA_DIR_ENV: &str = "CDOUT_DATA_DIR";

/// The override directory, when `CDOUT_DATA_DIR` holds a usable absolute path.
pub fn data_dir_override() -> Option<PathBuf> {
    let raw = std::env::var_os(DATA_DIR_ENV)?;
    let path = PathBuf::from(raw);
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return None;
    }
    Some(path)
}

/// `%APPDATA%\cdout` on Windows, `~/Library/Application Support/cdout` on
/// macOS, unless `CDOUT_DATA_DIR` overrides it. Not created here; callers
/// create the subdirectory they need.
pub fn app_data_dir() -> PathBuf {
    if let Some(dir) = data_dir_override() {
        return dir;
    }
    dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(crate::constants::APP_DATA_DIR_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dir_ends_with_app_name() {
        if data_dir_override().is_some() {
            return; // the developer's environment overrides it; nothing to assert
        }
        assert!(app_data_dir().ends_with(crate::constants::APP_DATA_DIR_NAME));
    }
}
