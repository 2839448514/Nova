import type { Ref } from "vue";

import type {
  NeedsUserInputPayload,
  ToolExecutionEntry,
  TurnCost,
} from "../../../lib/chat-types";

export type ConversationTurnRuntimeState = {
  isGenerating: boolean;
  assistantResponse: string;
  assistantReasoning: string;
  assistantTokenUsage?: number;
  assistantTurnCost?: TurnCost;
  pendingQuestion: NeedsUserInputPayload | null;
  pendingPermissionRequestId: string | null;
  currentToolStartedAt: number | null;
  currentToolCalls: number;
  currentToolDurationMs: number;
  currentInputTokens: number;
  currentOutputTokens: number;
  toolExecutionLogs: ToolExecutionEntry[];
  toolInputById: Map<string, string>;
  toolNameById: Map<string, string>;
};

type ActiveTurnRuntimeBindings = {
  activeConversationId: Ref<string>;
  isGenerating: Ref<boolean>;
  assistantResponse: Ref<string>;
  assistantReasoning: Ref<string>;
  assistantTokenUsage: Ref<number | undefined>;
  assistantTurnCost: Ref<TurnCost | undefined>;
  pendingQuestion: Ref<NeedsUserInputPayload | null>;
  pendingPermissionRequestId: Ref<string | null>;
  currentToolStartedAt: Ref<number | null>;
  currentToolCalls: Ref<number>;
  currentToolDurationMs: Ref<number>;
  currentInputTokens: Ref<number>;
  currentOutputTokens: Ref<number>;
  toolExecutionLogs: Ref<ToolExecutionEntry[]>;
  toolInputById: Map<string, string>;
  toolNameById: Map<string, string>;
};

