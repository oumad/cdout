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
  id?: string;
  function: ToolCallFunction;
}

export interface Message {
  role: MessageRole;
  content: string;
  tool_calls?: ToolCall[];
}

export interface ToolProposal {
  tool_name: string;
  command: string;
  tool_call_id?: string;
}

export interface AgentResponse {
  type: "Text" | "CommandProposal" | "ToolProposals";
  content: string | ToolProposal[];
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

// Skills
export interface SkillRequirements {
  bins: string[];
  any_bins: string[];
}

export interface SkillMetadata {
  name: string;
  description: string;
  requires: SkillRequirements | null;
}

export interface Skill {
  metadata: SkillMetadata;
  body: string;
  source_path: string;
  available: boolean;
  missing_bins: string[];
}

// CLI Credentials
export interface CliCredentialsStatus {
  claude_code: boolean;
  codex: boolean;
}

// Streaming
export type StreamChunk =
  | { kind: "TextDelta"; text: string }
  | { kind: "Done"; message: Message }
  | { kind: "Error"; error: string };
