//! Reads the user's current file-manager context — which folder is open and
//! which files are selected — so prompts like "rename these" need no filenames.
//!
//! The two implementations share nothing but [`ExplorerState`]: Windows drives
//! Explorer through COM (`IShellWindows`/`IWebBrowserApp`), macOS asks Finder
//! over AppleScript. Their debug payloads are deliberately different shapes,
//! because "which HWND is topmost in the Z-order" has no Finder analogue and
//! vice versa; the debug command exists to explain *why* detection failed on
//! the platform you're standing on.

use serde::Serialize;

#[derive(Serialize)]
pub struct ExplorerState {
    pub path: String,
    pub selected_files: Vec<String>,
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use self::windows::ExplorerDebugInfo;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::ExplorerDebugInfo;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod unsupported;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub use unsupported::ExplorerDebugInfo;

/// The folder and selection currently showing in the file manager. An empty
/// path means "no file-manager window found" — callers treat that as "operate
/// on nothing", never as an error.
pub fn get_active_explorer_info() -> Result<ExplorerState, String> {
    #[cfg(target_os = "windows")]
    {
        self::windows::get_active_explorer_info().map_err(|e| e.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        macos::get_active_explorer_info()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        unsupported::get_active_explorer_info()
    }
}

pub fn get_explorer_debug_info() -> Result<ExplorerDebugInfo, String> {
    #[cfg(target_os = "windows")]
    {
        self::windows::get_explorer_debug_info().map_err(|e| e.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        macos::get_explorer_debug_info()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        unsupported::get_explorer_debug_info()
    }
}
