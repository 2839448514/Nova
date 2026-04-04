<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { emitToast } from "./lib/toast";
import {
  buildPendingQuestionReply,
  parseNeedsUserInput,
  parsePlanModeChange,
  renderToolResult,
} from "./lib/chat-payloads";
import type {
  AskUserAnswerSubmission,
  ChatMessage,
  ChatMessageEvent,
  ConversationMemory,
  ConversationMeta,
  NeedsUserInputPayload,
  PersistedMessage,
  TurnCost,
} from "./lib/chat-types";
import Sidebar from "./components/layout/Sidebar.vue";
import WelcomeScreen from "./components/chat/WelcomeScreen.vue";
import ChatScreen from "./components/chat/ChatScreen.vue";
import GlobalToastHost from "./components/layout/GlobalToastHost.vue";

type BackendErrorEvent = {
  source?: string;
  message?: string;
  stage?: string | null;
};

const messages = ref<ChatMessage[]>([]);
const isGenerating = ref(false);
const assistantResponse = ref("");
const assistantTokenUsage = ref<number | undefined>(undefined);
const assistantTurnCost = ref<TurnCost | undefined>(undefined);
const conversations = ref<ConversationMeta[]>([]);
const activeConversationId = ref("");
const pendingQuestion = ref<NeedsUserInputPayload | null>(null);
const conversationMemory = ref<ConversationMemory | null>(null);
const currentToolStartedAt = ref<number | null>(null);
const currentToolCalls = ref(0);
const currentToolDurationMs = ref(0);
const currentInputTokens = ref(0);
const currentOutputTokens = ref(0);
const planMode = ref(false);
const toolInputById = new Map<string, string>();
const toolNameById = new Map<string, string>();

