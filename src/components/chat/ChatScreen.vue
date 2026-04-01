<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue';
import InputArea from '../layout/InputArea.vue';
import MarkdownRenderer from './MarkdownRenderer.vue';

interface Message {
  role: "user" | "assistant";
  content: string;
  tokenUsage?: number;
  cost?: TurnCost;
}

interface TurnCost {
  inputTokens: number;
  outputTokens: number;
  toolCalls: number;
  toolDurationMs: number;
}

interface PendingQuestion {
  question?: string;
  context?: string;
  options?: string[];
  allow_freeform?: boolean;
}

const props = defineProps<{
  messages: Message[];
  isGenerating: boolean;
  assistantResponse: string;
  assistantTokenUsage?: number;
  assistantTurnCost?: TurnCost;
  pendingQuestion?: PendingQuestion | null;
}>();

const emit = defineEmits<{
  (e: 'send', msg: string): void;
}>();

const chatAreaRef = ref<HTMLElement | null>(null);
const reactionMap = ref<Record<number, 'up' | 'down' | undefined>>({});
const copiedMap = ref<Record<string, boolean>>({});
const copyTimers: Record<string, ReturnType<typeof setTimeout> | undefined> = {};

const toolLogPattern = /^(?:>\s*)?Using tool:\s*(.+?)\.{0,3}\s*$/i;

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

const scrollToBottom = async () => {
  await nextTick();
  if (chatAreaRef.value) {
    chatAreaRef.value.scrollTop = chatAreaRef.value.scrollHeight;
  }
};

onMounted(() => {
  scrollToBottom();
});

const handleSend = (msg: string) => {
  emit('send', msg);
  scrollToBottom();
};

const extractToolLog = (content: string): string[] => {
  if (!content) return [];
  const items: string[] = [];
  for (const rawLine of content.split('\n')) {
    const line = rawLine.trim();
    const m = line.match(toolLogPattern);
    if (!m) continue;
    items.push(`Using tool: ${m[1]}`);
  }
  return items;
};

const stripToolLog = (content: string): string => {
  if (!content) return '';
  const lines = content
    .split('\n')
    .filter((line) => !toolLogPattern.test(line.trim()));
  return lines.join('\n').replace(/\n{3,}/g, '\n\n').trim();
};

const conversationTokenUsage = (index: number): number => {
  return props.messages.slice(0, index + 1).reduce((sum, m) => sum + (m.tokenUsage ?? 0), 0);
};

const streamingConversationTokenUsage = (): number => {
  const base = props.messages.reduce((sum, m) => sum + (m.tokenUsage ?? 0), 0);
  return base + (props.assistantTokenUsage ?? 0);
};

defineExpose({
  scrollToBottom
});

</script>

