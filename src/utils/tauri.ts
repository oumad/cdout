import { invoke } from "@tauri-apps/api/core";
import { CMD } from "../constants";
import type {
  ExplorerState,
  Message,
  AgentStepResult,
  ApiKeysResponse,
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

export function spotlightSubmit(
  prompt: string,
  model: string
): Promise<void> {
  return invoke(CMD.SPOTLIGHT_SUBMIT, { prompt, model });
}
