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
  /**
   * Marks a cdout-injected continuation/loop/interrupt nudge that needs
   * `role: "user"` so the LLM API accepts it, but should render as a
   * system-style note in the UI — never as your own message.
   */
  synthetic?: boolean;
}

export interface QuestionOption {
  label: string;
  description?: string;
}

export interface QuestionData {
  question: string;
  header?: string;
  options: QuestionOption[];
  multi_select: boolean;
}

export interface ToolProposal {
  tool_name: string;
  /** PowerShell script for run_powershell, or question text for ask_user_question. */
  command: string;
  tool_call_id?: string;
  /** Present only when tool_name === "ask_user_question". */
  question_data?: QuestionData;
}

/** A pending ask_user_question waiting on the user's selection. */
export interface PendingQuestion {
  data: QuestionData;
  tool_call_id?: string;
}

export interface AgentResponse {
  type: "Text" | "CommandProposal" | "ToolProposals";
  content: string | ToolProposal[];
}

/**
 * Verdict from the backend loop detector. Frontend reacts on Block/Break:
 * pause auto-execute and surface the proposal for explicit user approval.
 */
export type LoopVerdict =
  | { kind: "Ok" }
  | { kind: "Warning"; reason: string }
  | { kind: "Block"; reason: string }
  | { kind: "Break"; reason: string };

export interface AgentStepResult {
  response: AgentResponse;
  updated_history: Message[];
  loop_verdict?: LoopVerdict;
}

/**
 * Masked API key status. The renderer never receives the full key plaintext
 * — only a boolean "is set" flag and a last-four-character preview suitable
 * for showing "…3fa1" in a placeholder. Plaintext stays server-side.
 */
export interface ApiKeysResponse {
  openrouter_set: boolean;
  openrouter_preview: string | null;
  anthropic_set: boolean;
  anthropic_preview: string | null;
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

export interface SafetyHit {
  rule: string;
  snippet: string;
}

export interface QuarantinedSkill {
  metadata: SkillMetadata;
  source_path: string;
  safety_hits: SafetyHit[];
}

export interface ListSkillsResult {
  skills: Skill[];
  quarantined: QuarantinedSkill[];
}


// Sessions
export interface SessionMeta {
  id: string;
  title: string;
  created_at: number;
  last_active_at: number;
  explorer_path: string;
  file_count: number;
  model: string;
  message_count: number;
}

export interface Session extends SessionMeta {
  messages: Message[];
}

// Streaming
export type StreamChunk =
  | { kind: "TextDelta"; text: string }
  | { kind: "Done"; message: Message }
  | { kind: "Error"; error: string };
