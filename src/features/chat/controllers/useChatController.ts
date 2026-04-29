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
  PendingUploadFile,
  ToolExecutionEntry,
  TurnCost,
} from "../../../lib/chat-types";
import {
  buildAssistantCost,
  buildAssistantCostForState,
  buildModelMessage,
  estimateInputTokensForTurn,
  isDocumentUploadFile,
  isImageUploadFile,
  shouldPreservePendingPromptOnStop,
  toAttachmentMeta,
} from "./chat-message-helpers";
import {
  type BackendErrorEvent,
  type ChatScreenHandle,
  type ConversationTurnRuntimeState,
  type MainView,
  SCHEDULED_CONVERSATION_TITLE_PREFIX,
  type ScheduledTaskTriggerEvent,
} from "./chat-controller-types";
import {
  bindActiveRuntimeState,
  cleanupRuntimeStateIfIdle,
  clearActiveRuntimeState,
  ensureRuntimeState,
  hasAnyGeneratingConversations,
  isSpecificConversationGenerating,
  normalizeConversationId,
  resetPendingPromptState,
  resetToolTrackingState,
  resetTurnRuntimeState,
  restoreRuntimeState,
  stashRuntimeState,
} from "./chat-runtime-state";
import {
  appendToolExecutionInputInState,
  completeToolExecutionTraceInState,
  latestRunningToolExecutionIdByName,
  markRunningToolExecutionsInState,
  startToolExecutionTraceInState,
} from "./chat-tool-execution";

