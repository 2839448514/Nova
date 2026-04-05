<script setup lang="ts">
import type { ChatMessage } from '../../../lib/chat-types';

defineProps<{
  message: ChatMessage;
  index: number;
  copied: boolean;
  timeText: string;
}>();

const emit = defineEmits<{
  (e: 'retry', index: number): void;
  (e: 'copy', index: number): void;
}>();

const formatFileSize = (bytes?: number) => {
  if (!bytes || !Number.isFinite(bytes) || bytes <= 0) {
    return '';
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const kb = bytes / 1024;
  if (kb < 1024) {
    return `${kb.toFixed(1)} KB`;
  }
  return `${(kb / 1024).toFixed(1)} MB`;
};
</script>

<template>
  <div class="ml-auto max-w-[85%] flex flex-row-reverse gap-2.5 items-start">
    <div class="w-7 h-7 rounded-full flex items-center justify-center shrink-0 bg-[#23211b] text-[#f8f6ef] text-[11px] font-medium mt-0.5">你</div>
    <div class="flex flex-col items-end">
      <div class="flex items-center gap-2 mb-1">
        <p class="text-[11px] text-[#9b958a]">你</p>
        <span v-if="typeof message.tokenUsage === 'number'" class="token-badge">
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
            <ellipse cx="12" cy="5" rx="9" ry="3"></ellipse>
            <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"></path>
            <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"></path>
          </svg>
          本次 {{ message.tokenUsage ?? 0 }}
        </span>
      </div>
      <div class="bg-[#f1eee7] dark:bg-[#2d2d2d] px-4 py-2.5 rounded-xl border border-[#e6e1d6] dark:border-[#3c3c3c]">
        <div v-if="message.attachments?.length" class="mb-2 flex flex-wrap gap-1.5">
          <div
            v-for="(file, i) in message.attachments"
            :key="`${file.sourceName}-${i}`"
            class="inline-flex items-center gap-1.5 rounded-md border border-[#dfd8ca] dark:border-[#4a4a4a] bg-[#fbf8f1] dark:bg-[#353535] px-2 py-1 text-[11px] text-[#5a5245] dark:text-[#d7d0c5]"
          >
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
              <polyline points="14 2 14 8 20 8" />
            </svg>
            <span class="max-w-[180px] truncate" :title="file.sourceName">{{ file.sourceName }}</span>
            <span v-if="formatFileSize(file.size)" class="opacity-70">{{ formatFileSize(file.size) }}</span>
          </div>
        </div>
        <div
          v-if="message.content.trim()"
          class="text-[0.92rem] leading-relaxed whitespace-pre-wrap break-words text-[#23211b] dark:text-[#ececec]"
        >
          {{ message.content }}
        </div>
      </div>
      <div class="msg-toolbar">
        <span class="msg-time">{{ timeText }}</span>
        <button class="msg-icon-btn" aria-label="Retry user message" @click="emit('retry', index)">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/></svg>
        </button>
        <button class="msg-icon-btn" aria-label="Edit message">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
        </button>
        <button class="msg-icon-btn" :class="{ 'is-copied': copied }" aria-label="Copy message" @click="emit('copy', index)">
          <svg v-if="!copied" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
          <svg v-else width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
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
</style>
