import { ref, onMounted, onUnmounted } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { emitToast } from "../../../lib/toast";
import {
  buildPendingQuestionReply,
  extractPermissionActionFromAnswers,
} from "../../../lib/chat-payloads";
import {
  estimateTokens,
} from "../utils/session-memory";
import { createConversationActions } from "./chat-conversation-actions";
import { createFlowNodeStore } from "./chat-flow-nodes";
import {
  createConversationRuntimeRegistry,
  type ConversationTurnRuntimeState,
} from "./chat-runtime-state";
import { createChatStreamHandlers } from "./chat-stream-handlers";
import {
  completeToolExecutionTrace,
  markRunningToolExecutions,
} from "./chat-tool-trace";
import {
  cancelChatMessage,
  deleteConversation,
  listConversationRagDocuments,
  sendChatMessage,
  submitPermissionDecision,
  type RagDocumentMeta,
  upsertConversationRagDocuments,
  upsertConversationToolLog,
} from "../services/chat-api";
import type {
  AgentMode,
  AskUserAnswerSubmission,
  ChatAttachment,
  ChatMessage,
  ChatMessageEvent,
  ConversationMemory,
  ConversationMeta,
  FlowNodeEntry,
  NeedsUserInputPayload,
  PendingUploadFile,
  ToolExecutionEntry,
  TurnCost,
  UploadedImageFile,
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
  scrollLastUserMessageToTop: () => void;
};

type ModelTextBlock = {
  type: "text";
  text: string;
};

type ModelImageBlock = {
  type: "image";
  source: {
    type: "base64";
    media_type: string;
    data: string;
  };
};

