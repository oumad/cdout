//! Fallback for platforms with no file-manager integration (Linux). The app
//! still works — the user just has to name paths in the prompt, and the
//! working directory stays whatever the session was created with.

use super::ExplorerState;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct ExplorerDebugInfo {
    pub os: String,
    pub supported: bool,
    pub reason: String,
}

pub fn get_active_explorer_info() -> Result<ExplorerState, String> {
    Ok(ExplorerState {
        path: String::new(),
        selected_files: Vec::new(),
    })
}

pub fn get_explorer_debug_info() -> Result<ExplorerDebugInfo, String> {
    Ok(ExplorerDebugInfo {
        os: std::env::consts::OS.to_string(),
        supported: false,
        reason: "File-manager integration is implemented for Windows (Explorer) \
                 and macOS (Finder) only."
            .to_string(),
    })
}
