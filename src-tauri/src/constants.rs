pub const MAX_OUTPUT_LEN: usize = 2000;
pub const MAX_FILES_TO_LIST: usize = 20;
pub const TOOL_NAME: &str = "run_powershell";
pub const ASK_USER_QUESTION_TOOL: &str = "ask_user_question";

/// Keywords used to suppress fallback-command extraction from prose that's
/// clearly a completion summary ("Task complete", "successfully processed", …).
/// Without this guard, code like "I successfully ran `echo X`" would extract
/// `echo X` and re-execute it.
pub const COMPLETION_KEYWORDS: &[&str] = &["success", "completed", "task complete"];