type ModelMessage = {
  role: "user" | "assistant";
  content: string | Array<ModelTextBlock | ModelImageBlock>;
};

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
  const codingMode = ref(false);
  const isCreatingNewChat = ref(false);
  const isSidebarOpen = ref(true);
  const toolExecutionLogs = ref<ToolExecutionEntry[]>([]);
  const chatScreenRef = ref<ChatScreenHandle | null>(null);
  const toolInputById = new Map<string, string>();
  const toolNameById = new Map<string, string>();
  const { flowNodes, clearFlowNodes, upsertFlowNode } = createFlowNodeStore();
  const {
    clearActiveRuntimeState,
    cleanupRuntimeStateIfIdle,
    clearRuntimeStates,
    deleteRuntimeState,
    ensureRuntimeState,
    hasAnyGeneratingConversations,
    isSpecificConversationGenerating,
    restoreRuntimeState,
    stashRuntimeState,
  } = createConversationRuntimeRegistry({
    activeConversationId,
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
  });

  let unlistenChatStream: UnlistenFn | null = null;
  let unlistenBackendError: UnlistenFn | null = null;
  let unlistenScheduledTaskTrigger: UnlistenFn | null = null;
  let unlistenFlowNode: UnlistenFn | null = null;

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

  function persistToolExecutionLog(entry: ToolExecutionEntry, conversationId = activeConversationId.value) {
    if (!conversationId || entry.status === "running") {
      return;
    }

    void upsertConversationToolLog(conversationId, entry).catch((err) => {
      console.error("Failed to persist tool execution log:", err);
    });
  }

  function completeActiveToolExecutionTrace(
    toolId: string | null,
    toolName: string,
    result: string,
    status: ToolExecutionEntry["status"],
    inputFallback?: string,
  ) {
    const updatedEntry = completeToolExecutionTrace(
      toolExecutionLogs.value,
      toolId,
      toolName,
      result,
      status,
      inputFallback,
    );
    if (updatedEntry) {
      persistToolExecutionLog(updatedEntry);
    }
  }

  function markActiveRunningToolExecutions(status: "completed" | "error" | "cancelled") {
    const finalizedEntries = markRunningToolExecutions(toolExecutionLogs.value, status);
    for (const entry of finalizedEntries) {
      persistToolExecutionLog(entry);
    }
  }

  function completeStateToolExecutionTrace(
    conversationId: string,
    state: ConversationTurnRuntimeState,
    toolId: string | null,
    toolName: string,
    result: string,
    status: ToolExecutionEntry["status"],
    inputFallback?: string,
  ) {
    const updatedEntry = completeToolExecutionTrace(
      state.toolExecutionLogs,
      toolId,
      toolName,
      result,
      status,
      inputFallback,
    );
    if (updatedEntry) {
      persistToolExecutionLog(updatedEntry, conversationId);
    }
  }

  function markStateRunningToolExecutions(
    conversationId: string,
    state: ConversationTurnRuntimeState,
    status: "completed" | "error" | "cancelled",
  ) {
    const finalizedEntries = markRunningToolExecutions(state.toolExecutionLogs, status);
    for (const entry of finalizedEntries) {
      persistToolExecutionLog(entry, conversationId);
    }
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
    codingMode.value = mode === "coding";
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
    const names =
      msg.attachments
        ?.filter((item) => item.kind !== "image")
        .map((item) => item.sourceName)
        .filter(Boolean) ?? [];
    const ragNotice =
      names.length > 0
        ? `\n\n已上传文件（可在会话RAG中检索）：${names.join("，")}\n若你不确定答案，请先在RAG中检索相关片段，再视情况使用 web_search / web_fetch。`
        : "";

    if (content) {
      return `${content}${ragNotice}`;
    }

    if (names.length > 0) {
      return `请优先结合我上传的文件回答。${ragNotice}`;
    }

    return "";
  }

  function isDocumentUploadFile(file: PendingUploadFile): file is UploadedRagFile {
    return file.kind === "document";
  }

  function isImageUploadFile(file: PendingUploadFile): file is UploadedImageFile {
    return file.kind === "image";
  }

  function isImageAttachment(item: ChatAttachment): item is ChatAttachment & {
    kind: "image";
    mediaType: string;
    data: string;
  } {
    return item.kind === "image" && !!item.mediaType && !!item.data;
  }

  function toAttachmentMeta(files: PendingUploadFile[]): ChatAttachment[] {
    return files.map((file) => {
      if (file.kind === "image") {
        return {
          sourceName: file.sourceName,
          mimeType: file.mimeType,
          size: file.size,
          kind: "image",
          mediaType: file.mediaType,
          data: file.data,
        };
      }

      return {
        sourceName: file.sourceName,
        mimeType: file.mimeType,
        size: file.size,
        kind: "document",
      };
    });
  }

  function buildModelMessage(msg: ChatMessage): ModelMessage {
    const textContent = formatMessageContentForModel(msg);
    if (msg.role !== "user") {
      return {
        role: msg.role,
        content: textContent,
      };
    }

    const imageAttachments = (msg.attachments ?? []).filter(isImageAttachment);
    if (imageAttachments.length === 0) {
      return {
        role: msg.role,
        content: textContent,
      };
    }

    const fallbackText = textContent || "请结合我上传的图片回答。";
    const blocks: Array<ModelTextBlock | ModelImageBlock> = [
      {
        type: "text",
        text: fallbackText,
      },
    ];

    for (const image of imageAttachments) {
      const mediaType = (image.mediaType || image.mimeType || "").trim().toLowerCase();
      const data = (image.data || "").trim();
      if (!mediaType || !data) {
        continue;
      }

      blocks.push({
        type: "image",
        source: {
          type: "base64",
          media_type: mediaType,
          data,
        },
      });
    }

    if (blocks.length <= 1) {
      return {
        role: msg.role,
        content: fallbackText,
      };
    }

    return {
      role: msg.role,
      content: blocks,
    };
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

  const {
    buildAssistantCostForState,
    createNewConversation,
    loadConversation,
    persistConversationMemory,
    persistMessage,
    refreshConversations,
  } = createConversationActions({
    activeConversationId,
    agentMode,
    codingMode,
    conversationFiles,
    conversationMemory,
    conversations,
    messages,
    pendingUploads,
    planMode,
    toolExecutionLogs,
    clearActiveRuntimeState,
    refreshConversationFiles,
    restoreRuntimeState,
    stashRuntimeState,
  });

  function shouldPreservePendingPromptOnStop(turnState: string, stopReason: string): boolean {
    return (
      turnState === "awaiting_user_input" ||
      turnState === "needs_user_input" ||
      stopReason === "needs_user_input"
    );
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
      cleanupRuntimeStateIfIdle(conversationId);
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

    const cost = buildAssistantCost();
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
      deleteRuntimeState(activeConversationId.value);
    }
    chatScreenRef.value?.scrollToBottom();
  }

  const { handleChatStreamEvent } = createChatStreamHandlers({
    activeConversationId,
    agentMode,
    assistantReasoning,
    assistantResponse,
    assistantTokenUsage,
    assistantTurnCost,
    currentOutputTokens,
    currentToolCalls,
    currentToolDurationMs,
    currentToolStartedAt,
    isGenerating,
    pendingPermissionRequestId,
    pendingQuestion,
    planMode,
    toolExecutionLogs,
    toolInputById,
    toolNameById,
    cleanupRuntimeStateIfIdle,
    completeActiveToolExecutionTrace,
    completeStateToolExecutionTrace,
    deleteRuntimeState,
    ensureRuntimeState,
    finalizeBackgroundTurn,
    finalizeOrStopTurn,
    markActiveRunningToolExecutions,
    markStateRunningToolExecutions,
    resetPendingPromptState,
    resetToolTrackingState,
    resetTurnRuntimeState,
    scrollChatToBottom: () => {
      chatScreenRef.value?.scrollToBottom();
    },
    shouldPreservePendingPromptOnStop,
  });

  async function handleSendMessage(userText: string) {
    if (isGenerating.value) return;
    const text = userText.trim();
    const filesToSend = pendingUploads.value.slice();
    const textFiles = filesToSend.filter(isDocumentUploadFile);
    const imageFiles = filesToSend.filter(isImageUploadFile);
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
    clearFlowNodes();
    currentInputTokens.value = estimateInputTokensForTurn(
      modelUserText,
      uploadedAttachmentNames,
    );
    resetToolTrackingState();

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
        resetTurnRuntimeState();
        deleteRuntimeState(sendingConversationId);
      } else {
        const backgroundState = ensureRuntimeState(sendingConversationId);
        backgroundState.assistantResponse = "";
        backgroundState.assistantReasoning = "";
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

    deleteRuntimeState(id);
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

    clearRuntimeStates();
    resetTurnRuntimeState();
    assistantResponse.value = "";
    assistantReasoning.value = "";
    assistantTokenUsage.value = undefined;
    assistantTurnCost.value = undefined;
    pendingUploads.value = [];
    toolExecutionLogs.value = [];
    clearFlowNodes();
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
        void handleChatStreamEvent(event.payload);
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
      unlistenFlowNode = await listen<{
        node_id: string;
        label: string;
        status: string;
        detail?: string;
        conversation_id?: string;
      }>("flow-node", (event) => {
        const p = event.payload;
        const entry: FlowNodeEntry = {
          nodeId: p.node_id,
          label: p.label,
          status: p.status as FlowNodeEntry["status"],
          detail: p.detail,
          conversationId: p.conversation_id,
          timestamp: Date.now(),
        };
        // 更新已有节点（同 nodeId 的 running→completed/skipped/error）
        upsertFlowNode(entry);
      });

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
    if (unlistenFlowNode) unlistenFlowNode();
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
    flowNodes,
    conversations,
    activeConversationId,
    pendingQuestion,
    pendingUploads,
    conversationFiles,
    agentMode,
    planMode,
    codingMode,
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
