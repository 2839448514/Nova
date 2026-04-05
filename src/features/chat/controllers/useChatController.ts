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
  listConversationRagDocuments,
  listConversations,
  loadConversationHistory,
  sendChatMessage,
  submitPermissionDecision,
  type RagDocumentMeta,
  upsertConversationRagDocuments,
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
  TurnCost,
  UploadedRagFile,
} from "../../../lib/chat-types";

export type MainView = "chat" | "hooks";

type BackendErrorEvent = {
  source?: string;
  message?: string;
  stage?: string | null;
};

type ChatScreenHandle = {
  scrollToBottom: () => void;
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
  const chatScreenRef = ref<ChatScreenHandle | null>(null);
  const toolInputById = new Map<string, string>();
  const toolNameById = new Map<string, string>();

  let unlistenChatStream: UnlistenFn | null = null;
  let unlistenBackendError: UnlistenFn | null = null;

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
    activeConversationId.value = id;
    planMode.value = agentMode.value === "plan";
    pendingUploads.value = [];
    try {
      const saved = await loadConversationHistory(id);
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
      await loadConversationMemory(id);
      await refreshConversationFiles(id);
    } catch (err) {
      console.error("Failed to load conversation messages:", err);
      messages.value = [];
      conversationFiles.value = [];
    }
  }

  async function persistMessage(msg: ChatMessage) {
    if (!activeConversationId.value) return;
    try {
      await appendConversationMessage(activeConversationId.value, msg);
      await refreshConversations();
    } catch (err) {
      console.error("Failed to persist message:", err);
    }
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

    let uploadedAttachments: ChatAttachment[] = [];
    if (filesToSend.length > 0) {
      try {
        const result = await upsertConversationRagDocuments(
          activeConversationId.value,
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
        await refreshConversationFiles(activeConversationId.value);

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
    await persistMessage(userMsg);
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
        activeConversationId.value || null,
        rustMessages,
        planMode.value,
        agentMode.value,
      );
    } catch (err: any) {
      if (!isGenerating.value) {
        return;
      }
      console.error("Chat error:", err);
      const errorMsg: ChatMessage = { role: "assistant", content: `API Error: ${err}` };
      messages.value.push(errorMsg);
      await persistMessage(errorMsg);
      assistantResponse.value = "";
      assistantTokenUsage.value = undefined;
      assistantTurnCost.value = undefined;
      isGenerating.value = false;
      resetTurnRuntimeState();
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
    if (isGenerating.value || isCreatingNewChat.value) return;

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

      activeConversationId.value = id;
      messages.value = [];
      pendingUploads.value = [];
      assistantResponse.value = "";
      isGenerating.value = false;
      planMode.value = agentMode.value === "plan";
      conversationFiles.value = [];
      await refreshConversationFiles(id);
    } finally {
      isCreatingNewChat.value = false;
    }
  }

  async function handleSelectConversation(id: string) {
    if (!id || id === activeConversationId.value || isGenerating.value) return;
    mainView.value = "chat";
    resetPendingPromptState();
    assistantResponse.value = "";
    isGenerating.value = false;
    planMode.value = agentMode.value === "plan";
    pendingUploads.value = [];
    await loadConversation(id);
  }

  async function handleDeleteConversation(id: string) {
    if (!id || isGenerating.value) return;
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
        if (payload.type === "text" && payload.text) {
          assistantResponse.value += payload.text;
          chatScreenRef.value?.scrollToBottom();
        } else if (payload.type === "tool-use-start") {
          currentToolCalls.value += 1;
          currentToolStartedAt.value = Date.now();
          const toolName = payload.tool_use_name ?? "unknown";
          const toolId = payload.tool_use_id ?? "";
          if (toolId) {
            toolNameById.set(toolId, toolName);
            if (!toolInputById.has(toolId)) {
              toolInputById.set(toolId, "");
            }
          }
          assistantResponse.value += `\n> Using tool: ${toolName}...\n`;
          chatScreenRef.value?.scrollToBottom();
        } else if (payload.type === "tool-json-delta") {
          const toolId = payload.tool_use_id ?? "";
          if (toolId && payload.tool_use_input) {
            const prev = toolInputById.get(toolId) ?? "";
            toolInputById.set(toolId, prev + payload.tool_use_input);
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
          const toolId = payload.tool_use_id ?? "";
          const toolName =
            payload.tool_use_name ?? (toolId ? toolNameById.get(toolId) : undefined) ?? "unknown";
          const rawInput = toolId ? toolInputById.get(toolId) ?? "" : "";
          const info = summarizeToolInfo(toolName, rawInput);
          if (info) {
            assistantResponse.value += `\n> Tool info: ${info}\n`;
          }
          assistantResponse.value += `\n> Tool done: ${toolName}\n`;
          if (toolId) {
            toolInputById.delete(toolId);
            toolNameById.delete(toolId);
          }
          const result = (payload.tool_result ?? "").trim();
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
            finalizeOrStopTurn(payload.token_usage);
            resetTurnRuntimeState();
            return;
          }

          if (turnState === "error") {
            isGenerating.value = false;
            assistantResponse.value = "";
            assistantTokenUsage.value = undefined;
            assistantTurnCost.value = undefined;
            resetTurnRuntimeState();
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
            finalizeOrStopTurn(payload.token_usage);
            resetTurnRuntimeState();
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
  });

  onUnmounted(() => {
    if (unlistenChatStream) unlistenChatStream();
    if (unlistenBackendError) unlistenBackendError();
  });

  return {
    messages,
    isGenerating,
    assistantResponse,
    assistantTokenUsage,
    assistantTurnCost,
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
