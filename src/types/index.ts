export interface ExplorerState {
  path: string;
  selected_files: string[];
}

export type MessageRole = "system" | "user" | "assistant" | "tool";

export interface ToolCallFunction {
  name: string;
  arguments: Record<string, unknown>;
}

export interface ToolCall {
  function: ToolCallFunction;
}

export interface Message {
  role: MessageRole;
  content: string;
  tool_calls?: ToolCall[];
}

export interface AgentResponse {
  type: "Text" | "CommandProposal";
  content: string;
}

export interface AgentStepResult {
  response: AgentResponse;
  updated_history: Message[];
}

export interface ApiKeysResponse {
  openai: string | null;
  gemini: string | null;
}

export interface SpotlightSubmitPayload {
  prompt: string;
  model: string;
}
