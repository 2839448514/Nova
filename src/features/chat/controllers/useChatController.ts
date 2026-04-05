import { ref, onMounted, onUnmounted } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { emitToast } from "../../../lib/toast";
import {
  buildPendingQuestionReply,
  extractPermissionActionFromAnswers,
  parseNeedsUserInput,
  parsePlanModeChange,
  renderToolResult,
} from "../../../lib/chat-payloads";
import {
  estimateTokens,
  extractSessionMemory,
} from "../utils/session-memory";
import { summarizeToolInfo } from "../utils/tool-info";
import {
  appendConversationMessage,
  cancelChatMessage,
  createConversation,
  deleteConversation,
  getConversationMemory,
  loadConversationToolLogs,
  listConversationRagDocuments,
  listConversations,
  loadConversationHistory,
  sendChatMessage,
  submitPermissionDecision,
  type RagDocumentMeta,
  upsertConversationRagDocuments,
  upsertConversationToolLog,
  upsertConversationMemory,
} from "../services/chat-api";
import type {
  AgentMode,
  AskUserAnswerSubmission,
  ChatAttachment,
  ChatMessage,
  ChatMessageEvent,
  ConversationMemory,
  ConversationMeta,
  NeedsUserInputPayload,
  ToolExecutionEntry,
  TurnCost,
  UploadedRagFile,
} from "../../../lib/chat-types";

export type MainView = "chat" | "hooks" | "agent" | "schedule";

type BackendErrorEvent = {
  source?: string;
  message?: string;
  stage?: string | null;
};

type ScheduledTaskTriggerEvent = {
  id: string;
  conversationId?: string;
  cron: string;
  prompt: string;
  recurring: boolean;
  durable: boolean;
  createdAt?: string;
  triggeredAt?: string;
};

type ChatScreenHandle = {
  scrollToBottom: () => void;
};

