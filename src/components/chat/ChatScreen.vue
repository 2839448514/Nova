<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type {
  AgentMode,
  AskUserAnswerSubmission,
  ChatMessage,
  NeedsUserInputPayload,
  PendingUploadFile,
  TurnCost,
} from '../../lib/chat-types';
import InputArea from '../layout/InputArea.vue';
import AskUserInputDialog from './AskUserInputDialog.vue';
import AssistantMessageBubble from './messages/AssistantMessageBubble.vue';
import MarkdownRenderer from './MarkdownRenderer.vue';
import ToolLogPanel from './messages/ToolLogPanel.vue';
import UserMessageBubble from './messages/UserMessageBubble.vue';

const props = defineProps<{
  messages: ChatMessage[];
  isGenerating: boolean;
  assistantResponse: string;
  assistantReasoning?: string;
  assistantTokenUsage?: number;
  assistantTurnCost?: TurnCost;
  pendingQuestion?: NeedsUserInputPayload | null;
  planMode?: boolean;
  agentMode?: AgentMode;
  pendingUploads?: PendingUploadFile[];
}>();

const emit = defineEmits<{
  (e: 'send', msg: string): void;
  (e: 'ask-submit', value: AskUserAnswerSubmission): void;
  (e: 'ask-skip'): void;
  (e: 'cancel'): void;
  (e: 'mode-change', mode: AgentMode): void;
  (e: 'upload-files', files: PendingUploadFile[]): void;
  (e: 'remove-upload', index: number): void;
}>();

const chatAreaRef = ref<HTMLElement | null>(null);
const reactionMap = ref<Record<number, 'up' | 'down' | undefined>>({});
const copiedMap = ref<Record<string, boolean>>({});
const copyTimers: Record<string, ReturnType<typeof setTimeout> | undefined> = {};

// ── Coding workspace ──────────────────────────────────────────────────────────
const codingWorkspace = ref<string | null>(null);

async function loadCodingWorkspace() {
  try {
    const settings = await invoke<{ codingWorkspace?: string }>('get_settings');
    codingWorkspace.value = settings.codingWorkspace ?? null;
  } catch { /* ignore */ }
}

async function pickWorkspace() {
  try {
    const picked = await invoke<string | null>('pick_coding_workspace');
    if (picked) {
      codingWorkspace.value = picked;
      const settings = await invoke<Record<string, unknown>>('get_settings');
      settings.codingWorkspace = picked;
      await invoke('save_settings', { settings });
    }
  } catch { /* ignore */ }
}

watch(() => props.agentMode, (mode) => {
  if (mode === 'coding') loadCodingWorkspace();
}, { immediate: true });

const toolStartPattern = /^(?:>\s*)?Using tool:\s*(.+?)\.{0,3}\s*$/i;
const toolInfoPattern = /^(?:>\s*)?Tool info:\s*(.+?)\s*$/i;
const toolDonePattern = /^(?:>\s*)?Tool done:\s*(.+?)\s*$/i;

