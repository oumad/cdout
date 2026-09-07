/// Name of the app's data directory under the user config root
/// (`%APPDATA%\cdout` on Windows, `~/Library/Application Support/cdout` on
/// macOS). If this ever changes again, add a matching move to
/// `config::migrate_legacy_data_dir`.
pub const APP_DATA_DIR_NAME: &str = "cdout";

pub const MAX_OUTPUT_LEN: usize = 2000;
pub const MAX_FILES_TO_LIST: usize = 20;
/// The shell tool's name on this platform (`run_powershell` on Windows,
/// `run_shell` on macOS). Re-exported from `platform` so callers that only
/// need the name don't reach into the whole [`crate::platform::SHELL`] spec.
pub const TOOL_NAME: &str = crate::platform::SHELL.tool_name;
pub const ASK_USER_QUESTION_TOOL: &str = "ask_user_question";

/// Keywords used to suppress fallback-command extraction from prose that's
/// clearly a completion summary ("Task complete", "successfully processed", …).
/// Without this guard, code like "I successfully ran `echo X`" would extract
/// `echo X` and re-execute it.
pub const COMPLETION_KEYWORDS: &[&str] = &["success", "completed", "task complete"];