function readStringField(input: Record<string, any> | null, keys: string[]): string | null {
  if (!input) return null;
  for (const key of keys) {
    const value = input[key];
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return null;
}

function truncateText(text: string, maxLen: number): string {
  const v = (text || "").trim();
  if (!v) return v;
  return v.length > maxLen ? `${v.slice(0, maxLen)}...` : v;
}

function summarizeToolInfo(toolName: string, rawInput: string): string | null {
  let parsed: Record<string, any> | null = null;
  if (rawInput.trim()) {
    try {
      parsed = JSON.parse(rawInput);
    } catch {
      parsed = null;
    }
  }

  const lower = (toolName || "").toLowerCase();

  if (lower.includes("bash") || lower.includes("powershell") || lower.includes("shell")) {
    const command = readStringField(parsed, ["command", "cmd", "script"]);
    if (!command) return "shell action";
    const first = command.split(/\s+/).filter(Boolean)[0] || "unknown";
    return `command=${first}`;
  }

  if (
    lower.includes("browser") ||
    lower.includes("web_fetch") ||
    lower.includes("web_search") ||
    lower.includes("navigate")
  ) {
    const url = readStringField(parsed, ["url", "website", "uri"]);
    if (url) {
      try {
        const u = new URL(url);
        return `site=${truncateText(`${u.host}${u.pathname}`, 72)}`;
      } catch {
        return `site=${truncateText(url, 72)}`;
      }
    }
    const query = readStringField(parsed, ["query", "text"]);
    if (query) {
      return `query=${truncateText(query, 48)}`;
    }
    return "browser action";
  }

  if (
    lower.includes("file_read") ||
    lower.includes("write_file") ||
    lower.includes("file_edit") ||
    lower.includes("replace_string")
  ) {
    const path = readStringField(parsed, ["path", "filePath", "uri"]);
    return path ? `path=${truncateText(path, 64)}` : "file operation";
  }

  if (lower.startsWith("mcp/")) {
    const url = readStringField(parsed, ["url", "uri"]);
    if (url) return `mcp site=${truncateText(url, 64)}`;
    if (parsed) {
      const keys = Object.keys(parsed).slice(0, 3).join(",");
      return keys ? `mcp args=${keys}` : "mcp call";
    }
    return "mcp call";
  }

  if (parsed) {
    const keys = Object.keys(parsed).slice(0, 2).join(",");
    if (keys) return `args=${keys}`;
  }

  return null;
}

let unlistenChatStream: UnlistenFn | null = null;
let unlistenBackendError: UnlistenFn | null = null;
const isSidebarOpen = ref(true);
const chatScreenRef = ref<InstanceType<typeof ChatScreen> | null>(null);

function finalizeAssistantTurn(tokenUsage?: number) {
  const finalText = assistantResponse.value.trim();
  const cost = buildAssistantCost();
  assistantTurnCost.value = cost;
  const assistantMsg: ChatMessage = {
    role: "assistant",
    content: finalText || "（本轮没有返回可显示的文本内容）",
    tokenUsage: tokenUsage ?? assistantTokenUsage.value,
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

function buildConversationTitle(source: string): string {
  const t = source.trim();
  return t.length > 24 ? `${t.slice(0, 24)}...` : t;
}

function estimateTokens(text: string): number {
  const n = text.trim().length;
  if (n <= 0) return 0;
  return Math.ceil(n / 4);
}

function estimateInputTokensForTurn(userText: string): number {
  const historyText = messages.value
    .slice(-12)
    .map((m) => m.content)
    .join("\n");
  const memoryText = conversationMemory.value
    ? `Summary: ${conversationMemory.value.summary}\nFacts: ${conversationMemory.value.keyFacts.join("; ")}`
    : "";
  return estimateTokens(`${historyText}\n${memoryText}\n${userText}`);
}

function buildAssistantCost(): TurnCost {
  return {
    inputTokens: currentInputTokens.value,
    outputTokens: currentOutputTokens.value,
    toolCalls: currentToolCalls.value,
    toolDurationMs: currentToolDurationMs.value,
  };
}

function extractSessionMemory(): { summary: string; keyFacts: string[] } {
  const recent = messages.value.slice(-12);
  const summaryParts = recent.slice(-6).map((m) => `${m.role === "user" ? "用户" : "Nova"}: ${m.content.replace(/\s+/g, " ").slice(0, 120)}`);
  const summary = summaryParts.join(" | ").slice(0, 800);

  const facts: string[] = [];
  for (const m of recent) {
    const lines = m.content.split(/\n+/).map((s) => s.trim()).filter(Boolean);
    for (const line of lines) {
      if (facts.length >= 8) break;
      if (line.length >= 12 && line.length <= 120) {
        facts.push(line);
      }
    }
    if (facts.length >= 8) break;
  }

  return { summary, keyFacts: facts };
}

async function loadConversationMemory(conversationId: string) {
  try {
    const mem = await invoke<ConversationMemory | null>("get_conversation_memory", { conversationId });
    conversationMemory.value = mem;
  } catch (err) {
    console.error("Failed to load conversation memory:", err);
    conversationMemory.value = null;
  }
}

async function persistConversationMemory(conversationId: string) {
  const { summary, keyFacts } = extractSessionMemory();
  if (!summary.trim()) return;
  try {
    await invoke("upsert_conversation_memory", {
      conversationId,
      summary,
      keyFacts,
    });
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
    const items = await invoke<ConversationMeta[]>("list_conversations");
    conversations.value = items || [];
  } catch (err) {
    console.error("Failed to list conversations:", err);
  }
}

async function createNewConversation(seedTitle?: string): Promise<string | null> {
  try {
    const conv = await invoke<ConversationMeta>("create_conversation", {
      title: seedTitle?.trim() ? buildConversationTitle(seedTitle) : "New chat",
    });
    await refreshConversations();
    return conv.id;
  } catch (err) {
    console.error("Failed to create conversation:", err);
    return null;
  }
}

async function loadConversation(id: string) {
  activeConversationId.value = id;
  planMode.value = false;
  try {
    const saved = await invoke<PersistedMessage[]>("load_history", { conversationId: id });
    messages.value = (saved || [])
      .filter((m) => (m.role === "user" || m.role === "assistant") && !!m.content)
      .map((m) => ({
        role: m.role as "user" | "assistant",
        content: m.content,
        tokenUsage: m.tokenUsage,
        cost: m.cost,
      }));
    await loadConversationMemory(id);
  } catch (err) {
    console.error("Failed to load conversation messages:", err);
    messages.value = [];
  }
}

async function persistMessage(msg: ChatMessage) {
  if (!activeConversationId.value) return;
  try {
    await invoke("append_history", { conversationId: activeConversationId.value, message: msg });
    await refreshConversations();
  } catch (err) {
    console.error("Failed to persist message:", err);
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
        // Tool calls are rendered as a dedicated panel in ChatScreen.
        assistantResponse.value += `\n> Using tool: ${toolName}...\n`;
        chatScreenRef.value?.scrollToBottom();
      } else if (payload.type === "tool-json-delta") {
        const toolId = payload.tool_use_id ?? "";
        if (toolId && payload.tool_use_input) {
          const prev = toolInputById.get(toolId) ?? "";
          toolInputById.set(toolId, prev + payload.tool_use_input);
        }
      } else if (payload.type === "tool-result") {
        if (currentToolStartedAt.value) {
          currentToolDurationMs.value += Math.max(0, Date.now() - currentToolStartedAt.value);
          currentToolStartedAt.value = null;
        }
        const toolId = payload.tool_use_id ?? "";
        const toolName = payload.tool_use_name
          ?? (toolId ? toolNameById.get(toolId) : undefined)
          ?? "unknown";
        const rawInput = toolId ? (toolInputById.get(toolId) ?? "") : "";
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
            planMode.value = planModeChange.mode === "plan";
          }
          const needsUserInput = parseNeedsUserInput(result);
          if (needsUserInput) {
            pendingQuestion.value = needsUserInput;
            isGenerating.value = false;
            const rendered = renderToolResult(result);
            const preview = rendered.length > 1200 ? `${rendered.slice(0, 1200)}\n...(truncated)` : rendered;
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
        if (turnState === "error") {
          isGenerating.value = false;
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
          stopReason === "needs_user_input";

        if (shouldFinalize) {
          finalizeAssistantTurn(payload.token_usage);
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

async function handleSendMessage(userText: string) {
  if (!userText.trim() || isGenerating.value) return;
  pendingQuestion.value = null;

  if (!activeConversationId.value) {
    const id = await createNewConversation(userText);
    if (!id) return;
    activeConversationId.value = id;
    messages.value = [];
  }

  const userMsg: ChatMessage = { role: "user", content: userText };
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
  currentInputTokens.value = estimateInputTokensForTurn(userText);
  toolInputById.clear();
  toolNameById.clear();

  const rustMessages = messages.value.map(msg => ({
    role: msg.role,
    content: msg.content
  }));

  try {
    await invoke("send_chat_message", {
      conversationId: activeConversationId.value || null,
      messages: rustMessages,
      planMode: planMode.value,
    });
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
    isGenerating.value = false;
  }
}

async function handlePendingQuestionSubmit(payload: AskUserAnswerSubmission) {
  await handleSendMessage(buildPendingQuestionReply(payload, "submit"));
}

async function handlePendingQuestionSkip() {
  await handleSendMessage(buildPendingQuestionReply(null, "skip"));
}

async function handleNewChat() {
  const id = await createNewConversation("New chat");
  if (!id) return;
  activeConversationId.value = id;
  messages.value = [];
  assistantResponse.value = "";
  isGenerating.value = false;
  planMode.value = false;
}

async function handleSelectConversation(id: string) {
  if (!id || id === activeConversationId.value || isGenerating.value) return;
  assistantResponse.value = "";
  isGenerating.value = false;
  planMode.value = false;
  await loadConversation(id);
}

async function handleDeleteConversation(id: string) {
  if (!id || isGenerating.value) return;
  try {
    await invoke("delete_conversation", { conversationId: id });
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
        }
      }
    }
  } catch (err) {
    console.error("Failed to delete conversation:", err);
  }
}
</script>

<template>
  <div class="flex h-screen bg-[#fcfcfc] dark:bg-[#1a1a1a] text-[#1a1a1a] dark:text-[#ececec] overflow-hidden font-sans">
    <GlobalToastHost />
    
    <Sidebar
      v-if="isSidebarOpen"
      :recents="conversations"
      :activeConversationId="activeConversationId"
      @new-chat="handleNewChat"
      @select-conversation="handleSelectConversation"
      @delete-conversation="handleDeleteConversation"
      @toggle-sidebar="isSidebarOpen = !isSidebarOpen"
    />

    <!-- Main Content Area -->
    <main class="flex-1 flex flex-col relative h-full">
      <!-- Top Title Bar -->
      <header class="h-14 flex items-center justify-between px-4 absolute top-0 w-full z-10 pointer-events-none">
        <div class="flex items-center gap-2 pointer-events-auto">
          <button @click="isSidebarOpen = !isSidebarOpen" class="w-8 h-8 flex items-center justify-center rounded-md hover:bg-black/5 dark:hover:bg-white/5 text-muted-foreground">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><line x1="9" y1="3" x2="9" y2="21"/></svg>
          </button>
          <div class="flex gap-1 ml-2 text-muted-foreground/40 hidden md:flex">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M19 12H5M12 19l-7-7 7-7"/></svg>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
          </div>
        </div>
      </header>

      <WelcomeScreen 
        v-if="messages.length === 0" 
        :isGenerating="isGenerating"
        @send="handleSendMessage" 
      />

      <ChatScreen 
        v-else 
        ref="chatScreenRef"
        :messages="messages"
        :isGenerating="isGenerating"
        :assistantResponse="assistantResponse"
        :assistantTokenUsage="assistantTokenUsage"
        :assistantTurnCost="assistantTurnCost"
        :pendingQuestion="pendingQuestion"
        :planMode="planMode"
        @send="handleSendMessage"
        @ask-submit="handlePendingQuestionSubmit"
        @ask-skip="handlePendingQuestionSkip"
      />

    </main>
  </div>
</template>

<style>
/* Global reset since App.vue doesn't have styled-scoped anymore */
html, body, #app {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
}
</style>
