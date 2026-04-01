<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue';

interface PendingQuestion {
  question?: string;
  context?: string;
  options?: string[];
  allow_freeform?: boolean;
}

const props = defineProps<{
  isGenerating?: boolean;
  pendingQuestion?: PendingQuestion | null;
}>();

const emit = defineEmits<{
  (e: 'send', msg: string): void;
}>();

const currentInput = ref("");
const textareaRef = ref<HTMLTextAreaElement | null>(null);
const dismissedQuestionText = ref<string | null>(null);

const focusTextarea = () => {
  textareaRef.value?.focus();
};

const autoResize = () => {
  const el = textareaRef.value;
  if (!el) return;
  el.style.height = 'auto'; 
  const newHeight = Math.min(el.scrollHeight, 200); 
  el.style.height = `${newHeight}px`;
};

const sendMessage = (e?: KeyboardEvent) => {
  if (e && e.shiftKey) return;
  e?.preventDefault();
  if (!currentInput.value.trim() || props.isGenerating) return;

  const message = currentInput.value.trim();
  emit('send', message);
  currentInput.value = "";
  nextTick(() => {
    autoResize();
    focusTextarea();
  });
};

const chooseOption = (option: string) => {
  if (!option || props.isGenerating) return;
  emit('send', option);
  currentInput.value = "";
  nextTick(() => {
    autoResize();
    focusTextarea();
  });
};

const dismissQuestion = () => {
  dismissedQuestionText.value = props.pendingQuestion?.question?.trim() || null;
};

const isQuestionVisible = () => {
  const q = props.pendingQuestion?.question?.trim();
  if (!q) return false;
  return dismissedQuestionText.value !== q;
};

watch(
  () => props.isGenerating,
  () => {
    nextTick(() => {
      autoResize();
      focusTextarea();
    });
  }
);

onMounted(() => {
  nextTick(() => {
    autoResize();
    focusTextarea();
  });
});

defineExpose({
  focusTextarea,
});
</script>

<template>
  <div class="w-full">
    <div
      v-if="isQuestionVisible()"
      class="mb-3 bg-[#f8f6f1] dark:bg-[#272522] border border-[#e9e3d8] dark:border-[#3f3a33] rounded-2xl px-3 py-3"
    >
      <div class="flex items-start justify-between gap-3 mb-2">
        <div class="text-[0.95rem] leading-relaxed text-[#2a2824] dark:text-[#ede9df]">
          {{ pendingQuestion?.question }}
        </div>
        <button
          class="w-7 h-7 rounded-md flex items-center justify-center text-[#7d756a] hover:bg-black/5 dark:hover:bg-white/5"
          @click="dismissQuestion"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      </div>

      <div v-if="pendingQuestion?.context" class="mb-2 text-[0.8rem] text-[#847b6d] dark:text-[#b1a997]">
        {{ pendingQuestion.context }}
      </div>

      <div v-if="pendingQuestion?.options && pendingQuestion.options.length > 0" class="space-y-1.5">
        <button
          v-for="(opt, idx) in pendingQuestion.options"
          :key="`${idx}-${opt}`"
          class="w-full text-left rounded-xl border border-[#e2ddd2] dark:border-[#474038] bg-white/70 dark:bg-[#201e1a] px-3 py-2.5 text-[0.95rem] text-[#302d28] dark:text-[#e9e3d8] hover:bg-white dark:hover:bg-[#2a2621] transition-colors disabled:opacity-60"
          :disabled="isGenerating"
          @click="chooseOption(opt)"
        >
          <span class="inline-flex items-center justify-center w-6 h-6 rounded-full bg-[#efebe2] dark:bg-[#34302a] text-[0.8rem] mr-2">{{ idx + 1 }}</span>
          <span>{{ opt }}</span>
        </button>
      </div>
    </div>

    <div class="relative bg-white dark:bg-[#2a2a2a] border border-[#e5e5e5] dark:border-[#3a3a3a] rounded-2xl shadow-sm focus-within:ring-2 focus-within:ring-[#e5e5e5] dark:focus-within:ring-[#444] transition-all flex flex-col w-full">
    <textarea 
      ref="textareaRef"
      v-model="currentInput" 
      @keydown.enter="sendMessage"
      @input="autoResize"
      placeholder="Message Nova..."
      rows="1"
      class="w-full bg-transparent border-none text-[0.95rem] text-[#1a1a1a] dark:text-[#ececec] resize-none outline-none block max-h-[40vh] px-4 pt-3 pb-2 placeholder:text-[#a3a3a3]"
    ></textarea>
    
    <div class="flex items-center justify-between px-3 pb-3 pt-2">
      <div class="flex gap-2">
        <button class="w-8 h-8 rounded-lg flex items-center justify-center text-muted-foreground hover:bg-secondary/80 transition-colors">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14M5 12h14"/></svg>
        </button>
      </div>
      <button 
        class="w-8 h-8 rounded-full flex items-center justify-center transition-colors shadow-sm"
        :class="currentInput.trim() && !isGenerating ? 'bg-[#da7756] text-white hover:bg-[#c96c4d]' : 'bg-[#f4f4f4] dark:bg-[#333] text-muted-foreground'"
        :disabled="!currentInput.trim() || isGenerating"
        @click="sendMessage()"
      >
        <svg v-if="!currentInput.trim()" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="22"/></svg>
        <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/></svg>
      </button>
    </div>
    </div>
  </div>
</template>
