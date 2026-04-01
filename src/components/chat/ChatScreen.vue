<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue';
import InputArea from '../layout/InputArea.vue';
import MarkdownRenderer from './MarkdownRenderer.vue';

interface Message {
  role: "user" | "assistant";
  content: string;
  tokenUsage?: number;
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
  pendingQuestion?: PendingQuestion | null;
}>();

const emit = defineEmits<{
  (e: 'send', msg: string): void;
}>();

const chatAreaRef = ref<HTMLElement | null>(null);

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
          <div v-if="msg.role === 'user'" class="ml-auto bg-[#f4f4f4] dark:bg-[#2d2d2d] px-5 py-3.5 rounded-2xl rounded-tr-sm max-w-[80%] shadow-sm">
            <div class="text-[0.95rem] leading-relaxed whitespace-pre-wrap break-words text-[#1a1a1a] dark:text-[#ececec]">
              {{ msg.content }}
            </div>
          </div>

          <div v-else class="flex gap-4 max-w-[85%]">
            <div class="w-8 h-8 rounded-[10px] flex items-center justify-center shrink-0 bg-[#f0f0eb] dark:bg-[#333] text-[#da7756] mt-0.5 border border-[#e5e5e5] dark:border-[#444] shadow-sm">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/></svg>
            </div>
            <div class="text-[0.95rem] leading-relaxed break-words text-[#1a1a1a] dark:text-[#ececec]">
              <div v-if="typeof msg.tokenUsage === 'number'" class="mb-1 text-[11px] text-[#8a8478] dark:text-[#a09e99] font-mono">
                {{ msg.tokenUsage }} tokens
              </div>
              <MarkdownRenderer :content="msg.content" />
            </div>
          </div>
        </div>

        <!-- Streaming -->
        <div v-if="isGenerating" class="flex w-full justify-start group">
          <div class="flex gap-4 max-w-[85%]">
            <div class="w-8 h-8 rounded-[10px] flex items-center justify-center shrink-0 bg-[#f0f0eb] dark:bg-[#333] text-[#da7756] mt-0.5 border border-[#e5e5e5] dark:border-[#444] shadow-sm">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/></svg>
            </div>
            <div class="text-[0.95rem] leading-relaxed break-words text-[#1a1a1a] dark:text-[#ececec]">
              <div v-if="typeof assistantTokenUsage === 'number'" class="mb-1 text-[11px] text-[#8a8478] dark:text-[#a09e99] font-mono">
                {{ assistantTokenUsage }} tokens
              </div>
              <MarkdownRenderer :content="assistantResponse" />
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
</style>