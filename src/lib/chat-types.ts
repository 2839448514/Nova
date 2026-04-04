export interface TurnCost {
  inputTokens: number;
  outputTokens: number;
  toolCalls: number;
  toolDurationMs: number;
}

export type AgentMode = "agent" | "plan" | "auto";

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  tokenUsage?: number;
  cost?: TurnCost;
}

export interface PersistedMessage {
  role: string;
  content: string;
  tokenUsage?: number;
  cost?: TurnCost;
}

export interface ConversationMemory {
  summary: string;
  keyFacts: string[];
  updatedAt: number;
}

export interface ConversationMeta {
  id: string;
  title: string;
  updatedAt: number;
}

export interface ChatMessageEvent {
  type: string;
  text?: string;
  tool_use_id?: string;
  tool_use_name?: string;
  tool_use_input?: string;
  tool_result?: string;
  token_usage?: number;
  stop_reason?: string;
  turn_state?: string;
}

export interface AskUserOption {
  label: string;
  description: string;
  preview?: string;
}

export interface AskUserQuestionItem {
  question: string;
  header: string;
  options: AskUserOption[];
  multi_select?: boolean;
}

export interface NeedsUserInputPayload {
  type?: string;
  context?: string;
  allow_freeform?: boolean;
  questions?: AskUserQuestionItem[];
}

export interface AskUserAnswerSubmission {
  answers: Record<string, string | string[]>;
  freeform?: string;
}

export interface PlanModeChangePayload {
  type?: string;
  mode?: string;
  goal?: string;
  summary?: string;
  message?: string;
}
