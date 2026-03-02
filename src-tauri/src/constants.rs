pub const MAX_NUDGE_ATTEMPTS: usize = 3;
pub const MAX_OUTPUT_LEN: usize = 2000;
pub const MAX_FILES_TO_LIST: usize = 20;
pub const SHORT_EXPLANATION_LIMIT: usize = 500;
pub const TOOL_NAME: &str = "run_powershell";

pub const COMPLETION_KEYWORDS: &[&str] = &["success", "completed", "task complete"];

pub const INTENT_PHRASES: &[&str] = &[
    "i'll",
    "i will",
    "let me",
    "i'm going to",
    "i am going to",
    "which reduces",
    "which will",
    "this will",
    "to remove",
    "to crop",
    "to process",
    "to convert",
    "here's",
    "here is",
    "the command",
];
