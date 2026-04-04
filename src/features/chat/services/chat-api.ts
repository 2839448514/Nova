import { invoke } from "@tauri-apps/api/core";
import type {
  ChatMessage,
  ConversationMemory,
  ConversationMeta,
  PersistedMessage,
} from "../../../lib/chat-types";
import { buildConversationTitle } from "../utils/session-memory";

export async function listConversations(): Promise<ConversationMeta[]> {
  const items = await invoke<ConversationMeta[]>("list_conversations");
  return items || [];
}

export async function createConversation(seedTitle?: string): Promise<ConversationMeta> {
  return invoke<ConversationMeta>("create_conversation", {
    title: seedTitle?.trim() ? buildConversationTitle(seedTitle) : "New chat",
  });
}

export async function loadConversationHistory(conversationId: string): Promise<PersistedMessage[]> {
  const saved = await invoke<PersistedMessage[]>("load_history", { conversationId });
  return saved || [];
}

export async function appendConversationMessage(
  conversationId: string,
  message: ChatMessage,
): Promise<void> {
  await invoke("append_history", { conversationId, message });
}

export async function getConversationMemory(
  conversationId: string,
): Promise<ConversationMemory | null> {
  return invoke<ConversationMemory | null>("get_conversation_memory", { conversationId });
}

export async function upsertConversationMemory(
  conversationId: string,
  summary: string,
  keyFacts: string[],
): Promise<void> {
  await invoke("upsert_conversation_memory", {
    conversationId,
    summary,
    keyFacts,
  });
}

export async function deleteConversation(conversationId: string): Promise<void> {
  await invoke("delete_conversation", { conversationId });
}

export async function sendChatMessage(
  conversationId: string | null,
  messages: Array<{ role: string; content: string }>,
  planMode: boolean,
): Promise<void> {
  await invoke("send_chat_message", {
    conversationId,
    messages,
    planMode,
  });
}

export async function cancelChatMessage(conversationId: string | null): Promise<boolean> {
  return invoke<boolean>("cancel_chat_message", {
    conversationId,
  });
}

export async function submitPermissionDecision(
  conversationId: string | null,
  requestId: string,
  action: string,
): Promise<boolean> {
  return invoke<boolean>("submit_permission_decision", {
    conversationId,
    requestId,
    action,
  });
}