export function useChatController() {
  const messages = ref<ChatMessage[]>([]);
  const isGenerating = ref(false);
  const assistantResponse = ref("");
  const assistantReasoning = ref("");
  const assistantTokenUsage = ref<number | undefined>(undefined);
  const assistantTurnCost = ref<TurnCost | undefined>(undefined);
  const conversations = ref<ConversationMeta[]>([]);
  const activeConversationId = ref("");
  const conversationFiles = ref<RagDocumentMeta[]>([]);
  const pendingUploads = ref<PendingUploadFile[]>([]);
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
  const activeRuntimeRefs = {
    isGenerating,
    assistantResponse,
    assistantReasoning,
    assistantTokenUsage,
    assistantTurnCost,
    pendingQuestion,
    pendingPermissionRequestId,
    currentToolStartedAt,
    currentToolCalls,
    currentToolDurationMs,
    currentInputTokens,
    currentOutputTokens,
    toolExecutionLogs,
    toolInputById,
    toolNameById,
  };
  const activeRuntimeState = bindActiveRuntimeState(activeRuntimeRefs);

  let unlistenChatStream: UnlistenFn | null = null;
  let unlistenBackendError: UnlistenFn | null = null;
  let unlistenScheduledTaskTrigger: UnlistenFn | null = null;

  function persistToolExecutionLog(entry: ToolExecutionEntry, conversationId = activeConversationId.value) {
    if (!conversationId || entry.status === "running") {
      return;
    }

    void upsertConversationToolLog(conversationId, entry).catch((err) => {
      console.error("Failed to persist tool execution log:", err);
    });
  }

  function finalizeOrStopTurn(tokenUsage?: number) {
    if (assistantResponse.value.trim().length > 0 || assistantReasoning.value.trim().length > 0) {
      finalizeAssistantTurn(tokenUsage);
      return;
    }
    assistantResponse.value = "";
    assistantReasoning.value = "";
    assistantTokenUsage.value = undefined;
    assistantTurnCost.value = undefined;
    isGenerating.value = false;
  }

  function hasConversationContent(): boolean {
    return messages.value.some(
      (m) => m.content.trim().length > 0 || (m.reasoning?.trim().length ?? 0) > 0 || (m.attachments?.length ?? 0) > 0,
    );
  }

  function handleAgentModeChange(mode: AgentMode) {
    agentMode.value = mode;
    planMode.value = mode === "plan";
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
      conversations.value = (items || []).filter(
        (item) => !item.title.startsWith(SCHEDULED_CONVERSATION_TITLE_PREFIX),
      );
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
      stashRuntimeState(runtimeStateByConversation, previousConversationId, activeRuntimeRefs);
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
            (!!m.content || !!m.reasoning || (m.attachments?.length ?? 0) > 0),
        )
        .map((m) => ({
          role: m.role as "user" | "assistant",
          content: m.content,
          reasoning: m.reasoning,
          attachments: m.attachments,
          tokenUsage: m.tokenUsage,
          cost: m.cost,
        }));

      const restored = restoreRuntimeState(
        runtimeStateByConversation,
        targetConversationId,
        activeRuntimeRefs,
      );
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
      clearActiveRuntimeState(activeRuntimeRefs);
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

  async function finalizeBackgroundTurn(
    conversationId: string,
    state: ConversationTurnRuntimeState,
    tokenUsage?: number,
    preservePendingPrompt = false,
  ) {
    const finalText = state.assistantResponse.trim();
    const finalReasoning = state.assistantReasoning.trim();
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

    if (finalText || finalReasoning) {
      const assistantMsg: ChatMessage = {
        role: "assistant",
        content: finalText || "（本轮没有返回可显示的文本内容）",
        reasoning: finalReasoning || undefined,
        tokenUsage: resolvedTokenUsage > 0 ? resolvedTokenUsage : undefined,
        cost: buildAssistantCostForState(state),
      };
      await persistMessage(assistantMsg, conversationId);
    }

    state.assistantResponse = "";
    state.assistantReasoning = "";
    state.assistantTokenUsage = undefined;
    state.assistantTurnCost = undefined;
    state.isGenerating = false;
    state.currentToolStartedAt = null;
    state.currentToolCalls = 0;
    state.currentToolDurationMs = 0;
    state.currentInputTokens = 0;
    state.currentOutputTokens = 0;
    if (!preservePendingPrompt) {
      state.pendingQuestion = null;
      state.pendingPermissionRequestId = null;
    }
    state.toolInputById.clear();
    state.toolNameById.clear();

    if (!preservePendingPrompt) {
      cleanupRuntimeStateIfIdle(runtimeStateByConversation, conversationId);
    }
  }

  function finalizeAssistantTurn(tokenUsage?: number) {
    const finalText = assistantResponse.value.trim();
    const finalReasoning = assistantReasoning.value.trim();
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

    const cost = buildAssistantCost(
      currentInputTokens.value,
      currentOutputTokens.value,
      currentToolCalls.value,
      currentToolDurationMs.value,
    );
    assistantTurnCost.value = cost;
    const assistantMsg: ChatMessage = {
      role: "assistant",
      content: finalText || "（本轮没有返回可显示的文本内容）",
      reasoning: finalReasoning || undefined,
      tokenUsage: resolvedTokenUsage > 0 ? resolvedTokenUsage : undefined,
      cost,
    };
    messages.value.push(assistantMsg);
    void persistMessage(assistantMsg);
    void persistConversationMemory(activeConversationId.value);
    assistantResponse.value = "";
    assistantReasoning.value = "";
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
    const textFiles = filesToSend.filter(isDocumentUploadFile);
    const imageFiles = filesToSend.filter(isImageUploadFile);
    if (!text && filesToSend.length === 0) return;

    mainView.value = "chat";
    resetPendingPromptState(activeRuntimeRefs);

    if (!activeConversationId.value) {
      const seedTitle = text || filesToSend[0]?.sourceName || "New chat";
      const id = await createNewConversation(seedTitle);
      if (!id) return;
      activeConversationId.value = id;
      messages.value = [];
    }

    const sendingConversationId = activeConversationId.value;

    let uploadedAttachments: ChatAttachment[] = toAttachmentMeta(imageFiles);
    if (textFiles.length > 0) {
      try {
        const result = await upsertConversationRagDocuments(
          sendingConversationId,
          textFiles.map((file) => ({
            sourceName: file.sourceName,
            sourceType: "file",
            mimeType: file.mimeType,
            content: file.content,
          })),
        );

        if (result.added + result.updated <= 0 && imageFiles.length === 0) {
          emitToast({
            variant: "error",
            source: "upload",
            message: "文件上传失败，本轮未发送。",
          });
          return;
        }

        const rejectedNames = new Set(result.rejected.map((item) => item.sourceName));
        const acceptedTextFiles = textFiles.filter((file) => !rejectedNames.has(file.sourceName));
        uploadedAttachments = [
          ...toAttachmentMeta(acceptedTextFiles),
          ...toAttachmentMeta(imageFiles),
        ];
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

    if (filesToSend.length > 0) {
      pendingUploads.value = [];
    }

    if (activeConversationId.value !== sendingConversationId) {
      emitToast({
        variant: "info",
        source: "send",
        message: "会话已切换，本次发送已取消，请在当前会话重新发送。",
      });
      return;
    }

    const uploadedAttachmentNames = uploadedAttachments.map((item) => item.sourceName);
    const uploadedDocumentNames = uploadedAttachments
      .filter((item) => item.kind !== "image")
      .map((item) => item.sourceName);
    const uploadedImageCount = uploadedAttachments.filter((item) => item.kind === "image").length;
    const modelUserText =
      text ||
      (uploadedImageCount > 0
        ? "请结合我上传的图片回答。"
        : uploadedDocumentNames.length > 0
          ? `请结合我上传的文件回答：${uploadedDocumentNames.join("，")}`
        : text);

    const userMsg: ChatMessage = {
      role: "user",
      content: text,
      attachments: uploadedAttachments.length > 0 ? uploadedAttachments : undefined,
    };
    messages.value.push(userMsg);
    await persistMessage(userMsg, sendingConversationId);
    chatScreenRef.value?.scrollLastUserMessageToTop();
    isGenerating.value = true;
    assistantResponse.value = "";
    assistantReasoning.value = "";
    assistantTokenUsage.value = undefined;
    assistantTurnCost.value = undefined;
    currentToolStartedAt.value = null;
    currentToolCalls.value = 0;
    currentToolDurationMs.value = 0;
    currentOutputTokens.value = 0;
    currentInputTokens.value = estimateInputTokensForTurn(
      messages.value,
      conversationMemory.value,
      modelUserText,
      uploadedAttachmentNames,
    );
    resetToolTrackingState(activeRuntimeRefs);

    const rustMessages = messages.value.map((msg) => buildModelMessage(msg));

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
        assistantReasoning.value = "";
        assistantTokenUsage.value = undefined;
        assistantTurnCost.value = undefined;
        isGenerating.value = false;
        resetTurnRuntimeState(activeRuntimeRefs);
        runtimeStateByConversation.delete(normalizeConversationId(sendingConversationId));
      } else {
        const backgroundState = ensureRuntimeState(
          runtimeStateByConversation,
          sendingConversationId,
        );
        resetBackgroundRuntimeState(sendingConversationId, backgroundState);
      }
    }
  }

  async function handleUploadFiles(files: PendingUploadFile[]) {
    if (!files.length || isGenerating.value) {
      return;
    }

    mainView.value = "chat";

    pendingUploads.value = [...pendingUploads.value, ...files];
    emitToast({
      variant: "success",
      source: "upload",
      message: `已添加 ${files.length} 个附件到待发送列表。`,
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
        resetPendingPromptState(activeRuntimeRefs);
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
        resetPendingPromptState(activeRuntimeRefs);
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
    resetPendingPromptState(activeRuntimeRefs);

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
    if (
      isSpecificConversationGenerating(
        activeConversationId,
        isGenerating,
        runtimeStateByConversation,
        id,
      )
    ) {
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
    if (hasAnyGeneratingConversations(isGenerating, runtimeStateByConversation)) {
      emitToast({
        variant: "info",
        source: "history",
        message: "存在进行中的会话回复，请先停止后再清空历史。",
      });
      return;
    }

    runtimeStateByConversation.clear();
    resetTurnRuntimeState(activeRuntimeRefs);
    assistantResponse.value = "";
    assistantReasoning.value = "";
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

  function resetBackgroundRuntimeState(
    conversationId: string,
    state: ConversationTurnRuntimeState,
    preservePendingPrompt = false,
  ) {
    state.isGenerating = false;
    state.assistantResponse = "";
    state.assistantReasoning = "";
    state.assistantTokenUsage = undefined;
    state.assistantTurnCost = undefined;
    if (!preservePendingPrompt) {
      state.pendingPermissionRequestId = null;
      state.pendingQuestion = null;
    }
    state.currentToolStartedAt = null;
    state.currentToolCalls = 0;
    state.currentToolDurationMs = 0;
    state.currentInputTokens = 0;
    state.currentOutputTokens = 0;
    state.toolInputById.clear();
    state.toolNameById.clear();

    if (!preservePendingPrompt) {
      cleanupRuntimeStateIfIdle(runtimeStateByConversation, conversationId);
    }
  }

  async function handleChatStreamEvent(
    conversationId: string,
    payload: ChatMessageEvent,
    mode: "active" | "background",
  ) {
    const isActive = mode === "active";
    const state = isActive
      ? activeRuntimeState
      : ensureRuntimeState(runtimeStateByConversation, conversationId);

    if (payload.type === "text" && payload.text) {
      state.isGenerating = true;
      state.assistantResponse += payload.text;
      if (isActive) {
        chatScreenRef.value?.scrollToBottom();
      }
      return;
    }

    if (payload.type === "reasoning" && payload.text) {
      state.isGenerating = true;
      state.assistantReasoning += payload.text;
      if (isActive) {
        chatScreenRef.value?.scrollToBottom();
      }
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

      if (isActive) {
        state.assistantResponse += `\n> Using tool: ${toolName}...\n`;
        chatScreenRef.value?.scrollToBottom();
      }
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

      if (!requestId) {
        emitToast({
          variant: "error",
          source: "permission-request",
          message: isActive
            ? "收到权限请求但缺少 request_id，已尝试取消当前回合。"
            : `会话 ${conversationId} 收到异常权限请求，无法继续处理。`,
        });

        if (isActive) {
          void cancelChatMessage(activeConversationId.value || null).catch((err) => {
            emitToast({
              variant: "error",
              source: "permission-request",
              message: `取消异常权限请求失败: ${String(err)}`,
            });
          });
          resetPendingPromptState(activeRuntimeRefs);
        }
        return;
      }

      if (!parsed) {
        emitToast({
          variant: "error",
          source: "permission-request",
          message: isActive
            ? "收到权限请求但参数无效，已自动拒绝该请求。"
            : `会话 ${conversationId} 收到异常权限请求，已自动拒绝。`,
        });
        void submitPermissionDecision(
          isActive ? activeConversationId.value || null : conversationId,
          requestId,
          "deny_session",
        ).catch((err) => {
          emitToast({
            variant: "error",
            source: "permission-request",
            message: isActive
              ? `自动拒绝权限请求失败: ${String(err)}`
              : `会话 ${conversationId} 自动拒绝权限请求失败: ${String(err)}`,
          });
        });
        if (isActive) {
          resetPendingPromptState(activeRuntimeRefs);
        }
        return;
      }

      state.pendingPermissionRequestId = requestId;
      state.pendingQuestion = parsed;
      if (!isActive) {
        state.isGenerating = false;
        emitToast({
          variant: "info",
          source: "permission-request",
          message: `会话 ${conversationId} 需要权限确认，请切回该会话处理。`,
        });
      } else {
        chatScreenRef.value?.scrollToBottom();
      }
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
        latestRunningToolExecutionIdByName(state.toolExecutionLogs, toolName) ||
        "";
      const streamedInput = toolId ? state.toolInputById.get(toolId) ?? "" : "";
      const fallbackInput = (payload.tool_use_input ?? "").trim();
      const rawInput = streamedInput.trim().length > 0 ? streamedInput : fallbackInput;
      const result = (payload.tool_result ?? "").trim();

      if (isActive) {
        const info = summarizeToolInfo(toolName, rawInput);
        if (info) {
          state.assistantResponse += `\n> Tool info: ${info}\n`;
        }
        state.assistantResponse += `\n> Tool done: ${toolName}\n`;
      }

      completeToolExecutionTraceInState(
        conversationId,
        state,
        toolId || null,
        toolName,
        result,
        "completed",
        persistToolExecutionLog,
        rawInput,
      );

      if (toolId) {
        state.toolInputById.delete(toolId);
        state.toolNameById.delete(toolId);
      }

      if (result) {
        if (isActive) {
          const planModeChange = parsePlanModeChange(result);
          if (planModeChange) {
            const nextIsPlanMode = planModeChange.mode === "plan";
            planMode.value = nextIsPlanMode;
            agentMode.value = nextIsPlanMode ? "plan" : "agent";
          }
        }

        const needsUserInput = parseNeedsUserInput(result);
        if (needsUserInput) {
          state.pendingPermissionRequestId = null;
          state.pendingQuestion = needsUserInput;
          state.isGenerating = false;
          const rendered = renderToolResult(result);
          const preview =
            rendered.length > 1200 ? `${rendered.slice(0, 1200)}\n...(truncated)` : rendered;
          state.assistantResponse += `\n${preview}\n`;

          if (!isActive) {
            emitToast({
              variant: "info",
              source: "permission-request",
              message: `会话 ${conversationId} 需要继续输入，请切回该会话。`,
            });
          }
        }
      }

      if (isActive) {
        chatScreenRef.value?.scrollToBottom();
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
      markRunningToolExecutionsInState(
        conversationId,
        state,
        "cancelled",
        persistToolExecutionLog,
      );

      if (isActive) {
        finalizeOrStopTurn(payload.token_usage);
        resetTurnRuntimeState(activeRuntimeRefs);
        if (activeConversationId.value) {
          runtimeStateByConversation.delete(normalizeConversationId(activeConversationId.value));
        }
      } else {
        resetBackgroundRuntimeState(conversationId, state);
      }
      return;
    }

    if (turnState === "error") {
      markRunningToolExecutionsInState(
        conversationId,
        state,
        "error",
        persistToolExecutionLog,
      );

      if (isActive) {
        state.isGenerating = false;
        state.assistantResponse = "";
        state.assistantReasoning = "";
        state.assistantTokenUsage = undefined;
        state.assistantTurnCost = undefined;
        resetTurnRuntimeState(activeRuntimeRefs);
        if (activeConversationId.value) {
          runtimeStateByConversation.delete(normalizeConversationId(activeConversationId.value));
        }
      } else {
        resetBackgroundRuntimeState(conversationId, state);
      }

      const detail = (payload.text ?? "").trim();
      emitToast({
        variant: "error",
        source: "chat-stream",
        message: detail || (isActive
          ? `Provider error: ${stopReason || "unknown"}`
          : `会话 ${conversationId} 回复失败: ${stopReason || "unknown"}`),
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

    if (!shouldFinalize) {
      return;
    }

    const preservePendingPrompt =
      shouldPreservePendingPromptOnStop(turnState, stopReason) ||
      !!state.pendingPermissionRequestId ||
      !!state.pendingQuestion;
    markRunningToolExecutionsInState(
      conversationId,
      state,
      "completed",
      persistToolExecutionLog,
    );

    if (isActive) {
      finalizeOrStopTurn(payload.token_usage);

      if (!preservePendingPrompt) {
        resetTurnRuntimeState(activeRuntimeRefs);
      } else {
        resetToolTrackingState(activeRuntimeRefs);
      }

      if (activeConversationId.value && !preservePendingPrompt) {
        runtimeStateByConversation.delete(normalizeConversationId(activeConversationId.value));
      }
      return;
    }

    await finalizeBackgroundTurn(
      conversationId,
      state,
      payload.token_usage,
      preservePendingPrompt,
    );
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
          void handleChatStreamEvent(targetConversationId, payload, "background");
          return;
        }
        void handleChatStreamEvent(targetConversationId, payload, "active");
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
    assistantReasoning,
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
