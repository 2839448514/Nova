<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import Sidebar from "./components/layout/Sidebar.vue";
import WelcomeScreen from "./components/chat/WelcomeScreen.vue";
import ChatScreen from "./components/chat/ChatScreen.vue";

interface Message {
  role: "user" | "assistant";
  content: string;
  tokenUsage?: number;
}

interface PersistedMessage {
  role: string;
  content: string;
  tokenUsage?: number;
}

interface ConversationMeta {
  id: string;
  title: string;
  updatedAt: number;
}

interface ChatMessageEvent {
  type: string;
  text?: string;
  tool_use_id?: string;
  tool_use_name?: string;
  tool_use_input?: string;
  tool_result?: string;
  token_usage?: number;
}

interface NeedsUserInputPayload {
  type?: string;
  question?: string;
  context?: string;
  options?: string[];
  allow_freeform?: boolean;
}

const messages = ref<Message[]>([]);
const isGenerating = ref(false);
const assistantResponse = ref("");
const assistantTokenUsage = ref<number | undefined>(undefined);
const conversations = ref<ConversationMeta[]>([]);
const activeConversationId = ref("");

let unlisten: UnlistenFn | null = null;
const isSidebarOpen = ref(true);
const chatScreenRef = ref<InstanceType<typeof ChatScreen> | null>(null);

function buildConversationTitle(source: string): string {
  const t = source.trim();
  return t.length > 24 ? `${t.slice(0, 24)}...` : t;
}

function renderToolResult(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return "";

  try {
    const parsed = JSON.parse(trimmed) as NeedsUserInputPayload;
    if (parsed?.type === "needs_user_input") {
      const lines: string[] = [];
      if (parsed.context) {
        lines.push(`需要你补充信息：${parsed.context}`);
      }
      if (parsed.question) {
        lines.push(parsed.question);
      }
      if (Array.isArray(parsed.options) && parsed.options.length > 0) {
        lines.push("");
        lines.push("可选项：");
        for (const opt of parsed.options) {
          lines.push(`- ${opt}`);
        }
      }
      if (parsed.allow_freeform) {
        lines.push("");
        lines.push("也可以直接描述你的具体需求。");
      }
      return lines.join("\n");
    }

    // Generic JSON prettify for non-needs_user_input tool outputs.
    return JSON.stringify(parsed, null, 2);
  } catch {
    return trimmed;
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
  try {
    const saved = await invoke<PersistedMessage[]>("load_history", { conversationId: id });
    messages.value = (saved || [])
      .filter((m) => (m.role === "user" || m.role === "assistant") && !!m.content)
      .map((m) => ({
        role: m.role as "user" | "assistant",
        content: m.content,
        tokenUsage: m.tokenUsage,
      }));
  } catch (err) {
    console.error("Failed to load conversation messages:", err);
    messages.value = [];
  }
}

async function persistMessage(msg: Message) {
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
    unlisten = await listen<ChatMessageEvent>("chat-stream", (event) => {
      const payload = event.payload;
      if (payload.type === "text" && payload.text) {
        assistantResponse.value += payload.text;
        chatScreenRef.value?.scrollToBottom();
      } else if (payload.type === "tool-use-start") {
        assistantResponse.value += `\n> Using tool: ${payload.tool_use_name ?? "unknown"}...\n`;
        chatScreenRef.value?.scrollToBottom();
      } else if (payload.type === "tool-result") {
        const result = (payload.tool_result ?? "").trim();
        if (result) {
          const rendered = renderToolResult(result);
          const preview = rendered.length > 1200 ? `${rendered.slice(0, 1200)}\n...(truncated)` : rendered;
          assistantResponse.value += `\n${preview}\n`;
        }
        chatScreenRef.value?.scrollToBottom();
      } else if (payload.type === "token-usage") {
        assistantTokenUsage.value = payload.token_usage;
      } else if (payload.type === "stop") {
        const finalText = assistantResponse.value.trim();
        const assistantMsg: Message = {
          role: "assistant",
          content: finalText || "（本轮没有返回可显示的文本内容）",
          tokenUsage: payload.token_usage ?? assistantTokenUsage.value,
        };
        messages.value.push(assistantMsg);
        void persistMessage(assistantMsg);
        assistantResponse.value = "";
        assistantTokenUsage.value = undefined;
        isGenerating.value = false;
        chatScreenRef.value?.scrollToBottom();
      }
    });
  } catch (err) {
    console.error("Failed to setup listener:", err);
  }
});

onUnmounted(() => {
  if (unlisten) unlisten();
});

async function handleSendMessage(userText: string) {
  if (!userText.trim() || isGenerating.value) return;

  if (!activeConversationId.value) {
    const id = await createNewConversation(userText);
    if (!id) return;
    activeConversationId.value = id;
    messages.value = [];
  }

  const userMsg: Message = { role: "user", content: userText };
  messages.value.push(userMsg);
  await persistMessage(userMsg);
  isGenerating.value = true;
  assistantResponse.value = "";
  assistantTokenUsage.value = undefined;

  const rustMessages = messages.value.map(msg => ({
    role: msg.role,
    content: msg.content
  }));

  try {
    await invoke("send_chat_message", { messages: rustMessages });
    if (isGenerating.value) {
      const finalText = assistantResponse.value.trim();
      const assistantMsg: Message = {
        role: "assistant",
        content: finalText || "（本轮没有返回可显示的文本内容）",
        tokenUsage: assistantTokenUsage.value,
      };
      messages.value.push(assistantMsg);
      await persistMessage(assistantMsg);
      assistantResponse.value = "";
      assistantTokenUsage.value = undefined;
      isGenerating.value = false;
    }
  } catch (err: any) {
    console.error("Chat error:", err);
    const errorMsg: Message = { role: "assistant", content: `API Error: ${err}` };
    messages.value.push(errorMsg);
    await persistMessage(errorMsg);
    assistantResponse.value = "";
    assistantTokenUsage.value = undefined;
    isGenerating.value = false;
  }
}

async function handleNewChat() {
  const id = await createNewConversation("New chat");
  if (!id) return;
  activeConversationId.value = id;
  messages.value = [];
  assistantResponse.value = "";
  isGenerating.value = false;
}

async function handleSelectConversation(id: string) {
  if (!id || id === activeConversationId.value || isGenerating.value) return;
  assistantResponse.value = "";
  isGenerating.value = false;
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
        @send="handleSendMessage" 
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