const formatNowTime = () => {
  const now = new Date();
  const hh = String(now.getHours()).padStart(2, '0');
  const mm = String(now.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
};

const copyText = async (text: string, key: string) => {
  if (!text?.trim()) return;
  try {
    await navigator.clipboard.writeText(text);
    copiedMap.value[key] = true;
    if (copyTimers[key]) {
      clearTimeout(copyTimers[key]);
    }
    copyTimers[key] = setTimeout(() => {
      copiedMap.value[key] = false;
    }, 900);
  } catch {
    // Ignore clipboard failures silently to keep UI interaction smooth.
  }
};

const setReaction = (index: number, value: 'up' | 'down') => {
  reactionMap.value[index] = reactionMap.value[index] === value ? undefined : value;
};

const retryFromUser = (index: number) => {
  const text = props.messages[index]?.content?.trim();
  if (!text) return;
  emit('send', text);
  scrollToBottom();
};

const retryFromAssistant = (assistantIndex: number) => {
  const prev = [...props.messages.slice(0, assistantIndex)].reverse().find((m) => m.role === 'user');
  if (!prev?.content?.trim()) return;
  emit('send', prev.content);
  scrollToBottom();
};

const buildAssistantCopyText = (message: ChatMessage) => {
  const sections = [];
  if (message.reasoning?.trim()) {
    sections.push(`AI 思考过程\n${message.reasoning.trim()}`);
  }
  if (message.content?.trim()) {
    sections.push(message.content.trim());
  }
  return sections.join('\n\n');
};

const scrollToBottom = async () => {
  await nextTick();
  if (chatAreaRef.value) {
    chatAreaRef.value.scrollTop = chatAreaRef.value.scrollHeight;
  }
};

const scrollLastUserMessageToTop = async () => {
  await nextTick();
  if (!chatAreaRef.value) return;
  const rows = chatAreaRef.value.querySelectorAll<HTMLElement>('[data-role="user"]');
  const last = rows[rows.length - 1];
  if (last) {
    last.scrollIntoView({ block: 'start', behavior: 'smooth' });
  } else {
    chatAreaRef.value.scrollTop = chatAreaRef.value.scrollHeight;
  }
};

onMounted(() => {
  scrollToBottom();
});

const handleSend = (msg: string) => {
  emit('send', msg);
  scrollLastUserMessageToTop();
};

const handleUploadFiles = (files: PendingUploadFile[]) => {
  emit('upload-files', files);
};

const handleRemoveUpload = (index: number) => {
  emit('remove-upload', index);
};

const extractToolLog = (content: string): string[] => {
  if (!content) return [];
  const items: string[] = [];
  let pendingIndex = -1;
  for (const rawLine of content.split('\n')) {
    const line = rawLine.trim();
    const start = line.match(toolStartPattern);
    if (start) {
      items.push(`Using tool: ${start[1]}`);
      pendingIndex = items.length - 1;
      continue;
    }

    const info = line.match(toolInfoPattern);
    if (info) {
      if (pendingIndex >= 0) {
        items[pendingIndex] = `${items[pendingIndex]} | ${info[1]}`;
      } else {
        items.push(`Tool info: ${info[1]}`);
      }
      continue;
    }

    const done = line.match(toolDonePattern);
    if (done) {
      if (pendingIndex >= 0) {
        items[pendingIndex] = `${items[pendingIndex]} | done`;
        pendingIndex = -1;
      } else {
        items.push(`Tool done: ${done[1]}`);
      }
    }
  }
  return items;
};

const stripToolLog = (content: string): string => {
  if (!content) return '';
  const lines = content
    .split('\n')
    .filter((line) => {
      const t = line.trim();
      return !toolStartPattern.test(t) && !toolInfoPattern.test(t) && !toolDonePattern.test(t);
    });
  return lines.join('\n').replace(/\n{3,}/g, '\n\n').trim();
};

const conversationTokenUsage = (index: number): number => {
  return props.messages.slice(0, index + 1).reduce((sum, m) => sum + (m.tokenUsage ?? 0), 0);
};

const estimateTokensFromContent = (content: string): number => {
  const normalized = content.replace(/\s+/g, ' ').trim();
  if (!normalized) return 0;
  return Math.max(1, Math.ceil(normalized.length / 4));
};

const streamingTokenUsage = (): number => {
  if (typeof props.assistantTokenUsage === 'number' && props.assistantTokenUsage > 0) {
    return props.assistantTokenUsage;
  }
  if (!props.isGenerating) {
    return 0;
  }
  return estimateTokensFromContent(stripToolLog(props.assistantResponse));
};

const streamingConversationTokenUsage = (): number => {
  const base = props.messages.reduce((sum, m) => sum + (m.tokenUsage ?? 0), 0);
  return base + streamingTokenUsage();
};

const hasStreamingReasoning = () => !!props.assistantReasoning?.trim();

defineExpose({
  scrollToBottom,
  scrollLastUserMessageToTop,
});
</script>

<template>
  <div class="flex flex-col h-full w-full max-w-4xl mx-auto pt-14">
    <div class="flex-1 overflow-y-auto px-4 pb-4 custom-scrollbar" ref="chatAreaRef">
      <div class="w-full flex flex-col gap-6">
        <div
          v-for="(msg, index) in messages"
          :key="index"
          :data-role="msg.role"
          class="flex w-full group"
        >
          <UserMessageBubble
            v-if="msg.role === 'user'"
            :message="msg"
            :index="index"
            :copied="!!copiedMap[`user-${index}`]"
            :timeText="formatNowTime()"
            @retry="retryFromUser"
            @copy="copyText(msg.content, `user-${index}`)"
          />

          <AssistantMessageBubble
            v-else
            :message="{ ...msg, content: stripToolLog(msg.content) }"
            :index="index"
            :copied="!!copiedMap[`assistant-${index}`]"
            :conversationTokenUsage="conversationTokenUsage(index)"
            :toolLogs="extractToolLog(msg.content)"
            @copy="copyText(buildAssistantCopyText(msg), `assistant-${index}`)"
            @retry="retryFromAssistant"
            @react="setReaction($event.index, $event.value)"
          />
        </div>

        <div v-if="isGenerating" class="flex w-full justify-start group">
          <div class="flex gap-3.5 w-full max-w-[85%]">
            <div class="w-7 h-7 rounded-full flex items-center justify-center shrink-0 bg-[#f6f3ec] dark:bg-[#333] text-[#6f685a] mt-0.5 border border-[#e7e2d7] dark:border-[#444] text-[11px] font-medium">
              N
            </div>
            <div class="min-w-0 flex-1 text-[0.95rem] leading-relaxed break-words text-[#1a1a1a] dark:text-[#ececec]">
              <div class="flex items-center gap-2 mb-1">
                <p class="text-[11px] text-[#9b958a]">Nova</p>
                <span
                  v-if="streamingTokenUsage() > 0 || streamingConversationTokenUsage() > 0"
                  class="token-badge"
                >
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                    <ellipse cx="12" cy="5" rx="9" ry="3"></ellipse>
                    <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"></path>
                    <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"></path>
                  </svg>
                  本次 {{ streamingTokenUsage() }} · 会话 {{ streamingConversationTokenUsage() }}
                </span>
              </div>
              <details
                v-if="hasStreamingReasoning()"
                class="reasoning-panel mt-2"
                open
              >
                <summary>AI 思考过程</summary>
                <MarkdownRenderer :content="props.assistantReasoning || ''" />
              </details>
              <MarkdownRenderer :content="stripToolLog(assistantResponse)" />
              <ToolLogPanel :items="extractToolLog(assistantResponse)" />
              <span class="inline-block w-1.5 h-[1em] bg-current ml-1 align-middle animate-pulse opacity-70"></span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div class="w-full bg-transparent px-4 pt-4 pb-6">
      <div class="w-full max-w-[760px] mx-auto">

        <!-- Coding workspace bar -->
        <template v-if="props.agentMode === 'coding'">
          <!-- No workspace -->
          <div
            v-if="!codingWorkspace"
            class="mb-3 flex items-center justify-between gap-3 px-4 py-3 rounded-xl border border-[#e7e2d7] dark:border-[#333] bg-[#faf9f6] dark:bg-[#1e1e1e]"
          >
            <div class="min-w-0">
              <p class="text-sm font-medium text-[#1a1a1a] dark:text-[#ececec] leading-tight">当前工作目录缺失</p>
              <p class="text-xs text-muted-foreground mt-0.5">此对话的工作目录已不存在</p>
            </div>
            <button
              class="shrink-0 text-[11px] px-3 py-1 rounded-lg border border-[#e7e2d7] dark:border-[#444] text-muted-foreground hover:bg-black/5 dark:hover:bg-white/5 transition-colors whitespace-nowrap"
              @click="pickWorkspace"
            >选择工作区</button>
          </div>

          <!-- Has workspace -->
          <div
            v-else
            class="mb-3 flex items-center gap-2 px-4 py-2.5 rounded-xl border border-[#e7e2d7] dark:border-[#333] bg-[#faf9f6] dark:bg-[#1e1e1e]"
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="shrink-0 text-muted-foreground">
              <path d="M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z"/>
            </svg>
            <span class="text-xs font-medium text-[#1a1a1a] dark:text-[#ececec] break-all flex-1 min-w-0">{{ codingWorkspace }}</span>
            <button
              class="shrink-0 text-[11px] px-2 py-0.5 rounded border border-[#e7e2d7] dark:border-[#333] text-muted-foreground hover:bg-black/5 dark:hover:bg-white/5 transition-colors"
              @click="pickWorkspace"
            >选择工作区</button>
          </div>
        </template>

        <AskUserInputDialog
          v-if="pendingQuestion"
          :request="pendingQuestion"
          @submit="emit('ask-submit', $event)"
          @skip="emit('ask-skip')"
        />
        <InputArea
          v-else
          :isGenerating="isGenerating"
          :agentMode="agentMode"
          :pendingUploads="pendingUploads"
          @send="handleSend"
          @cancel="emit('cancel')"
          @mode-change="emit('mode-change', $event)"
          @upload-files="handleUploadFiles"
          @remove-upload="handleRemoveUpload"
        />
      </div>
      <div class="text-center text-[0.7rem] text-muted-foreground mt-2">
        Nova can make mistakes. Please verify important information.
      </div>
    </div>
  </div>
</template>

<style scoped>
.custom-scrollbar::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}

