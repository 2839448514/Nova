import type { Ref } from "vue";

import { emitToast } from "../../../lib/toast";
import {
  parseNeedsUserInput,
  parsePlanModeChange,
  renderToolResult,
} from "../../../lib/chat-payloads";
import type {
  AgentMode,
  ChatMessageEvent,
  NeedsUserInputPayload,
  ToolExecutionEntry,
  TurnCost,
} from "../../../lib/chat-types";
import { cancelChatMessage, submitPermissionDecision } from "../services/chat-api";
import { summarizeToolInfo } from "../utils/tool-info";

import type { ConversationTurnRuntimeState } from "./chat-runtime-state";
import {
  appendToolExecutionInput,
  latestRunningToolExecutionIdByName,
  startToolExecutionTrace,
} from "./chat-tool-trace";

type ChatStreamHandlersDeps = {
  activeConversationId: Ref<string>;
  agentMode: Ref<AgentMode>;
  assistantReasoning: Ref<string>;
  assistantResponse: Ref<string>;
  assistantTokenUsage: Ref<number | undefined>;
  assistantTurnCost: Ref<TurnCost | undefined>;
  currentOutputTokens: Ref<number>;
  currentToolCalls: Ref<number>;
  currentToolDurationMs: Ref<number>;
  currentToolStartedAt: Ref<number | null>;
  isGenerating: Ref<boolean>;
  pendingPermissionRequestId: Ref<string | null>;
  pendingQuestion: Ref<NeedsUserInputPayload | null>;
  planMode: Ref<boolean>;
  toolExecutionLogs: Ref<ToolExecutionEntry[]>;
  toolInputById: Map<string, string>;
  toolNameById: Map<string, string>;
  cleanupRuntimeStateIfIdle: (conversationId: string) => void;
  completeActiveToolExecutionTrace: (
    toolId: string | null,
    toolName: string,
    result: string,
    status: ToolExecutionEntry["status"],
    inputFallback?: string,
  ) => void;
  completeStateToolExecutionTrace: (
    conversationId: string,
    state: ConversationTurnRuntimeState,
    toolId: string | null,
    toolName: string,
    result: string,
    status: ToolExecutionEntry["status"],
    inputFallback?: string,
  ) => void;
  deleteRuntimeState: (conversationId?: string | null) => void;
  ensureRuntimeState: (conversationId: string) => ConversationTurnRuntimeState;
  finalizeBackgroundTurn: (
    conversationId: string,
    state: ConversationTurnRuntimeState,
    tokenUsage?: number,
    preservePendingPrompt?: boolean,
  ) => Promise<void>;
  finalizeOrStopTurn: (tokenUsage?: number) => void;
  markActiveRunningToolExecutions: (
    status: "completed" | "error" | "cancelled",
  ) => void;
  markStateRunningToolExecutions: (
    conversationId: string,
    state: ConversationTurnRuntimeState,
    status: "completed" | "error" | "cancelled",
  ) => void;
  resetPendingPromptState: () => void;
  resetToolTrackingState: () => void;
  resetTurnRuntimeState: () => void;
  scrollChatToBottom: () => void;
  shouldPreservePendingPromptOnStop: (turnState: string, stopReason: string) => boolean;
};

function clearBackgroundState(
  deps: ChatStreamHandlersDeps,
  conversationId: string,
  state: ConversationTurnRuntimeState,
) {
  state.isGenerating = false;
  state.assistantResponse = "";
  state.assistantReasoning = "";
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
  deps.cleanupRuntimeStateIfIdle(conversationId);
}