export function createConversationRuntimeRegistry(bindings: ActiveTurnRuntimeBindings) {
  const runtimeStateByConversation = new Map<string, ConversationTurnRuntimeState>();

  function createEmptyRuntimeState(): ConversationTurnRuntimeState {
    return {
      isGenerating: false,
      assistantResponse: "",
      assistantReasoning: "",
      assistantTokenUsage: undefined,
      assistantTurnCost: undefined,
      pendingQuestion: null,
      pendingPermissionRequestId: null,
      currentToolStartedAt: null,
      currentToolCalls: 0,
      currentToolDurationMs: 0,
      currentInputTokens: 0,
      currentOutputTokens: 0,
      toolExecutionLogs: [],
      toolInputById: new Map<string, string>(),
      toolNameById: new Map<string, string>(),
    };
  }

  function normalizeConversationId(conversationId?: string | null): string {
    const normalized = (conversationId ?? "").trim();
    return normalized || "__default__";
  }

  function cloneRuntimeState(
    state: ConversationTurnRuntimeState,
  ): ConversationTurnRuntimeState {
    return {
      ...state,
      toolExecutionLogs: state.toolExecutionLogs.map((entry) => ({ ...entry })),
      toolInputById: new Map(state.toolInputById),
      toolNameById: new Map(state.toolNameById),
    };
  }

  function snapshotActiveRuntimeState(): ConversationTurnRuntimeState {
    return {
      isGenerating: bindings.isGenerating.value,
      assistantResponse: bindings.assistantResponse.value,
      assistantReasoning: bindings.assistantReasoning.value,
      assistantTokenUsage: bindings.assistantTokenUsage.value,
      assistantTurnCost: bindings.assistantTurnCost.value,
      pendingQuestion: bindings.pendingQuestion.value,
      pendingPermissionRequestId: bindings.pendingPermissionRequestId.value,
      currentToolStartedAt: bindings.currentToolStartedAt.value,
      currentToolCalls: bindings.currentToolCalls.value,
      currentToolDurationMs: bindings.currentToolDurationMs.value,
      currentInputTokens: bindings.currentInputTokens.value,
      currentOutputTokens: bindings.currentOutputTokens.value,
      toolExecutionLogs: bindings.toolExecutionLogs.value.map((entry) => ({ ...entry })),
      toolInputById: new Map(bindings.toolInputById),
      toolNameById: new Map(bindings.toolNameById),
    };
  }

  function applyRuntimeStateToActive(state: ConversationTurnRuntimeState) {
    bindings.isGenerating.value = state.isGenerating;
    bindings.assistantResponse.value = state.assistantResponse;
    bindings.assistantReasoning.value = state.assistantReasoning;
    bindings.assistantTokenUsage.value = state.assistantTokenUsage;
    bindings.assistantTurnCost.value = state.assistantTurnCost;
    bindings.pendingQuestion.value = state.pendingQuestion;
    bindings.pendingPermissionRequestId.value = state.pendingPermissionRequestId;
    bindings.currentToolStartedAt.value = state.currentToolStartedAt;
    bindings.currentToolCalls.value = state.currentToolCalls;
    bindings.currentToolDurationMs.value = state.currentToolDurationMs;
    bindings.currentInputTokens.value = state.currentInputTokens;
    bindings.currentOutputTokens.value = state.currentOutputTokens;
    bindings.toolExecutionLogs.value = state.toolExecutionLogs.map((entry) => ({ ...entry }));

    bindings.toolInputById.clear();
    for (const [id, input] of state.toolInputById.entries()) {
      bindings.toolInputById.set(id, input);
    }

    bindings.toolNameById.clear();
    for (const [id, name] of state.toolNameById.entries()) {
      bindings.toolNameById.set(id, name);
    }
  }

  function clearActiveRuntimeState() {
    bindings.isGenerating.value = false;
    bindings.assistantResponse.value = "";
    bindings.assistantReasoning.value = "";
    bindings.assistantTokenUsage.value = undefined;
    bindings.assistantTurnCost.value = undefined;
    bindings.pendingQuestion.value = null;
    bindings.pendingPermissionRequestId.value = null;
    bindings.currentToolStartedAt.value = null;
    bindings.currentToolCalls.value = 0;
    bindings.currentToolDurationMs.value = 0;
    bindings.currentInputTokens.value = 0;
    bindings.currentOutputTokens.value = 0;
    bindings.toolExecutionLogs.value = [];
    bindings.toolInputById.clear();
    bindings.toolNameById.clear();
  }

  function cleanupRuntimeStateIfIdle(conversationId: string) {
    const key = normalizeConversationId(conversationId);
    const state = runtimeStateByConversation.get(key);
    if (!state) {
      return;
    }

    const hasRenderableResponse = state.assistantResponse.trim().length > 0;
    const hasReasoning = state.assistantReasoning.trim().length > 0;
    const hasPendingPrompt = !!state.pendingPermissionRequestId || !!state.pendingQuestion;
    const hasRunningTool = state.toolExecutionLogs.some((entry) => entry.status === "running");
    if (!state.isGenerating && !hasRenderableResponse && !hasReasoning && !hasPendingPrompt && !hasRunningTool) {
      runtimeStateByConversation.delete(key);
    }
  }

  function stashRuntimeState(conversationId: string) {
    const key = normalizeConversationId(conversationId);
    runtimeStateByConversation.set(key, snapshotActiveRuntimeState());
    cleanupRuntimeStateIfIdle(key);
  }

  function restoreRuntimeState(conversationId: string): boolean {
    const key = normalizeConversationId(conversationId);
    const state = runtimeStateByConversation.get(key);
    if (!state) {
      clearActiveRuntimeState();
      return false;
    }

    applyRuntimeStateToActive(cloneRuntimeState(state));
    cleanupRuntimeStateIfIdle(key);
    return true;
  }

  function ensureRuntimeState(conversationId: string): ConversationTurnRuntimeState {
    const key = normalizeConversationId(conversationId);
    const existing = runtimeStateByConversation.get(key);
    if (existing) {
      return existing;
    }

    const created = createEmptyRuntimeState();
    runtimeStateByConversation.set(key, created);
    return created;
  }

  function hasAnyGeneratingConversations(): boolean {
    if (bindings.isGenerating.value) {
      return true;
    }

    for (const state of runtimeStateByConversation.values()) {
      if (state.isGenerating) {
        return true;
      }
    }
    return false;
  }

  function isSpecificConversationGenerating(conversationId: string): boolean {
    if (conversationId === bindings.activeConversationId.value) {
      return bindings.isGenerating.value;
    }

    const state = runtimeStateByConversation.get(normalizeConversationId(conversationId));
    return state?.isGenerating ?? false;
  }

  function deleteRuntimeState(conversationId?: string | null) {
    if (!conversationId) {
      return;
    }
    runtimeStateByConversation.delete(normalizeConversationId(conversationId));
  }

  function clearRuntimeStates() {
    runtimeStateByConversation.clear();
  }

  return {
    clearActiveRuntimeState,
    cleanupRuntimeStateIfIdle,
    clearRuntimeStates,
    deleteRuntimeState,
    ensureRuntimeState,
    hasAnyGeneratingConversations,
    isSpecificConversationGenerating,
    normalizeConversationId,
    restoreRuntimeState,
    stashRuntimeState,
  };
}
