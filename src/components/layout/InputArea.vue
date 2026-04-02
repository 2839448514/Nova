<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';

const props = defineProps<{
  isGenerating?: boolean;
}>();

const emit = defineEmits<{
  (e: 'send', msg: string): void;
}>();

const currentInput = ref("");
const textareaRef = ref<HTMLTextAreaElement | null>(null);

const settings = ref<any>(null);

const availableModels = computed(() => {
  if (!settings.value || !settings.value.provider || !settings.value.customModels) return [];
  return settings.value.customModels[settings.value.provider] || [];
});

const loadSettings = async () => {
  try {
    settings.value = await invoke('get_settings');
  } catch (error) {
    console.error('Failed to load settings in InputArea:', error);
  }
};

const onModelChange = async (event: Event) => {
  const select = event.target as HTMLSelectElement;
  if (!settings.value) return;
  settings.value.model = select.value;
  try {
    await invoke('save_settings', { settings: settings.value });
  } catch (error) {
    console.error('Failed to save model change:', error);
  }
};

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

watch(
  () => props.isGenerating,
  () => {
    nextTick(() => {
      autoResize();
      focusTextarea();
    });
  }
);

const handleSettingsUpdate = () => loadSettings();
onMounted(() => {
  loadSettings();
  window.addEventListener('settings-updated', handleSettingsUpdate);
  nextTick(() => {
    autoResize();
    focusTextarea();
  });
});

onUnmounted(() => {
  window.removeEventListener('settings-updated', handleSettingsUpdate);
});

defineExpose({
  focusTextarea,
});
</script>

<template>
  <div class="w-full">
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
      <div class="flex gap-2 items-center">
        <button class="w-8 h-8 rounded-lg flex items-center justify-center text-muted-foreground hover:bg-secondary/80 transition-colors">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14M5 12h14"/></svg>
        </button>

        <select 
          v-if="availableModels.length > 0 && settings"
          v-model="settings.model"
          @change="onModelChange"
          class="bg-transparent border border-[#e5e5e5] dark:border-[#3a3a3a] text-xs rounded-md px-2 py-1 outline-none text-muted-foreground hover:bg-secondary/80 transition-colors cursor-pointer max-w-[200px]"
        >
          <option v-for="model in availableModels" :key="model" :value="model">
            {{ model }}
          </option>
        </select>
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
