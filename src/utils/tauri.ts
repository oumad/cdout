import { invoke, Channel } from "@tauri-apps/api/core";
import { CMD } from "../constants";
import type {
  ExplorerState,
  Message,
  AgentStepResult,
  ApiKeysResponse,
  StreamChunk,
  Skill,
  CliCredentialsStatus,
} from "../types";

// Explorer
export function getExplorerStatus(): Promise<ExplorerState> {
  return invoke<ExplorerState>(CMD.GET_EXPLORER_STATUS);
}

export function getExplorerDebug(): Promise<unknown> {
  return invoke(CMD.GET_EXPLORER_DEBUG);
}

// Models
export function getOllamaModels(): Promise<string[]> {
  return invoke<string[]>(CMD.GET_OLLAMA_MODELS);
}

export function getOllamaUrl(): Promise<string> {
  return invoke<string>(CMD.GET_OLLAMA_URL);
}

export function setOllamaUrl(url: string): Promise<void> {
  return invoke(CMD.SET_OLLAMA_URL, { url });
}

export function getSelectedModel(): Promise<string | null> {
  return invoke<string | null>(CMD.GET_SELECTED_MODEL);
}

export function setSelectedModel(model: string): Promise<void> {
  return invoke(CMD.SET_SELECTED_MODEL, { model });
}

// Auth
export function loginAntigravity(): Promise<string> {
  return invoke<string>(CMD.LOGIN_ANTIGRAVITY);
}

export function getAntigravityStatus(): Promise<string | null> {
  return invoke<string | null>(CMD.GET_ANTIGRAVITY_STATUS);
}

// API Keys
export function getApiKeys(): Promise<ApiKeysResponse> {
  return invoke<ApiKeysResponse>(CMD.GET_API_KEYS);
}

export function setOpenaiKey(key: string): Promise<void> {
  return invoke(CMD.SET_OPENAI_KEY, { key });
}

export function setGeminiKey(key: string): Promise<void> {
  return invoke(CMD.SET_GEMINI_KEY, { key });
}

// Agent
export function initAgentConversation(
  contextPath: string,
  selectedFiles: string[],
  userPrompt: string
): Promise<Message[]> {
  return invoke<Message[]>(CMD.INIT_AGENT_CONVERSATION, {
    contextPath,
    selectedFiles,
    userPrompt,
  });
}

export function runAgentStep(
  model: string,
  history: Message[]
): Promise<AgentStepResult> {
  return invoke<AgentStepResult>(CMD.RUN_AGENT_STEP, { model, history });
}

export function runAgentStepStream(
  model: string,
  history: Message[],
  onChunk: (chunk: StreamChunk) => void
): Promise<AgentStepResult> {
  const channel = new Channel<StreamChunk>();
  channel.onmessage = onChunk;
  return invoke<AgentStepResult>(CMD.RUN_AGENT_STEP_STREAM, {
    model,
    history,
    onChunk: channel,
  });
}

export function executePowershell(
  command: string,
  cwd: string | null
): Promise<string> {
  return invoke<string>(CMD.EXECUTE_POWERSHELL, { command, cwd });
}

// Utility
export function writeFileList(files: string[]): Promise<string> {
  return invoke<string>(CMD.WRITE_FILE_LIST, { files });
}

export function getHotkey(): Promise<string> {
  return invoke<string>(CMD.GET_HOTKEY);
}

export function setHotkey(hotkey: string): Promise<void> {
  return invoke(CMD.SET_HOTKEY, { hotkey });
}

// CLI Credentials
export function getCliCredentialsStatus(): Promise<CliCredentialsStatus> {
  return invoke<CliCredentialsStatus>(CMD.GET_CLI_CREDENTIALS_STATUS);
}

// Skills
export function listSkills(): Promise<Skill[]> {
  return invoke<Skill[]>(CMD.LIST_SKILLS);
}

export function spotlightSubmit(
  prompt: string,
  model: string
): Promise<void> {
  return invoke(CMD.SPOTLIGHT_SUBMIT, { prompt, model });
}
