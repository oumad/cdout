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
  GET_OLLAMA_MODELS: "get_ollama_models",
  GET_OLLAMA_URL: "get_ollama_url",
  SET_OLLAMA_URL: "set_ollama_url",
  LOGIN_ANTIGRAVITY: "login_antigravity",
  GET_ANTIGRAVITY_STATUS: "get_antigravity_status",
  SET_OPENAI_KEY: "set_openai_key",
  SET_GEMINI_KEY: "set_gemini_key",
  GET_API_KEYS: "get_api_keys",
  GET_HOTKEY: "get_hotkey",
  SET_HOTKEY: "set_hotkey",
  INIT_AGENT_CONVERSATION: "init_agent_conversation",
  RUN_AGENT_STEP: "run_agent_step",
  RUN_AGENT_STEP_STREAM: "run_agent_step_stream",
  EXECUTE_POWERSHELL: "execute_powershell",
  WRITE_FILE_LIST: "write_file_list",
  GET_CLI_CREDENTIALS_STATUS: "get_cli_credentials_status",
  LIST_SKILLS: "list_skills",
  SPOTLIGHT_SUBMIT: "spotlight_submit",
} as const;

// Event names
export const EVENTS = {
  SPOTLIGHT_SUBMITTED: "spotlight-submitted",
} as const;
