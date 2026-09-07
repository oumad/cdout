import { invoke, Channel } from "@tauri-apps/api/core";
import { CMD } from "../constants";
import type {
  ExplorerState,
  Message,
  AgentStepResult,
  ApiKeysResponse,
  ProviderStatus,
  StreamChunk,
  ListSkillsResult,
  Session,
  SessionMeta,
  PlatformInfo,
} from "../types";

// Explorer
export function getExplorerStatus(): Promise<ExplorerState> {
  return invoke<ExplorerState>(CMD.GET_EXPLORER_STATUS);
}

export function getExplorerDebug(): Promise<unknown> {
  return invoke(CMD.GET_EXPLORER_DEBUG);
}

// Platform
/** `listFilePath` is substituted into `read_list_hint` when provided. */
export function getPlatformInfo(listFilePath?: string): Promise<PlatformInfo> {
  return invoke<PlatformInfo>(CMD.GET_PLATFORM_INFO, {
    listFilePath: listFilePath ?? null,
  });
}

// Models
export function getOllamaModels(): Promise<string[]> {
  return invoke<string[]>(CMD.GET_OLLAMA_MODELS);
}

export function getProviderStatus(): Promise<ProviderStatus> {
  return invoke<ProviderStatus>(CMD.GET_PROVIDER_STATUS);
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

// API Keys
export function getApiKeys(): Promise<ApiKeysResponse> {
  return invoke<ApiKeysResponse>(CMD.GET_API_KEYS);
}

export function setOpenrouterKey(key: string): Promise<void> {
  return invoke(CMD.SET_OPENROUTER_KEY, { key });
}

export function setAnthropicKey(key: string): Promise<void> {
  return invoke(CMD.SET_ANTHROPIC_KEY, { key });
}

// Free-tier OpenRouter models toggle
export function getShowFreeOpenrouterModels(): Promise<boolean> {
  return invoke<boolean>(CMD.GET_SHOW_FREE_OPENROUTER_MODELS);
}

export function setShowFreeOpenrouterModels(enabled: boolean): Promise<void> {
  return invoke(CMD.SET_SHOW_FREE_OPENROUTER_MODELS, { enabled });
}

// OpenRouter disclosure (one-time migration banner)
export function getOpenrouterDisclosureAck(): Promise<boolean> {
  return invoke<boolean>(CMD.GET_OPENROUTER_DISCLOSURE_ACK);
}

export function setOpenrouterDisclosureAck(ack: boolean): Promise<void> {
  return invoke(CMD.SET_OPENROUTER_DISCLOSURE_ACK, { ack });
}

export function hasLegacyCredentials(): Promise<boolean> {
  return invoke<boolean>(CMD.HAS_LEGACY_CREDENTIALS);
}

export function cleanupLegacyCredentials(
  removeThirdParty: boolean
): Promise<void> {
  return invoke(CMD.CLEANUP_LEGACY_CREDENTIALS, { removeThirdParty });
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

export function runAgentStepStream(
  model: string,
  history: Message[],
  onChunk: (chunk: StreamChunk) => void,
  dropTools = false
): Promise<AgentStepResult> {
  const channel = new Channel<StreamChunk>();
  channel.onmessage = onChunk;
  return invoke<AgentStepResult>(CMD.RUN_AGENT_STEP_STREAM, {
    model,
    history,
    dropTools,
    onChunk: channel,
  });
}

export function executeShellCommand(
  command: string,
  cwd: string | null
): Promise<string> {
  return invoke<string>(CMD.EXECUTE_SHELL_COMMAND, { command, cwd });
}

export interface RunningCommand {
  pid: number;
  command_preview: string;
}

export function cancelStream(): Promise<void> {
  return invoke(CMD.CANCEL_STREAM);
}

export function getRunningCommand(): Promise<RunningCommand | null> {
  return invoke<RunningCommand | null>(CMD.GET_RUNNING_COMMAND);
}

export function killRunningCommand(): Promise<void> {
  return invoke(CMD.KILL_RUNNING_COMMAND);
}

export function resetLoopDetector(): Promise<void> {
  return invoke(CMD.RESET_LOOP_DETECTOR);
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

// Sessions
export function listSessions(): Promise<SessionMeta[]> {
  return invoke<SessionMeta[]>(CMD.LIST_SESSIONS);
}

export function loadSession(id: string): Promise<Session> {
  return invoke<Session>(CMD.LOAD_SESSION, { id });
}

export function createSession(
  userPrompt: string,
  contextPath: string,
  selectedFiles: string[],
  model: string
): Promise<Session> {
  return invoke<Session>(CMD.CREATE_SESSION, {
    userPrompt,
    contextPath,
    selectedFiles,
    model,
  });
}

export function saveSessionMessages(
  id: string,
  messages: Message[]
): Promise<SessionMeta> {
  return invoke<SessionMeta>(CMD.SAVE_SESSION_MESSAGES, { id, messages });
}

export function deleteSession(id: string): Promise<void> {
  return invoke(CMD.DELETE_SESSION, { id });
}

export function renameSession(id: string, title: string): Promise<SessionMeta> {
  return invoke<SessionMeta>(CMD.RENAME_SESSION, { id, title });
}

// Skills
export function listSkills(): Promise<ListSkillsResult> {
  return invoke<ListSkillsResult>(CMD.LIST_SKILLS);
}

export function spotlightSubmit(
  prompt: string,
  model: string
): Promise<void> {
  return invoke(CMD.SPOTLIGHT_SUBMIT, { prompt, model });
}