type ConversationTurnRuntimeState = {
  isGenerating: boolean;
  assistantResponse: string;
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

export function useChatController() {
  const messages = ref<ChatMessage[]>([]);
  const isGenerating = ref(false);
  const assistantResponse = ref("");
  const assistantTokenUsage = ref<number | undefined>(undefined);
  const assistantTurnCost = ref<TurnCost | undefined>(undefined);
  const conversations = ref<ConversationMeta[]>([]);
  const activeConversationId = ref("");
  const conversationFiles = ref<RagDocumentMeta[]>([]);
  const pendingUploads = ref<UploadedRagFile[]>([]);
  const pendingQuestion = ref<NeedsUserInputPayload | null>(null);
  const pendingPermissionRequestId = ref<string | null>(null);
  const conversationMemory = ref<ConversationMemory | null>(null);
  const mainView = ref<MainView>("chat");
  const currentToolStartedAt = ref<number | null>(null);
  const currentToolCalls = ref(0);
  const currentToolDurationMs = ref(0);
  const currentInputTokens = ref(0);
  const currentOutputTokens = ref(0);
  const agentMode = ref<AgentMode>("agent");
  const planMode = ref(false);
  const isCreatingNewChat = ref(false);
  const isSidebarOpen = ref(true);
  const toolExecutionLogs = ref<ToolExecutionEntry[]>([]);
  const chatScreenRef = ref<ChatScreenHandle | null>(null);
  const toolInputById = new Map<string, string>();
  const toolNameById = new Map<string, string>();
  const runtimeStateByConversation = new Map<string, ConversationTurnRuntimeState>();

  let unlistenChatStream: UnlistenFn | null = null;
  let unlistenBackendError: UnlistenFn | null = null;
  let unlistenScheduledTaskTrigger: UnlistenFn | null = null;

  function createEmptyRuntimeState(): ConversationTurnRuntimeState {
    return {
      isGenerating: false,
      assistantResponse: "",
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

  function cloneRuntimeState(state: ConversationTurnRuntimeState): ConversationTurnRuntimeState {
    return {
      ...state,
      toolExecutionLogs: state.toolExecutionLogs.map((entry) => ({ ...entry })),
      toolInputById: new Map(state.toolInputById),
      toolNameById: new Map(state.toolNameById),
    };
  }

  function snapshotActiveRuntimeState(): ConversationTurnRuntimeState {
    return {
      isGenerating: isGenerating.value,
      assistantResponse: assistantResponse.value,
      assistantTokenUsage: assistantTokenUsage.value,
      assistantTurnCost: assistantTurnCost.value,
      pendingQuestion: pendingQuestion.value,
      pendingPermissionRequestId: pendingPermissionRequestId.value,
      currentToolStartedAt: currentToolStartedAt.value,
      currentToolCalls: currentToolCalls.value,
      currentToolDurationMs: currentToolDurationMs.value,
      currentInputTokens: currentInputTokens.value,
      currentOutputTokens: currentOutputTokens.value,
      toolExecutionLogs: toolExecutionLogs.value.map((entry) => ({ ...entry })),
      toolInputById: new Map(toolInputById),
      toolNameById: new Map(toolNameById),
    };
  }

  function applyRuntimeStateToActive(state: ConversationTurnRuntimeState) {
    isGenerating.value = state.isGenerating;
    assistantResponse.value = state.assistantResponse;
    assistantTokenUsage.value = state.assistantTokenUsage;
    assistantTurnCost.value = state.assistantTurnCost;
    pendingQuestion.value = state.pendingQuestion;
    pendingPermissionRequestId.value = state.pendingPermissionRequestId;
    currentToolStartedAt.value = state.currentToolStartedAt;
    currentToolCalls.value = state.currentToolCalls;
    currentToolDurationMs.value = state.currentToolDurationMs;
    currentInputTokens.value = state.currentInputTokens;
    currentOutputTokens.value = state.currentOutputTokens;
    toolExecutionLogs.value = state.toolExecutionLogs.map((entry) => ({ ...entry }));

    toolInputById.clear();
    for (const [id, input] of state.toolInputById.entries()) {
      toolInputById.set(id, input);
    }

    toolNameById.clear();
    for (const [id, name] of state.toolNameById.entries()) {
      toolNameById.set(id, name);
    }
  }

  function clearActiveRuntimeState() {
    isGenerating.value = false;
    assistantResponse.value = "";
    assistantTokenUsage.value = undefined;
    assistantTurnCost.value = undefined;
    pendingQuestion.value = null;
    pendingPermissionRequestId.value = null;
    currentToolStartedAt.value = null;
    currentToolCalls.value = 0;
    currentToolDurationMs.value = 0;
    currentInputTokens.value = 0;
    currentOutputTokens.value = 0;
    toolExecutionLogs.value = [];
    toolInputById.clear();
    toolNameById.clear();
  }

  function cleanupRuntimeStateIfIdle(conversationId: string) {
    const key = normalizeConversationId(conversationId);
    const state = runtimeStateByConversation.get(key);
    if (!state) {
      return;
    }

    const hasRenderableResponse = state.assistantResponse.trim().length > 0;
    const hasPendingPrompt = !!state.pendingPermissionRequestId || !!state.pendingQuestion;
    const hasRunningTool = state.toolExecutionLogs.some((entry) => entry.status === "running");
    if (!state.isGenerating && !hasRenderableResponse && !hasPendingPrompt && !hasRunningTool) {
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
    if (isGenerating.value) {
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
    if (conversationId === activeConversationId.value) {
      return isGenerating.value;
    }

    const state = runtimeStateByConversation.get(normalizeConversationId(conversationId));
    return state?.isGenerating ?? false;
  }

  function resetToolTrackingState() {
    currentToolStartedAt.value = null;
    toolInputById.clear();
    toolNameById.clear();
  }

  function resetPendingPromptState() {
    pendingPermissionRequestId.value = null;
    pendingQuestion.value = null;
  }

  function resetTurnRuntimeState() {
    resetToolTrackingState();
    resetPendingPromptState();
  }

  function findToolExecutionIndexById(toolId: string): number {
    return toolExecutionLogs.value.findIndex((entry) => entry.id === toolId);
  }

  function latestRunningToolExecutionIdByName(toolName: string): string | null {
    for (let i = toolExecutionLogs.value.length - 1; i >= 0; i -= 1) {
      const entry = toolExecutionLogs.value[i];
      if (entry.toolName === toolName && entry.status === "running") {
        return entry.id;
      }
    }
    return null;
  }

  function startToolExecutionTrace(toolId: string, toolName: string) {
    const idx = findToolExecutionIndexById(toolId);
    if (idx >= 0) {
      toolExecutionLogs.value[idx] = {
        ...toolExecutionLogs.value[idx],
        toolName,
        status: "running",
        startedAt: Date.now(),
        finishedAt: undefined,
      };
      return;
    }

    toolExecutionLogs.value.push({
      id: toolId,
      toolName,
      input: "",
      result: "",
      status: "running",
      startedAt: Date.now(),
      finishedAt: undefined,
    });
  }

  function appendToolExecutionInput(toolId: string, inputDelta: string) {
    const idx = findToolExecutionIndexById(toolId);
    if (idx < 0) {
      return;
    }

    const entry = toolExecutionLogs.value[idx];
    toolExecutionLogs.value[idx] = {
      ...entry,
      input: `${entry.input}${inputDelta}`,
    };
  }

  function completeToolExecutionTrace(
    toolId: string | null,
    toolName: string,
    result: string,
    status: ToolExecutionEntry["status"],
    inputFallback?: string,
  ) {
    const resolvedId = toolId || latestRunningToolExecutionIdByName(toolName);
    if (!resolvedId) {
      return;
    }

    const idx = findToolExecutionIndexById(resolvedId);
    if (idx < 0) {
      return;
    }

    const entry = toolExecutionLogs.value[idx];
    const normalizedFallback = (inputFallback ?? "").trim();
    const resolvedInput = entry.input.trim().length > 0 ? entry.input : normalizedFallback;
    const updatedEntry: ToolExecutionEntry = {
      ...entry,
      toolName,
      input: resolvedInput,
      result,
      status,
      finishedAt: Date.now(),
    };
    toolExecutionLogs.value[idx] = updatedEntry;
    persistToolExecutionLog(updatedEntry);
  }

  function markRunningToolExecutions(status: "completed" | "error" | "cancelled") {
    const now = Date.now();
    const finalizedEntries: ToolExecutionEntry[] = [];
    toolExecutionLogs.value = toolExecutionLogs.value.map((entry) => {
      if (entry.status !== "running") {
        return entry;
      }

      const updatedEntry: ToolExecutionEntry = {
        ...entry,
        status,
        finishedAt: now,
      };
      finalizedEntries.push(updatedEntry);
      return updatedEntry;
    });

    for (const entry of finalizedEntries) {
      persistToolExecutionLog(entry);
    }
  }

  function findToolExecutionIndexByIdInLogs(entries: ToolExecutionEntry[], toolId: string): number {
    return entries.findIndex((entry) => entry.id === toolId);
  }

  function latestRunningToolExecutionIdByNameInLogs(
    entries: ToolExecutionEntry[],
    toolName: string,
  ): string | null {
    for (let i = entries.length - 1; i >= 0; i -= 1) {
      const entry = entries[i];
      if (entry.toolName === toolName && entry.status === "running") {
        return entry.id;
      }
    }
    return null;
  }

  function startToolExecutionTraceInState(
    state: ConversationTurnRuntimeState,
    toolId: string,
    toolName: string,
  ) {
    const idx = findToolExecutionIndexByIdInLogs(state.toolExecutionLogs, toolId);
    if (idx >= 0) {
      state.toolExecutionLogs[idx] = {
        ...state.toolExecutionLogs[idx],
        toolName,
        status: "running",
        startedAt: Date.now(),
        finishedAt: undefined,
      };
      return;
    }

    state.toolExecutionLogs.push({
      id: toolId,
      toolName,
      input: "",
      result: "",
      status: "running",
      startedAt: Date.now(),
      finishedAt: undefined,
    });
  }

  function appendToolExecutionInputInState(
    state: ConversationTurnRuntimeState,
    toolId: string,
    inputDelta: string,
  ) {
    const idx = findToolExecutionIndexByIdInLogs(state.toolExecutionLogs, toolId);
    if (idx < 0) {
      return;
    }

    const entry = state.toolExecutionLogs[idx];
    state.toolExecutionLogs[idx] = {
      ...entry,
      input: `${entry.input}${inputDelta}`,
    };
  }

  function completeToolExecutionTraceInState(
    conversationId: string,
    state: ConversationTurnRuntimeState,
    toolId: string | null,
    toolName: string,
    result: string,
    status: ToolExecutionEntry["status"],
    inputFallback?: string,
  ) {
    const resolvedId =
      toolId || latestRunningToolExecutionIdByNameInLogs(state.toolExecutionLogs, toolName);
    if (!resolvedId) {
      return;
    }

    const idx = findToolExecutionIndexByIdInLogs(state.toolExecutionLogs, resolvedId);
    if (idx < 0) {
      return;
    }

    const entry = state.toolExecutionLogs[idx];
    const normalizedFallback = (inputFallback ?? "").trim();
    const resolvedInput = entry.input.trim().length > 0 ? entry.input : normalizedFallback;
    const updatedEntry: ToolExecutionEntry = {
      ...entry,
      toolName,
      input: resolvedInput,
      result,
      status,
      finishedAt: Date.now(),
    };

    state.toolExecutionLogs[idx] = updatedEntry;
    persistToolExecutionLog(updatedEntry, conversationId);
  }

  function markRunningToolExecutionsInState(
    conversationId: string,
    state: ConversationTurnRuntimeState,
    status: "completed" | "error" | "cancelled",
  ) {
    const now = Date.now();
    for (let i = 0; i < state.toolExecutionLogs.length; i += 1) {
      const entry = state.toolExecutionLogs[i];
      if (entry.status !== "running") {
        continue;
      }

      const updatedEntry: ToolExecutionEntry = {
        ...entry,
        status,
        finishedAt: now,
      };
      state.toolExecutionLogs[i] = updatedEntry;
      persistToolExecutionLog(updatedEntry, conversationId);
    }
  }

  function persistToolExecutionLog(entry: ToolExecutionEntry, conversationId = activeConversationId.value) {
    if (!conversationId || entry.status === "running") {
      return;
    }

    void upsertConversationToolLog(conversationId, entry).catch((err) => {
      console.error("Failed to persist tool execution log:", err);
    });
  }

  function finalizeOrStopTurn(tokenUsage?: number) {
    if (assistantResponse.value.trim().length > 0) {
      finalizeAssistantTurn(tokenUsage);
      return;
    }
    assistantResponse.value = "";
    assistantTokenUsage.value = undefined;
    assistantTurnCost.value = undefined;
    isGenerating.value = false;
  }

  function hasConversationContent(): boolean {
    return messages.value.some(
      (m) => m.content.trim().length > 0 || (m.attachments?.length ?? 0) > 0,
    );
  }

  function handleAgentModeChange(mode: AgentMode) {
    agentMode.value = mode;
    planMode.value = mode === "plan";
  }

  function estimateInputTokensForTurn(userText: string, attachmentNames: string[]): number {
    const historyText = messages.value
      .slice(-12)
      .map((m) => m.content)
      .join("\n");
    const memoryText = conversationMemory.value
      ? `Summary: ${conversationMemory.value.summary}\nFacts: ${conversationMemory.value.keyFacts.join("; ")}`
      : "";
    const attachmentText = attachmentNames.length
      ? `Attachments: ${attachmentNames.join(", ")}`
      : "";
    return estimateTokens(`${historyText}\n${memoryText}\n${attachmentText}\n${userText}`);
  }

  function formatMessageContentForModel(msg: ChatMessage): string {
    const content = msg.content.trim();
    if (content) {
      return content;
    }

    const names = msg.attachments?.map((item) => item.sourceName).filter(Boolean) ?? [];
    if (names.length > 0) {
      return `Attached files: ${names.join(", ")}`;
    }

    return "";
  }

  function toAttachmentMeta(files: UploadedRagFile[]): ChatAttachment[] {
    return files.map((file) => ({
      sourceName: file.sourceName,
      mimeType: file.mimeType,
      size: file.size,
    }));
  }

  async function refreshConversationFiles(conversationId: string) {
    if (!conversationId) {
      conversationFiles.value = [];
      return;
    }

    try {
      conversationFiles.value = await listConversationRagDocuments(conversationId);
    } catch (err) {
      console.error("Failed to load conversation files:", err);
      conversationFiles.value = [];
    }
  }

  async function refreshActiveConversationFiles() {
    await refreshConversationFiles(activeConversationId.value);
  }

  function buildAssistantCost(): TurnCost {
    return {
      inputTokens: currentInputTokens.value,
      outputTokens: currentOutputTokens.value,
      toolCalls: currentToolCalls.value,
      toolDurationMs: currentToolDurationMs.value,
    };
  }

  async function loadConversationMemory(conversationId: string) {
    try {
      const mem = await getConversationMemory(conversationId);
      conversationMemory.value = mem;
    } catch (err) {
      console.error("Failed to load conversation memory:", err);
      conversationMemory.value = null;
    }
  }

  async function persistConversationMemory(conversationId: string) {
    const { summary, keyFacts } = extractSessionMemory(messages.value);
    if (!summary.trim()) return;
    try {
      await upsertConversationMemory(conversationId, summary, keyFacts);
      conversationMemory.value = {
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
      conversations.value = items || [];
    } catch (err) {
      console.error("Failed to list conversations:", err);
    }
  }

  async function createNewConversation(seedTitle?: string): Promise<string | null> {
    try {
      const conv = await createConversation(seedTitle);
      await refreshConversations();
      return conv.id;
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

    const previousConversationId = activeConversationId.value;
    if (previousConversationId && previousConversationId !== targetConversationId) {
      stashRuntimeState(previousConversationId);
    }

    activeConversationId.value = targetConversationId;
    planMode.value = agentMode.value === "plan";
    pendingUploads.value = [];
    try {
      const saved = await loadConversationHistory(targetConversationId);
      const savedToolLogs = await loadConversationToolLogs(targetConversationId);
      messages.value = (saved || [])
        .filter(
          (m) =>
            (m.role === "user" || m.role === "assistant") &&
            (!!m.content || (m.attachments?.length ?? 0) > 0),
        )
        .map((m) => ({
          role: m.role as "user" | "assistant",
          content: m.content,
          attachments: m.attachments,
          tokenUsage: m.tokenUsage,
          cost: m.cost,
        }));

      const restored = restoreRuntimeState(targetConversationId);
      if (!restored) {
        toolExecutionLogs.value = savedToolLogs;
      } else if (toolExecutionLogs.value.length === 0) {
        toolExecutionLogs.value = savedToolLogs;
      }

      await loadConversationMemory(targetConversationId);
      await refreshConversationFiles(targetConversationId);
    } catch (err) {
      console.error("Failed to load conversation messages:", err);
      messages.value = [];
      clearActiveRuntimeState();
      conversationFiles.value = [];
    }
  }

  async function persistMessage(msg: ChatMessage, conversationId = activeConversationId.value) {
    if (!conversationId) return;
    try {
      await appendConversationMessage(conversationId, msg);
      await refreshConversations();
    } catch (err) {
      console.error("Failed to persist message:", err);
    }
  }

  function buildAssistantCostForState(state: ConversationTurnRuntimeState): TurnCost {
    return {
      inputTokens: state.currentInputTokens,
      outputTokens: state.currentOutputTokens,
      toolCalls: state.currentToolCalls,
      toolDurationMs: state.currentToolDurationMs,
    };
  }

  async function finalizeBackgroundTurn(conversationId: string, state: ConversationTurnRuntimeState, tokenUsage?: number) {
    const finalText = state.assistantResponse.trim();
    const fallbackTokenUsage = finalText ? estimateTokens(finalText) : 0;
    const resolvedTokenUsage =
      typeof tokenUsage === "number" && tokenUsage > 0
        ? tokenUsage
        : typeof state.assistantTokenUsage === "number" && state.assistantTokenUsage > 0
          ? state.assistantTokenUsage
          : fallbackTokenUsage;

    if (state.currentOutputTokens <= 0 && resolvedTokenUsage > 0) {
      state.currentOutputTokens = resolvedTokenUsage;
    }

    if (finalText) {
      const assistantMsg: ChatMessage = {
        role: "assistant",
        content: finalText,
        tokenUsage: resolvedTokenUsage > 0 ? resolvedTokenUsage : undefined,
        cost: buildAssistantCostForState(state),
      };
      await persistMessage(assistantMsg, conversationId);
    }

    state.assistantResponse = "";
    state.assistantTokenUsage = undefined;
    state.assistantTurnCost = undefined;
    state.isGenerating = false;
    state.currentToolStartedAt = null;
    state.currentToolCalls = 0;
    state.currentToolDurationMs = 0;
    state.currentInputTokens = 0;
    state.currentOutputTokens = 0;
    state.pendingQuestion = null;
    state.pendingPermissionRequestId = null;
    state.toolInputById.clear();
    state.toolNameById.clear();
    cleanupRuntimeStateIfIdle(conversationId);
  }

  function finalizeAssistantTurn(tokenUsage?: number) {
    const finalText = assistantResponse.value.trim();
    const fallbackTokenUsage = finalText ? estimateTokens(finalText) : 0;
    const resolvedTokenUsage =
      typeof tokenUsage === "number" && tokenUsage > 0
        ? tokenUsage
        : typeof assistantTokenUsage.value === "number" && assistantTokenUsage.value > 0
          ? assistantTokenUsage.value
          : fallbackTokenUsage;

    if (currentOutputTokens.value <= 0 && resolvedTokenUsage > 0) {
      currentOutputTokens.value = resolvedTokenUsage;
    }

    const cost = buildAssistantCost();
    assistantTurnCost.value = cost;
    const assistantMsg: ChatMessage = {
      role: "assistant",
      content: finalText || "（本轮没有返回可显示的文本内容）",
      tokenUsage: resolvedTokenUsage > 0 ? resolvedTokenUsage : undefined,
      cost,
    };
    messages.value.push(assistantMsg);
    void persistMessage(assistantMsg);
    void persistConversationMemory(activeConversationId.value);
    assistantResponse.value = "";
    assistantTokenUsage.value = undefined;
    isGenerating.value = false;
    if (activeConversationId.value) {
      runtimeStateByConversation.delete(normalizeConversationId(activeConversationId.value));
    }
    chatScreenRef.value?.scrollToBottom();
  }

  async function handleSendMessage(userText: string) {
    if (isGenerating.value) return;
    const text = userText.trim();
    const filesToSend = pendingUploads.value.slice();
    if (!text && filesToSend.length === 0) return;

    mainView.value = "chat";
    resetPendingPromptState();

    if (!activeConversationId.value) {
      const seedTitle = text || filesToSend[0]?.sourceName || "New chat";
      const id = await createNewConversation(seedTitle);
      if (!id) return;
      activeConversationId.value = id;
      messages.value = [];
    }

    const sendingConversationId = activeConversationId.value;

    let uploadedAttachments: ChatAttachment[] = [];
    if (filesToSend.length > 0) {
      try {
        const result = await upsertConversationRagDocuments(
          sendingConversationId,
          filesToSend.map((file) => ({
            sourceName: file.sourceName,
            sourceType: "file",
            mimeType: file.mimeType,
            content: file.content,
          })),
        );

        if (result.added + result.updated <= 0) {
          emitToast({
            variant: "error",
            source: "upload",
            message: "文件上传失败，本轮未发送。",
          });
          return;
        }

        uploadedAttachments = toAttachmentMeta(filesToSend);
        pendingUploads.value = [];
          await refreshConversationFiles(sendingConversationId);

        if (result.rejected.length > 0) {
          const detail = result.rejected
            .slice(0, 2)
            .map((item) => `${item.sourceName}(${item.reason})`)
            .join("；");
          emitToast({
            variant: "error",
            source: "upload",
            message: `部分文件上传失败：${detail}`,
          });
        }
      } catch (err) {
        emitToast({
          variant: "error",
          source: "upload",
          message: `文件上传失败，本轮未发送: ${String(err)}`,
        });
        return;
      }
    }

    if (activeConversationId.value !== sendingConversationId) {
      emitToast({
        variant: "info",
        source: "send",
        message: "会话已切换，本次发送已取消，请在当前会话重新发送。",
      });
      return;
    }

    const modelUserText =
      text ||
      (uploadedAttachments.length > 0
        ? `请结合我上传的文件回答：${uploadedAttachments.map((item) => item.sourceName).join("，")}`
        : text);

    const userMsg: ChatMessage = {
      role: "user",
      content: text,
      attachments: uploadedAttachments.length > 0 ? uploadedAttachments : undefined,
    };
    messages.value.push(userMsg);
    await persistMessage(userMsg, sendingConversationId);
    isGenerating.value = true;
    assistantResponse.value = "";
    assistantTokenUsage.value = undefined;
    assistantTurnCost.value = undefined;
    currentToolStartedAt.value = null;
    currentToolCalls.value = 0;
    currentToolDurationMs.value = 0;
    currentOutputTokens.value = 0;
    currentInputTokens.value = estimateInputTokensForTurn(
      modelUserText,
      uploadedAttachments.map((item) => item.sourceName),
    );
    resetToolTrackingState();

    const rustMessages = messages.value.map((msg) => ({
      role: msg.role,
      content: formatMessageContentForModel(msg),
    }));

    try {
      await sendChatMessage(
        sendingConversationId || null,
        rustMessages,
        planMode.value,
        agentMode.value,
      );
    } catch (err: any) {
      const isActiveFailedConversation = activeConversationId.value === sendingConversationId;
      if (isActiveFailedConversation && !isGenerating.value) {
        return;
      }

      console.error("Chat error:", err);
      const errorMsg: ChatMessage = { role: "assistant", content: `API Error: ${err}` };
      if (isActiveFailedConversation) {
        messages.value.push(errorMsg);
      }
      await persistMessage(errorMsg, sendingConversationId);

      if (isActiveFailedConversation) {
        assistantResponse.value = "";
        assistantTokenUsage.value = undefined;
        assistantTurnCost.value = undefined;
        isGenerating.value = false;
        resetTurnRuntimeState();
        runtimeStateByConversation.delete(normalizeConversationId(sendingConversationId));
      } else {
        const backgroundState = ensureRuntimeState(sendingConversationId);
        backgroundState.assistantResponse = "";
        backgroundState.assistantTokenUsage = undefined;
        backgroundState.assistantTurnCost = undefined;
        backgroundState.isGenerating = false;
        backgroundState.pendingQuestion = null;
        backgroundState.pendingPermissionRequestId = null;
        backgroundState.currentToolStartedAt = null;
        backgroundState.currentToolCalls = 0;
        backgroundState.currentToolDurationMs = 0;
        backgroundState.currentInputTokens = 0;
        backgroundState.currentOutputTokens = 0;
        backgroundState.toolInputById.clear();
        backgroundState.toolNameById.clear();
        cleanupRuntimeStateIfIdle(sendingConversationId);
      }
    }
  }

  async function handleUploadFiles(files: UploadedRagFile[]) {
    if (!files.length || isGenerating.value) {
      return;
    }

    mainView.value = "chat";

    pendingUploads.value = [...pendingUploads.value, ...files];
    emitToast({
      variant: "success",
      source: "upload",
      message: `已添加 ${files.length} 个文件到待发送列表。`,
    });
  }

  function handleRemovePendingUpload(index: number) {
    if (index < 0 || index >= pendingUploads.value.length) {
      return;
    }
    pendingUploads.value.splice(index, 1);
  }

  async function handleCancelGeneration() {
    if (!isGenerating.value) return;
    try {
      await cancelChatMessage(activeConversationId.value || null);
    } catch (err) {
      console.error("Failed to cancel generation:", err);
      emitToast({
        variant: "error",
        source: "cancel",
        message: `取消失败: ${String(err)}`,
      });
    }
  }

  async function handlePendingQuestionSubmit(payload: AskUserAnswerSubmission) {
    if (pendingPermissionRequestId.value) {
      const action = extractPermissionActionFromAnswers(payload);
      if (!action) {
        emitToast({
          variant: "error",
          source: "permission",
          message: "未识别到权限操作，请重新选择允许/拒绝选项。",
        });
        return;
      }

      try {
        await submitPermissionDecision(
          activeConversationId.value || null,
          pendingPermissionRequestId.value,
          action,
        );
        resetPendingPromptState();
      } catch (err) {
        emitToast({
          variant: "error",
          source: "permission",
          message: `提交权限决策失败: ${String(err)}`,
        });
      }
      return;
    }

    await handleSendMessage(buildPendingQuestionReply(payload, "submit"));
  }

  async function handlePendingQuestionSkip() {
    if (pendingPermissionRequestId.value) {
      try {
        await submitPermissionDecision(
          activeConversationId.value || null,
          pendingPermissionRequestId.value,
          "deny_session",
        );
        resetPendingPromptState();
      } catch (err) {
        emitToast({
          variant: "error",
          source: "permission",
          message: `提交权限拒绝失败: ${String(err)}`,
        });
      }
      return;
    }

    await handleSendMessage(buildPendingQuestionReply(null, "skip"));
  }

  async function handleNewChat() {
    if (isCreatingNewChat.value) return;

    mainView.value = "chat";
    resetPendingPromptState();

    // 当前已是空会话时，不重复创建新的空会话。
    if (activeConversationId.value && !hasConversationContent() && !assistantResponse.value.trim()) {
      return;
    }

    isCreatingNewChat.value = true;
    try {
      const id = await createNewConversation("New chat");
      if (!id) {
        return;
      }

      await loadConversation(id);
    } finally {
      isCreatingNewChat.value = false;
    }
  }

  async function handleSelectConversation(id: string) {
    if (!id || id === activeConversationId.value) return;
    mainView.value = "chat";
    await loadConversation(id);
  }

  async function handleDeleteConversation(id: string) {
    if (!id) return;
    if (isSpecificConversationGenerating(id)) {
      emitToast({
        variant: "info",
        source: "delete-conversation",
        message: "该会话正在回复中，请先停止后再删除。",
      });
      return;
    }

    runtimeStateByConversation.delete(normalizeConversationId(id));
    try {
      await deleteConversation(id);
      await refreshConversations();

      if (activeConversationId.value === id) {
        if (conversations.value.length > 0) {
          await loadConversation(conversations.value[0].id);
        } else {
          const newId = await createNewConversation("New chat");
          if (newId) {
            await loadConversation(newId);
          } else {
            activeConversationId.value = "";
            messages.value = [];
            pendingUploads.value = [];
            conversationFiles.value = [];
          }
        }
      }
    } catch (err) {
      console.error("Failed to delete conversation:", err);
    }
  }

  function handleChangeMainView(view: MainView) {
    mainView.value = view;
  }

  const handleHistoryCleared = async () => {
    if (hasAnyGeneratingConversations()) {
      emitToast({
        variant: "info",
        source: "history",
        message: "存在进行中的会话回复，请先停止后再清空历史。",
      });
      return;
    }

    runtimeStateByConversation.clear();
    resetTurnRuntimeState();
    assistantResponse.value = "";
    assistantTokenUsage.value = undefined;
    assistantTurnCost.value = undefined;
    pendingUploads.value = [];
    toolExecutionLogs.value = [];
    conversationFiles.value = [];
    conversationMemory.value = null;
    messages.value = [];

    await refreshConversations();
    if (conversations.value.length === 0) {
      const newId = await createNewConversation("New chat");
      if (newId) {
        await loadConversation(newId);
      } else {
        activeConversationId.value = "";
      }
      return;
    }

    await loadConversation(conversations.value[0].id);
  };

  async function handleBackgroundChatStreamEvent(
    conversationId: string,
    payload: ChatMessageEvent,
  ) {
    const state = ensureRuntimeState(conversationId);

    if (payload.type === "text" && payload.text) {
      state.isGenerating = true;
      state.assistantResponse += payload.text;
      return;
    }

    if (payload.type === "tool-use-start") {
      state.isGenerating = true;
      state.currentToolCalls += 1;
      state.currentToolStartedAt = Date.now();

      const toolName = (payload.tool_use_name ?? "unknown").trim() || "unknown";
      const rawToolId = (payload.tool_use_id ?? "").trim();
      const toolId = rawToolId || `tool-${Date.now()}-${state.currentToolCalls}`;
      state.toolNameById.set(toolId, toolName);
      if (!state.toolInputById.has(toolId)) {
        state.toolInputById.set(toolId, "");
      }
      startToolExecutionTraceInState(state, toolId, toolName);
      return;
    }

    if (payload.type === "tool-json-delta") {
      const toolId = (payload.tool_use_id ?? "").trim();
      if (toolId && payload.tool_use_input) {
        const prev = state.toolInputById.get(toolId) ?? "";
        state.toolInputById.set(toolId, prev + payload.tool_use_input);
        appendToolExecutionInputInState(state, toolId, payload.tool_use_input);
      }
      return;
    }

    if (payload.type === "permission-request") {
      const requestId = (payload.tool_use_id ?? "").trim();
      const promptPayload = (payload.text ?? "").trim();
      const parsed = parseNeedsUserInput(promptPayload);
      if (!requestId || !parsed) {
        emitToast({
          variant: "error",
          source: "permission-request",
          message: `会话 ${conversationId} 收到异常权限请求，已自动拒绝。`,
        });
        if (requestId) {
          void submitPermissionDecision(conversationId, requestId, "deny_session").catch((err) => {
            emitToast({
              variant: "error",
              source: "permission-request",
              message: `会话 ${conversationId} 自动拒绝权限请求失败: ${String(err)}`,
            });
          });
        }
        return;
      }

      state.pendingPermissionRequestId = requestId;
      state.pendingQuestion = parsed;
      state.isGenerating = false;
      emitToast({
        variant: "info",
        source: "permission-request",
        message: `会话 ${conversationId} 需要权限确认，请切回该会话处理。`,
      });
      return;
    }

    if (payload.type === "tool-result") {
      if (state.currentToolStartedAt) {
        state.currentToolDurationMs += Math.max(0, Date.now() - state.currentToolStartedAt);
        state.currentToolStartedAt = null;
      }

      const rawToolId = (payload.tool_use_id ?? "").trim();
      const fallbackToolName = (payload.tool_use_name ?? "").trim();
      const toolName =
        fallbackToolName ||
        (rawToolId ? state.toolNameById.get(rawToolId) : undefined) ||
        "unknown";
      const toolId =
        rawToolId ||
        latestRunningToolExecutionIdByNameInLogs(state.toolExecutionLogs, toolName) ||
        "";
      const streamedInput = toolId ? state.toolInputById.get(toolId) ?? "" : "";
      const fallbackInput = (payload.tool_use_input ?? "").trim();
      const rawInput = streamedInput.trim().length > 0 ? streamedInput : fallbackInput;
      const result = (payload.tool_result ?? "").trim();
      completeToolExecutionTraceInState(
        conversationId,
        state,
        toolId || null,
        toolName,
        result,
        "completed",
        rawInput,
      );

      if (toolId) {
        state.toolInputById.delete(toolId);
        state.toolNameById.delete(toolId);
      }

      if (result) {
        const needsUserInput = parseNeedsUserInput(result);
        if (needsUserInput) {
          state.pendingPermissionRequestId = null;
          state.pendingQuestion = needsUserInput;
          state.isGenerating = false;
          const rendered = renderToolResult(result);
          const preview =
            rendered.length > 1200 ? `${rendered.slice(0, 1200)}\n...(truncated)` : rendered;
          state.assistantResponse += `\n${preview}\n`;
          emitToast({
            variant: "info",
            source: "permission-request",
            message: `会话 ${conversationId} 需要继续输入，请切回该会话。`,
          });
        }
      }
      return;
    }

    if (payload.type === "token-usage") {
      state.assistantTokenUsage = payload.token_usage;
      state.currentOutputTokens = payload.token_usage ?? state.currentOutputTokens;
      return;
    }

    if (payload.type !== "stop") {
      return;
    }

    const stopReason = payload.stop_reason ?? "";
    const turnState = payload.turn_state ?? "";

    if (turnState === "cancelled" || stopReason === "cancelled") {
      markRunningToolExecutionsInState(conversationId, state, "cancelled");
      state.isGenerating = false;
      state.assistantResponse = "";
      state.assistantTokenUsage = undefined;
      state.assistantTurnCost = undefined;
      state.pendingPermissionRequestId = null;
      state.pendingQuestion = null;
      state.currentToolStartedAt = null;
      state.currentToolCalls = 0;
      state.currentToolDurationMs = 0;
      state.currentInputTokens = 0;
      state.currentOutputTokens = 0;
      state.toolInputById.clear();
      state.toolNameById.clear();
      cleanupRuntimeStateIfIdle(conversationId);
      return;
    }

    if (turnState === "error") {
      markRunningToolExecutionsInState(conversationId, state, "error");
      state.isGenerating = false;
      state.assistantResponse = "";
      state.assistantTokenUsage = undefined;
      state.assistantTurnCost = undefined;
      state.pendingPermissionRequestId = null;
      state.pendingQuestion = null;
      state.currentToolStartedAt = null;
      state.currentToolCalls = 0;
      state.currentToolDurationMs = 0;
      state.currentInputTokens = 0;
      state.currentOutputTokens = 0;
      state.toolInputById.clear();
      state.toolNameById.clear();
      cleanupRuntimeStateIfIdle(conversationId);

      const detail = (payload.text ?? "").trim();
      emitToast({
        variant: "error",
        source: "chat-stream",
        message: detail || `会话 ${conversationId} 回复失败: ${stopReason || "unknown"}`,
      });
      return;
    }

    const shouldFinalize =
      turnState === "completed" ||
      turnState === "awaiting_user_input" ||
      turnState === "needs_user_input" ||
      turnState === "stop_hook_prevented" ||
      stopReason === "stop_hook_prevented" ||
      stopReason === "needs_user_input";

    if (shouldFinalize) {
      markRunningToolExecutionsInState(conversationId, state, "completed");
      await finalizeBackgroundTurn(conversationId, state, payload.token_usage);
    }
  }

  onMounted(async () => {
    await refreshConversations();
    if (conversations.value.length === 0) {
      const id = await createNewConversation("New chat");
      if (id) {
        await loadConversation(id);
      }
    } else {
      await loadConversation(conversations.value[0].id);
    }

    try {
      unlistenChatStream = await listen<ChatMessageEvent>("chat-stream", (event) => {
        const payload = event.payload;
        const payloadConversationId = (payload.conversation_id ?? "").trim();
        const targetConversationId = payloadConversationId || activeConversationId.value;
        if (!targetConversationId) {
          return;
        }

        if (targetConversationId !== activeConversationId.value) {
          void handleBackgroundChatStreamEvent(targetConversationId, payload);
          return;
        }

        if (payload.type === "text" && payload.text) {
          assistantResponse.value += payload.text;
          chatScreenRef.value?.scrollToBottom();
        } else if (payload.type === "tool-use-start") {
          currentToolCalls.value += 1;
          currentToolStartedAt.value = Date.now();
          const toolName = (payload.tool_use_name ?? "unknown").trim() || "unknown";
          const rawToolId = (payload.tool_use_id ?? "").trim();
          const toolId = rawToolId || `tool-${Date.now()}-${currentToolCalls.value}`;

          toolNameById.set(toolId, toolName);
          if (!toolInputById.has(toolId)) {
            toolInputById.set(toolId, "");
          }

          startToolExecutionTrace(toolId, toolName);
          assistantResponse.value += `\n> Using tool: ${toolName}...\n`;
          chatScreenRef.value?.scrollToBottom();
        } else if (payload.type === "tool-json-delta") {
          const toolId = (payload.tool_use_id ?? "").trim();
          if (toolId && payload.tool_use_input) {
            const prev = toolInputById.get(toolId) ?? "";
            toolInputById.set(toolId, prev + payload.tool_use_input);
            appendToolExecutionInput(toolId, payload.tool_use_input);
          }
        } else if (payload.type === "permission-request") {
          const requestId = (payload.tool_use_id ?? "").trim();
          const promptPayload = (payload.text ?? "").trim();
          const parsed = parseNeedsUserInput(promptPayload);

          if (!requestId) {
            emitToast({
              variant: "error",
              source: "permission-request",
              message: "收到权限请求但缺少 request_id，已尝试取消当前回合。",
            });
            void cancelChatMessage(activeConversationId.value || null).catch((err) => {
              emitToast({
                variant: "error",
                source: "permission-request",
                message: `取消异常权限请求失败: ${String(err)}`,
              });
            });
            resetPendingPromptState();
            return;
          }

          if (!parsed) {
            emitToast({
              variant: "error",
              source: "permission-request",
              message: "收到权限请求但参数无效，已自动拒绝该请求。",
            });
            void submitPermissionDecision(
              activeConversationId.value || null,
              requestId,
              "deny_session",
            ).catch((err) => {
              emitToast({
                variant: "error",
                source: "permission-request",
                message: `自动拒绝权限请求失败: ${String(err)}`,
              });
            });
            resetPendingPromptState();
            return;
          }

          pendingPermissionRequestId.value = requestId;
          pendingQuestion.value = parsed;
          chatScreenRef.value?.scrollToBottom();
        } else if (payload.type === "tool-result") {
          if (currentToolStartedAt.value) {
            currentToolDurationMs.value += Math.max(0, Date.now() - currentToolStartedAt.value);
            currentToolStartedAt.value = null;
          }
          const rawToolId = (payload.tool_use_id ?? "").trim();
          const fallbackToolName = (payload.tool_use_name ?? "").trim();
          const toolName =
            fallbackToolName ||
            (rawToolId ? toolNameById.get(rawToolId) : undefined) ||
            "unknown";
          const toolId = rawToolId || latestRunningToolExecutionIdByName(toolName) || "";
          const streamedInput = toolId ? toolInputById.get(toolId) ?? "" : "";
          const fallbackInput = (payload.tool_use_input ?? "").trim();
          const rawInput = streamedInput.trim().length > 0 ? streamedInput : fallbackInput;
          const info = summarizeToolInfo(toolName, rawInput);
          if (info) {
            assistantResponse.value += `\n> Tool info: ${info}\n`;
          }
          assistantResponse.value += `\n> Tool done: ${toolName}\n`;
          const result = (payload.tool_result ?? "").trim();
          completeToolExecutionTrace(toolId || null, toolName, result, "completed", rawInput);

          if (toolId) {
            toolInputById.delete(toolId);
            toolNameById.delete(toolId);
          }

          if (result) {
            const planModeChange = parsePlanModeChange(result);
            if (planModeChange) {
              const isPlan = planModeChange.mode === "plan";
              planMode.value = isPlan;
              agentMode.value = isPlan ? "plan" : "agent";
            }
            const needsUserInput = parseNeedsUserInput(result);
            if (needsUserInput) {
              pendingPermissionRequestId.value = null;
              pendingQuestion.value = needsUserInput;
              isGenerating.value = false;
              const rendered = renderToolResult(result);
              const preview =
                rendered.length > 1200 ? `${rendered.slice(0, 1200)}\n...(truncated)` : rendered;
              assistantResponse.value += `\n${preview}\n`;
            }
          }
          chatScreenRef.value?.scrollToBottom();
        } else if (payload.type === "token-usage") {
          assistantTokenUsage.value = payload.token_usage;
          currentOutputTokens.value = payload.token_usage ?? currentOutputTokens.value;
        } else if (payload.type === "stop") {
          const stopReason = payload.stop_reason ?? "";
          const turnState = payload.turn_state ?? "";

          if (turnState === "cancelled" || stopReason === "cancelled") {
            markRunningToolExecutions("cancelled");
            finalizeOrStopTurn(payload.token_usage);
            resetTurnRuntimeState();
            if (activeConversationId.value) {
              runtimeStateByConversation.delete(normalizeConversationId(activeConversationId.value));
            }
            return;
          }

          if (turnState === "error") {
            markRunningToolExecutions("error");
            isGenerating.value = false;
            assistantResponse.value = "";
            assistantTokenUsage.value = undefined;
            assistantTurnCost.value = undefined;
            resetTurnRuntimeState();
            if (activeConversationId.value) {
              runtimeStateByConversation.delete(normalizeConversationId(activeConversationId.value));
            }
            const detail = (payload.text ?? "").trim();
            emitToast({
              variant: "error",
              source: "chat-stream",
              message: detail || `Provider error: ${stopReason || "unknown"}`,
            });
            return;
          }
          const shouldFinalize =
            turnState === "completed" ||
            turnState === "awaiting_user_input" ||
            turnState === "needs_user_input" ||
            turnState === "stop_hook_prevented" ||
            stopReason === "stop_hook_prevented" ||
            stopReason === "needs_user_input";

          if (shouldFinalize) {
            markRunningToolExecutions("completed");
            finalizeOrStopTurn(payload.token_usage);
            resetTurnRuntimeState();
            if (activeConversationId.value) {
              runtimeStateByConversation.delete(normalizeConversationId(activeConversationId.value));
            }
          }
        }
      });
    } catch (err) {
      console.error("Failed to setup listener:", err);
    }

    try {
      unlistenBackendError = await listen<BackendErrorEvent>("backend-error", (event) => {
        const payload = event.payload ?? {};
        const prefix = [payload.source, payload.stage].filter(Boolean).join(" / ");
        const message = payload.message || "后端工作流发生未知错误";
        emitToast({
          variant: "error",
          source: "backend-error",
          message: prefix ? `[${prefix}] ${message}` : message,
        });
      });
    } catch (err) {
      console.error("Failed to setup backend-error listener:", err);
    }

    try {
      unlistenScheduledTaskTrigger = await listen<ScheduledTaskTriggerEvent>(
        "scheduled-task-trigger",
        (event) => {
          const payload = event.payload;
          const promptPreview = (payload.prompt ?? "").trim();
          const previewText =
            promptPreview.length > 70
              ? `${promptPreview.slice(0, 70)}...`
              : promptPreview;

          emitToast({
            variant: "info",
            source: "schedule",
            message: `定时任务触发: ${payload.id} (${payload.cron})${payload.conversationId ? ` [${payload.conversationId}]` : ""}${previewText ? ` - ${previewText}` : ""}`,
          });
        },
      );
    } catch (err) {
      console.error("Failed to setup scheduled-task-trigger listener:", err);
    }

    window.addEventListener("history-cleared", handleHistoryCleared as EventListener);
  });

  onUnmounted(() => {
    if (unlistenChatStream) unlistenChatStream();
    if (unlistenBackendError) unlistenBackendError();
    if (unlistenScheduledTaskTrigger) unlistenScheduledTaskTrigger();
    window.removeEventListener("history-cleared", handleHistoryCleared as EventListener);
  });

  return {
    messages,
    isGenerating,
    assistantResponse,
    assistantTokenUsage,
    assistantTurnCost,
    toolExecutionLogs,
    conversations,
    activeConversationId,
    pendingQuestion,
    pendingUploads,
    conversationFiles,
    agentMode,
    planMode,
    mainView,
    isSidebarOpen,
    chatScreenRef,
    refreshActiveConversationFiles,
    handleSendMessage,
    handleUploadFiles,
    handleRemovePendingUpload,
    handleCancelGeneration,
    handlePendingQuestionSubmit,
    handlePendingQuestionSkip,
    handleAgentModeChange,
    handleNewChat,
    handleSelectConversation,
    handleDeleteConversation,
    handleChangeMainView,
  };
}