.custom-scrollbar::-webkit-scrollbar-track {
  background: transparent;
}

.custom-scrollbar::-webkit-scrollbar-thumb {
  background-color: var(--color-border, #e5e5e5);
  border-radius: 10px;
}

.dark .custom-scrollbar::-webkit-scrollbar-thumb {
  background-color: #444;
}

.token-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 9px;
  color: #A39E93;
  border: 1px solid rgba(229, 225, 213, 0.6);
  background: rgba(229, 225, 213, 0.2);
  padding: 3px 6px;
  border-radius: 6px;
  font-family: monospace;
  letter-spacing: 0.04em;
  font-variant-numeric: tabular-nums;
}

.dark .token-badge {
  color: #a09e99;
  border-color: #5a5549;
  background: rgba(60, 56, 48, 0.45);
}

.reasoning-panel {
  margin-bottom: 10px;
  border: 1px solid rgba(225, 218, 204, 0.9);
  background: rgba(249, 246, 239, 0.85);
  border-radius: 10px;
  padding: 8px 10px;
}

.reasoning-panel summary {
  cursor: pointer;
  font-size: 11px;
  color: #8a8478;
  user-select: none;
  list-style: none;
}

.reasoning-panel summary::-webkit-details-marker {
  display: none;
}

.reasoning-panel summary::before {
  content: "▸";
  display: inline-block;
  margin-right: 6px;
  transition: transform 0.15s ease;
}

.reasoning-panel[open] summary::before {
  transform: rotate(90deg);
}

.reasoning-panel :deep(.markdown-body) {
  margin-top: 8px;
}

.dark .reasoning-panel {
  border-color: #4a443a;
  background: rgba(41, 38, 33, 0.92);
}

.dark .reasoning-panel summary {
  color: #b1ab9f;
}

</style>
