import { invoke } from "@tauri-apps/api/core";
import type {
  AgentMode,
  ChatMessage,
  ConversationMemory,
  ConversationMeta,
  PersistedMessage,
} from "../../../lib/chat-types";
import type { PermissionActionName } from "../../../lib/chat-payloads";
import { buildConversationTitle } from "../utils/session-memory";

type ChatRequestMessage = Pick<ChatMessage, "role" | "content">;

export type RagUploadDocumentInput = {
  sourceName: string;
  sourceType: string;
  mimeType?: string;
  content: string;
};

export type RagRejectedItem = {
  sourceName: string;
  reason: string;
};

export type RagUpsertResult = {
  added: number;
  updated: number;
  rejected: RagRejectedItem[];
  totalDocuments: number;
  totalChars: number;
};

export type RagDocumentMeta = {
  id: string;
  sourceName: string;
  sourceType: string;
  mimeType?: string;
  contentChars: number;
  preview: string;
  checksum: string;
  createdAt: number;
  updatedAt: number;
};

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
  messages: ChatRequestMessage[],
  planMode: boolean,
  agentMode: AgentMode,
): Promise<void> {
  await invoke("send_chat_message", {
    conversationId,
    messages,
    planMode,
    agentMode,
  });
}

export async function upsertConversationRagDocuments(
  conversationId: string,
  documents: RagUploadDocumentInput[],
): Promise<RagUpsertResult> {
  return invoke<RagUpsertResult>("rag_upsert_conversation_documents", {
    conversationId,
    documents,
  });
}

export async function listConversationRagDocuments(
  conversationId: string,
): Promise<RagDocumentMeta[]> {
  return invoke<RagDocumentMeta[]>("rag_list_conversation_documents", {
    conversationId,
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
  action: PermissionActionName,
): Promise<boolean> {
  return invoke<boolean>("submit_permission_decision", {
    conversationId,
    requestId,
    action,
  });
}
