import type { Ref } from "vue";

import type {
  AgentMode,
  ChatMessage,
  ConversationMemory,
  ConversationMeta,
  PersistedMessage,
  ToolExecutionEntry,
} from "../../../lib/chat-types";
import {
  appendConversationMessage,
  createConversation,
  getConversationMemory,
  listConversations,
  loadConversationHistory,
  loadConversationToolLogs,
  upsertConversationMemory,
} from "../services/chat-api";
import { extractSessionMemory } from "../utils/session-memory";

import type { ConversationTurnRuntimeState } from "./chat-runtime-state";

const SCHEDULED_CONVERSATION_TITLE_PREFIX = "Scheduled [";

type ConversationActionsDeps = {
  activeConversationId: Ref<string>;
  agentMode: Ref<AgentMode>;
  codingMode: Ref<boolean>;
  conversationFiles: Ref<unknown[]>;
  conversationMemory: Ref<ConversationMemory | null>;
  conversations: Ref<ConversationMeta[]>;
  messages: Ref<ChatMessage[]>;
  pendingUploads: Ref<unknown[]>;
  planMode: Ref<boolean>;
  toolExecutionLogs: Ref<ToolExecutionEntry[]>;
  clearActiveRuntimeState: () => void;
  refreshConversationFiles: (conversationId: string) => Promise<void>;
  restoreRuntimeState: (conversationId: string) => boolean;
  stashRuntimeState: (conversationId: string) => void;
};

function toRenderableMessages(saved: PersistedMessage[]): ChatMessage[] {
  return (saved || [])
    .filter(
      (message) =>
        (message.role === "user" || message.role === "assistant") &&
        (!!message.content || !!message.reasoning || (message.attachments?.length ?? 0) > 0),
    )
    .map((message) => ({
      role: message.role as "user" | "assistant",
      content: message.content,
      reasoning: message.reasoning,
      attachments: message.attachments,
      tokenUsage: message.tokenUsage,
      cost: message.cost,
    }));
}

export function createConversationActions(deps: ConversationActionsDeps) {
  async function loadConversationMemory(conversationId: string) {
    try {
      const memory = await getConversationMemory(conversationId);
      deps.conversationMemory.value = memory;
    } catch (err) {
      console.error("Failed to load conversation memory:", err);
      deps.conversationMemory.value = null;
    }
  }

  async function persistConversationMemory(conversationId: string) {
    const { summary, keyFacts } = extractSessionMemory(deps.messages.value);
    if (!summary.trim()) {
      return;
    }

    try {
      await upsertConversationMemory(conversationId, summary, keyFacts);
      deps.conversationMemory.value = {
        summary,
        keyFacts,
        updatedAt: Date.now(),
      };
    } catch (err) {
      console.error("Failed to persist conversation memory:", err);
    }
  }

  async function refreshConversations() {
    try {
      const items = await listConversations();
      deps.conversations.value = (items || []).filter(
        (item) => !item.title.startsWith(SCHEDULED_CONVERSATION_TITLE_PREFIX),
      );
    } catch (err) {
      console.error("Failed to list conversations:", err);
    }
  }

  async function createNewConversation(seedTitle?: string): Promise<string | null> {
    try {
      const conversation = await createConversation(seedTitle);
      await refreshConversations();
      return conversation.id;
    } catch (err) {
      console.error("Failed to create conversation:", err);
      return null;
    }
  }

  async function loadConversation(id: string) {
    const targetConversationId = id.trim();
    if (!targetConversationId) {
      return;
    }

    const previousConversationId = deps.activeConversationId.value;
    if (previousConversationId && previousConversationId !== targetConversationId) {
      deps.stashRuntimeState(previousConversationId);
    }

    deps.activeConversationId.value = targetConversationId;
    deps.planMode.value = deps.agentMode.value === "plan";
    deps.codingMode.value = deps.agentMode.value === "coding";
    deps.pendingUploads.value = [];

    try {
      const saved = await loadConversationHistory(targetConversationId);
      const savedToolLogs = await loadConversationToolLogs(targetConversationId);
      deps.messages.value = toRenderableMessages(saved || []);

      const restored = deps.restoreRuntimeState(targetConversationId);
      if (!restored) {
        deps.toolExecutionLogs.value = savedToolLogs;
      } else if (deps.toolExecutionLogs.value.length === 0) {
        deps.toolExecutionLogs.value = savedToolLogs;
      }

      await loadConversationMemory(targetConversationId);
      await deps.refreshConversationFiles(targetConversationId);
    } catch (err) {
      console.error("Failed to load conversation messages:", err);
      deps.messages.value = [];
      deps.clearActiveRuntimeState();
      deps.conversationFiles.value = [];
    }
  }

  async function persistMessage(
    message: ChatMessage,
    conversationId = deps.activeConversationId.value,
  ) {
    if (!conversationId) {
      return;
    }

    try {
      await appendConversationMessage(conversationId, message);
      await refreshConversations();
    } catch (err) {
      console.error("Failed to persist message:", err);
    }
  }

  function buildAssistantCostForState(state: ConversationTurnRuntimeState) {
    return {
      inputTokens: state.currentInputTokens,
      outputTokens: state.currentOutputTokens,
      toolCalls: state.currentToolCalls,
      toolDurationMs: state.currentToolDurationMs,
    };
  }

  return {
    buildAssistantCostForState,
    createNewConversation,
    loadConversation,
    loadConversationMemory,
    persistConversationMemory,
    persistMessage,
    refreshConversations,
  };
}