<template>
  <div class="flex flex-col h-full w-full max-w-4xl mx-auto pt-14">
    <!-- Messages Area -->
    <div class="flex-1 overflow-y-auto px-4 pb-4 custom-scrollbar" ref="chatAreaRef">
      <div class="w-full flex flex-col gap-6">
        <div 
          v-for="(msg, index) in messages" 
          :key="index" 
          class="flex w-full group"
        >
          <div v-if="msg.role === 'user'" class="ml-auto max-w-[85%] flex flex-row-reverse gap-2.5 items-start">
            <div class="w-7 h-7 rounded-full flex items-center justify-center shrink-0 bg-[#23211b] text-[#f8f6ef] text-[11px] font-medium mt-0.5">你</div>
            <div class="flex flex-col items-end">
              <div class="flex items-center gap-2 mb-1">
                <p class="text-[11px] text-[#9b958a]">你</p>
                <span v-if="typeof msg.tokenUsage === 'number'" class="token-badge">
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                    <ellipse cx="12" cy="5" rx="9" ry="3"></ellipse>
                    <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"></path>
                    <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"></path>
                  </svg>
                  本次 {{ msg.tokenUsage ?? 0 }}
                </span>
              </div>
              <div class="bg-[#f1eee7] dark:bg-[#2d2d2d] px-4 py-2.5 rounded-xl border border-[#e6e1d6] dark:border-[#3c3c3c]">
                <div class="text-[0.92rem] leading-relaxed whitespace-pre-wrap break-words text-[#23211b] dark:text-[#ececec]">
                  {{ msg.content }}
                </div>
              </div>
              <div class="msg-toolbar">
                <span class="msg-time">{{ formatNowTime() }}</span>
                <button class="msg-icon-btn" aria-label="Retry user message" @click="retryFromUser(index)">
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/></svg>
                </button>
                <button class="msg-icon-btn" aria-label="Edit message">
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
                </button>
                <button class="msg-icon-btn" :class="{ 'is-copied': copiedMap[`user-${index}`] }" aria-label="Copy message" @click="copyText(msg.content, `user-${index}`)">
                  <svg v-if="!copiedMap[`user-${index}`]" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
                  <svg v-else width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
                </button>
              </div>
            </div>
          </div>

          <div v-else class="flex gap-3.5 max-w-[85%]">
            <div class="w-7 h-7 rounded-full flex items-center justify-center shrink-0 bg-[#f6f3ec] dark:bg-[#333] text-[#6f685a] mt-0.5 border border-[#e7e2d7] dark:border-[#444] text-[11px] font-medium">
              N
            </div>
            <div class="text-[0.95rem] leading-relaxed break-words text-[#1a1a1a] dark:text-[#ececec]">
              <div class="flex items-center gap-2 mb-1">
                <p class="text-[11px] text-[#9b958a]">Nova</p>
                <span
                  v-if="typeof msg.tokenUsage === 'number'"
                  class="token-badge"
                >
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                    <ellipse cx="12" cy="5" rx="9" ry="3"></ellipse>
                    <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"></path>
                    <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"></path>
                  </svg>
                  本次 {{ msg.tokenUsage ?? 0 }} · 会话 {{ conversationTokenUsage(index) }}
                </span>
              </div>
              <MarkdownRenderer :content="stripToolLog(msg.content)" />
              <div v-if="msg.role === 'assistant' && extractToolLog(msg.content).length > 0" class="tool-log-panel">
                <div class="tool-log-title">工具调用</div>
                <div
                  v-for="(item, toolIndex) in extractToolLog(msg.content)"
                  :key="`tool-${index}-${toolIndex}`"
                  class="tool-log-item"
                >
                  {{ item }}
                </div>
              </div>
              <div v-if="msg.role === 'assistant' && msg.cost" class="cost-panel">
                <span>in {{ msg.cost.inputTokens }}</span>
                <span>out {{ msg.cost.outputTokens }}</span>
                <span>tools {{ msg.cost.toolCalls }}</span>
                <span>tool ms {{ msg.cost.toolDurationMs }}</span>
              </div>
              <div class="msg-toolbar">
                <button class="msg-icon-btn" :class="{ 'is-copied': copiedMap[`assistant-${index}`] }" aria-label="Copy assistant message" @click="copyText(msg.content, `assistant-${index}`)">
                  <svg v-if="!copiedMap[`assistant-${index}`]" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
                  <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
                </button>
                <button
                  class="msg-icon-btn"
                  :class="{ 'is-active': reactionMap[index] === 'up' }"
                  aria-label="Thumbs up"
                  @click="setReaction(index, 'up')"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M14 9V5a3 3 0 0 0-3-3l-4 9v11h11.28a2 2 0 0 0 2-1.7l1.38-9a2 2 0 0 0-2-2.3H14z"/><path d="M7 22H4a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2h3"/></svg>
                </button>
                <button
                  class="msg-icon-btn"
                  :class="{ 'is-active-down': reactionMap[index] === 'down' }"
                  aria-label="Thumbs down"
                  @click="setReaction(index, 'down')"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M10 15v4a3 3 0 0 0 3 3l4-9V2H5.72a2 2 0 0 0-2 1.7l-1.38 9a2 2 0 0 0 2 2.3H10z"/><path d="M17 2h2.67A2.31 2.31 0 0 1 22 4v7a2.31 2.31 0 0 1-2.33 2H17"/></svg>
                </button>
                <button class="msg-icon-btn" aria-label="Retry" @click="retryFromAssistant(index)">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/></svg>
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- Streaming -->
        <div v-if="isGenerating" class="flex w-full justify-start group">
          <div class="flex gap-3.5 max-w-[85%]">
            <div class="w-7 h-7 rounded-full flex items-center justify-center shrink-0 bg-[#f6f3ec] dark:bg-[#333] text-[#6f685a] mt-0.5 border border-[#e7e2d7] dark:border-[#444] text-[11px] font-medium">
              N
            </div>
            <div class="text-[0.95rem] leading-relaxed break-words text-[#1a1a1a] dark:text-[#ececec]">
              <div class="flex items-center gap-2 mb-1">
                <p class="text-[11px] text-[#9b958a]">Nova</p>
                <span v-if="typeof assistantTokenUsage === 'number'" class="token-badge">
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                    <ellipse cx="12" cy="5" rx="9" ry="3"></ellipse>
                    <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"></path>
                    <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"></path>
                  </svg>
                  本次 {{ assistantTokenUsage ?? 0 }} · 会话 {{ streamingConversationTokenUsage() }}
                </span>
              </div>
              <MarkdownRenderer :content="stripToolLog(assistantResponse)" />
              <div v-if="extractToolLog(assistantResponse).length > 0" class="tool-log-panel">
                <div class="tool-log-title">工具调用</div>
                <div
                  v-for="(item, toolIndex) in extractToolLog(assistantResponse)"
                  :key="`stream-tool-${toolIndex}`"
                  class="tool-log-item"
                >
                  {{ item }}
                </div>
              </div>
              <div v-if="assistantTurnCost" class="cost-panel">
                <span>in {{ assistantTurnCost.inputTokens }}</span>
                <span>out {{ assistantTurnCost.outputTokens }}</span>
                <span>tools {{ assistantTurnCost.toolCalls }}</span>
                <span>tool ms {{ assistantTurnCost.toolDurationMs }}</span>
              </div>
              <span class="inline-block w-1.5 h-[1em] bg-current ml-1 align-middle animate-pulse opacity-70"></span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Input Box (Chat state) -->
    <div class="p-4 w-full bg-gradient-to-t from-[#fcfcfc] dark:from-[#1a1a1a] pb-6">
      <InputArea :isGenerating="isGenerating" :pendingQuestion="pendingQuestion" @send="handleSend" />
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

