<script setup lang="ts">
import InputArea from '../layout/InputArea.vue';

defineProps<{
  isGenerating?: boolean;
}>();

const emit = defineEmits<{
  (e: 'send', msg: string): void;
}>();

const handleSend = (msg: string) => {
  emit('send', msg);
};
</script>

<template>
  <div class="flex-1 flex flex-col items-center justify-center pt-10 px-4 w-full h-full">
    
    <div class="flex items-center justify-center text-[0.8rem] text-muted-foreground mb-6 font-medium">
      <span class="px-3 py-1 rounded-full border border-border/60 bg-transparent opacity-80 cursor-pointer">
        Free plan <span class="mx-1">·</span> <span class="text-[#1a1a1a] dark:text-[#ececec] underline underline-offset-2">Upgrade</span>
      </span>
    </div>

    <h1 class="text-4xl text-[#1a1a1a] dark:text-[#ececec] font-serif mb-8 flex items-center justify-center gap-4 tracking-tight">
      <svg class="text-[#da7756]" width="38" height="38" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>
      </svg>
      Back at it, yileina
    </h1>

    <div class="w-full max-w-[42rem] flex flex-col">
      <InputArea :isGenerating="isGenerating" @send="handleSend" />

      <!-- Suggestion Pills -->
      <div class="flex flex-wrap items-center justify-center gap-2 mt-4 text-[#555] dark:text-[#aaa]">
        <button class="flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-border/80 bg-white dark:bg-[#2a2a2a] hover:bg-[#f3f3f3] dark:hover:bg-[#333] transition-colors text-[0.85rem] font-medium shadow-sm">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
          Write
        </button>
        <button class="flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-border/80 bg-white dark:bg-[#2a2a2a] hover:bg-[#f3f3f3] dark:hover:bg-[#333] transition-colors text-[0.85rem] font-medium shadow-sm">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 10v6M2 10l10-5 10 5-10 5z"/><path d="M6 12v5c3 3 9 3 12 0v-5"/></svg>
          Learn
        </button>
        <button class="flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-border/80 bg-white dark:bg-[#2a2a2a] hover:bg-[#f3f3f3] dark:hover:bg-[#333] transition-colors text-[0.85rem] font-medium shadow-sm">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
          Code
        </button>
        <button class="flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-border/80 bg-white dark:bg-[#2a2a2a] hover:bg-[#f3f3f3] dark:hover:bg-[#333] transition-colors text-[0.85rem] font-medium shadow-sm">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 8h1a4 4 0 0 1 0 8h-1M2 8h16v9a4 4 0 0 1-4 4H6a4 4 0 0 1-4-4V8zM6 1v3M10 1v3M14 1v3"/></svg>
          Life stuff
        </button>
        <button class="flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-border/80 bg-white dark:bg-[#2a2a2a] hover:bg-[#f3f3f3] dark:hover:bg-[#333] transition-colors text-[0.85rem] font-medium shadow-sm">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/></svg>
          Claude's choice
        </button>
      </div>
    </div>
  </div>
</template>
