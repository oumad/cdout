// Timing
export const BLUR_GRACE_MS = 300;
export const FOCUS_DELAY_MS = 50;
export const MODEL_POLL_MS = 5000;
export const AUTO_STEP_DELAY_MS = 500;
export const ERROR_AUTO_DISMISS_MS = 8000;

// Limits
export const MAX_AUTO_STEPS = 10;
export const FILE_LIST_THRESHOLD = 20;
export const TOOL_OUTPUT_COLLAPSE_CHARS = 200;
export const MESSAGE_COLLAPSE_CHARS = 500;

// Tool names. The shell tool is named per-platform by the backend
// (`run_powershell` on Windows, `run_shell` on macOS) because the name is part
// of the prompt the model reads; use `platform.shell_tool_name` from
// `getPlatformInfo()` when synthesizing a tool call. Both spellings are
// accepted by the backend registry.
export const TOOLS = {
  ASK_USER_QUESTION: "ask_user_question",
} as const;

// Completion detection keywords
export const COMPLETION_KEYWORDS = [
  "task complete",
  "all done",
  "successfully processed all",
  "completed successfully",
  "successfully",
  "has been",
  "have been",
  "done",
] as const;

export const COMPLETION_SHORT_KEYWORDS = ["complete", "finished"] as const;
export const COMPLETION_SHORT_MAX_LEN = 200;

// Tauri command names
export const CMD = {
  GET_EXPLORER_STATUS: "get_explorer_status",
  GET_EXPLORER_DEBUG: "get_explorer_debug",
  GET_PLATFORM_INFO: "get_platform_info",
  GET_OLLAMA_MODELS: "get_ollama_models",
  GET_PROVIDER_STATUS: "get_provider_status",
  GET_OLLAMA_URL: "get_ollama_url",
  SET_OLLAMA_URL: "set_ollama_url",
  GET_SELECTED_MODEL: "get_selected_model",
  SET_SELECTED_MODEL: "set_selected_model",
  SET_OPENROUTER_KEY: "set_openrouter_key",
  SET_ANTHROPIC_KEY: "set_anthropic_key",
  GET_API_KEYS: "get_api_keys",
  GET_SHOW_FREE_OPENROUTER_MODELS: "get_show_free_openrouter_models",
  SET_SHOW_FREE_OPENROUTER_MODELS: "set_show_free_openrouter_models",
  GET_OPENROUTER_DISCLOSURE_ACK: "get_openrouter_disclosure_ack",
  SET_OPENROUTER_DISCLOSURE_ACK: "set_openrouter_disclosure_ack",
  HAS_LEGACY_CREDENTIALS: "has_legacy_credentials",
  CLEANUP_LEGACY_CREDENTIALS: "cleanup_legacy_credentials",
  GET_HOTKEY: "get_hotkey",
  SET_HOTKEY: "set_hotkey",
  INIT_AGENT_CONVERSATION: "init_agent_conversation",
  LIST_SESSIONS: "list_sessions",
  LOAD_SESSION: "load_session",
  CREATE_SESSION: "create_session",
  SAVE_SESSION_MESSAGES: "save_session_messages",
  DELETE_SESSION: "delete_session",
  RENAME_SESSION: "rename_session",
  RUN_AGENT_STEP_STREAM: "run_agent_step_stream",
  EXECUTE_SHELL_COMMAND: "execute_shell_command",
  RESET_LOOP_DETECTOR: "reset_loop_detector",
  CANCEL_STREAM: "cancel_stream",
  GET_RUNNING_COMMAND: "get_running_command",
  KILL_RUNNING_COMMAND: "kill_running_command",
  WRITE_FILE_LIST: "write_file_list",
  LIST_SKILLS: "list_skills",
  SPOTLIGHT_SUBMIT: "spotlight_submit",
} as const;

// Event names
export const EVENTS = {
  SPOTLIGHT_SUBMITTED: "spotlight-submitted",
} as const;