.msg-toolbar {
  display: flex;
  align-items: center;
  gap: 1px;
  margin-top: 4px;
  padding: 0 1px;
}

.msg-time {
  font-size: 11px;
  color: #bbb6ae;
  margin-right: 4px;
  font-variant-numeric: tabular-nums;
}

.msg-icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  border-radius: 5px;
  color: #bbb6ae;
  cursor: pointer;
  transition: color 0.15s, background 0.15s;
}

.msg-icon-btn:hover {
  color: #6b6456;
  background: #f0ede7;
}

.msg-icon-btn.is-copied {
  color: #4a7c59;
}

.msg-icon-btn.is-active {
  color: #2a6496;
}

.msg-icon-btn.is-active-down {
  color: #b03a2e;
}

.token-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 9px;
  color: #a39e93;
  border: 1px solid rgba(229, 225, 213, 0.6);
  background: rgba(229, 225, 213, 0.2);
  padding: 3px 6px;
  border-radius: 6px;
  font-family: 'SF Mono', 'Fira Code', 'Cascadia Mono', monospace;
  letter-spacing: 0.03em;
  font-variant-numeric: tabular-nums;
}

.dark .token-badge {
  color: #a09e99;
  border-color: #5a5549;
  background: rgba(60, 56, 48, 0.45);
}

.tool-log-panel {
  width: 100%;
  margin-top: 10px;
  padding: 10px 12px;
  border-radius: 10px;
  border: 1px solid #ebe7dd;
  background: #f8f6ef;
}

.tool-log-title {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.04em;
  color: #7d7667;
  margin-bottom: 6px;
}

.tool-log-item {
  font-size: 12px;
  line-height: 1.6;
  color: #5e584c;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: 'SF Mono', 'Fira Code', monospace;
}

.cost-panel {
  display: inline-flex;
  gap: 8px;
  margin-top: 6px;
  padding: 4px 8px;
  border-radius: 999px;
  background: #f6f3ec;
  border: 1px solid #e3ddd2;
  font-size: 10px;
  color: #8e877a;
  font-variant-numeric: tabular-nums;
}

.dark .cost-panel {
  color: #a09e99;
  border-color: #464646;
  background: #2f2f2f;
}
</style>