export function createChatStreamHandlers(deps: ChatStreamHandlersDeps) {
  async function handleBackgroundChatStreamEvent(
    conversationId: string,
    payload: ChatMessageEvent,
  ) {
    const state = deps.ensureRuntimeState(conversationId);

    if (payload.type === "text" && payload.text) {
      state.isGenerating = true;
      state.assistantResponse += payload.text;
      return;
    }

    if (payload.type === "reasoning" && payload.text) {
      state.isGenerating = true;
      state.assistantReasoning += payload.text;
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
      startToolExecutionTrace(state.toolExecutionLogs, toolId, toolName);
      return;
    }

    if (payload.type === "tool-json-delta") {
      const toolId = (payload.tool_use_id ?? "").trim();
      if (toolId && payload.tool_use_input) {
        const previous = state.toolInputById.get(toolId) ?? "";
        state.toolInputById.set(toolId, previous + payload.tool_use_input);
        appendToolExecutionInput(state.toolExecutionLogs, toolId, payload.tool_use_input);
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
        rawToolId || latestRunningToolExecutionIdByName(state.toolExecutionLogs, toolName) || "";
      const streamedInput = toolId ? state.toolInputById.get(toolId) ?? "" : "";
      const fallbackInput = (payload.tool_use_input ?? "").trim();
      const rawInput = streamedInput.trim().length > 0 ? streamedInput : fallbackInput;
      const result = (payload.tool_result ?? "").trim();
      deps.completeStateToolExecutionTrace(
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
      deps.markStateRunningToolExecutions(conversationId, state, "cancelled");
      clearBackgroundState(deps, conversationId, state);
      return;
    }

    if (turnState === "error") {
      deps.markStateRunningToolExecutions(conversationId, state, "error");
      clearBackgroundState(deps, conversationId, state);

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
      const preservePendingPrompt =
        deps.shouldPreservePendingPromptOnStop(turnState, stopReason) ||
        !!state.pendingPermissionRequestId ||
        !!state.pendingQuestion;
      deps.markStateRunningToolExecutions(conversationId, state, "completed");
      await deps.finalizeBackgroundTurn(
        conversationId,
        state,
        payload.token_usage,
        preservePendingPrompt,
      );
    }
  }

  function handleActiveChatStreamEvent(payload: ChatMessageEvent) {
    if (payload.type === "text" && payload.text) {
      deps.assistantResponse.value += payload.text;
      deps.scrollChatToBottom();
      return;
    }

    if (payload.type === "reasoning" && payload.text) {
      deps.assistantReasoning.value += payload.text;
      deps.scrollChatToBottom();
      return;
    }

    if (payload.type === "tool-use-start") {
      deps.currentToolCalls.value += 1;
      deps.currentToolStartedAt.value = Date.now();
      const toolName = (payload.tool_use_name ?? "unknown").trim() || "unknown";
      const rawToolId = (payload.tool_use_id ?? "").trim();
      const toolId = rawToolId || `tool-${Date.now()}-${deps.currentToolCalls.value}`;

      deps.toolNameById.set(toolId, toolName);
      if (!deps.toolInputById.has(toolId)) {
        deps.toolInputById.set(toolId, "");
      }

      startToolExecutionTrace(deps.toolExecutionLogs.value, toolId, toolName);
      deps.assistantResponse.value += `\n> Using tool: ${toolName}...\n`;
      deps.scrollChatToBottom();
      return;
    }

    if (payload.type === "tool-json-delta") {
      const toolId = (payload.tool_use_id ?? "").trim();
      if (toolId && payload.tool_use_input) {
        const previous = deps.toolInputById.get(toolId) ?? "";
        deps.toolInputById.set(toolId, previous + payload.tool_use_input);
        appendToolExecutionInput(deps.toolExecutionLogs.value, toolId, payload.tool_use_input);
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
          message: "收到权限请求但缺少 request_id，已尝试取消当前回合。",
        });
        void cancelChatMessage(deps.activeConversationId.value || null).catch((err) => {
          emitToast({
            variant: "error",
            source: "permission-request",
            message: `取消异常权限请求失败: ${String(err)}`,
          });
        });
        deps.resetPendingPromptState();
        return;
      }

      if (!parsed) {
        emitToast({
          variant: "error",
          source: "permission-request",
          message: "收到权限请求但参数无效，已自动拒绝该请求。",
        });
        void submitPermissionDecision(
          deps.activeConversationId.value || null,
          requestId,
          "deny_session",
        ).catch((err) => {
          emitToast({
            variant: "error",
            source: "permission-request",
            message: `自动拒绝权限请求失败: ${String(err)}`,
          });
        });
        deps.resetPendingPromptState();
        return;
      }

      deps.pendingPermissionRequestId.value = requestId;
      deps.pendingQuestion.value = parsed;
      deps.scrollChatToBottom();
      return;
    }

    if (payload.type === "tool-result") {
      if (deps.currentToolStartedAt.value) {
        deps.currentToolDurationMs.value += Math.max(
          0,
          Date.now() - deps.currentToolStartedAt.value,
        );
        deps.currentToolStartedAt.value = null;
      }
      const rawToolId = (payload.tool_use_id ?? "").trim();
      const fallbackToolName = (payload.tool_use_name ?? "").trim();
      const toolName =
        fallbackToolName ||
        (rawToolId ? deps.toolNameById.get(rawToolId) : undefined) ||
        "unknown";
      const toolId =
        rawToolId || latestRunningToolExecutionIdByName(deps.toolExecutionLogs.value, toolName) || "";
      const streamedInput = toolId ? deps.toolInputById.get(toolId) ?? "" : "";
      const fallbackInput = (payload.tool_use_input ?? "").trim();
      const rawInput = streamedInput.trim().length > 0 ? streamedInput : fallbackInput;
      const info = summarizeToolInfo(toolName, rawInput);
      if (info) {
        deps.assistantResponse.value += `\n> Tool info: ${info}\n`;
      }
      deps.assistantResponse.value += `\n> Tool done: ${toolName}\n`;
      const result = (payload.tool_result ?? "").trim();
      deps.completeActiveToolExecutionTrace(toolId || null, toolName, result, "completed", rawInput);

      if (toolId) {
        deps.toolInputById.delete(toolId);
        deps.toolNameById.delete(toolId);
      }

      if (result) {
        const planModeChange = parsePlanModeChange(result);
        if (planModeChange) {
          const isPlan = planModeChange.mode === "plan";
          deps.planMode.value = isPlan;
          deps.agentMode.value = isPlan ? "plan" : "agent";
        }
        const needsUserInput = parseNeedsUserInput(result);
        if (needsUserInput) {
          deps.pendingPermissionRequestId.value = null;
          deps.pendingQuestion.value = needsUserInput;
          deps.isGenerating.value = false;
          const rendered = renderToolResult(result);
          const preview =
            rendered.length > 1200 ? `${rendered.slice(0, 1200)}\n...(truncated)` : rendered;
          deps.assistantResponse.value += `\n${preview}\n`;
        }
      }
      deps.scrollChatToBottom();
      return;
    }

    if (payload.type === "token-usage") {
      deps.assistantTokenUsage.value = payload.token_usage;
      deps.currentOutputTokens.value = payload.token_usage ?? deps.currentOutputTokens.value;
      return;
    }

    if (payload.type !== "stop") {
      return;
    }

    const stopReason = payload.stop_reason ?? "";
    const turnState = payload.turn_state ?? "";

    if (turnState === "cancelled" || stopReason === "cancelled") {
      deps.markActiveRunningToolExecutions("cancelled");
      deps.finalizeOrStopTurn(payload.token_usage);
      deps.resetTurnRuntimeState();
      deps.deleteRuntimeState(deps.activeConversationId.value);
      return;
    }

    if (turnState === "error") {
      deps.markActiveRunningToolExecutions("error");
      deps.isGenerating.value = false;
      deps.assistantResponse.value = "";
      deps.assistantReasoning.value = "";
      deps.assistantTokenUsage.value = undefined;
      deps.assistantTurnCost.value = undefined;
      deps.resetTurnRuntimeState();
      deps.deleteRuntimeState(deps.activeConversationId.value);
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
      const preservePendingPrompt =
        deps.shouldPreservePendingPromptOnStop(turnState, stopReason) ||
        !!deps.pendingPermissionRequestId.value ||
        !!deps.pendingQuestion.value;
      deps.markActiveRunningToolExecutions("completed");
      deps.finalizeOrStopTurn(payload.token_usage);

      if (!preservePendingPrompt) {
        deps.resetTurnRuntimeState();
      } else {
        deps.resetToolTrackingState();
      }

      if (!preservePendingPrompt) {
        deps.deleteRuntimeState(deps.activeConversationId.value);
      }
    }
  }

  async function handleChatStreamEvent(payload: ChatMessageEvent) {
    const payloadConversationId = (payload.conversation_id ?? "").trim();
    const targetConversationId = payloadConversationId || deps.activeConversationId.value;
    if (!targetConversationId) {
      return;
    }

    if (targetConversationId !== deps.activeConversationId.value) {
      await handleBackgroundChatStreamEvent(targetConversationId, payload);
      return;
    }

    handleActiveChatStreamEvent(payload);
  }

  return {
    handleBackgroundChatStreamEvent,
    handleChatStreamEvent,
  };
}
