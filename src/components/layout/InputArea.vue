<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { AgentMode } from '../../lib/chat-types';

const props = defineProps<{
  isGenerating?: boolean;
  agentMode?: AgentMode;
}>();

const emit = defineEmits<{
  (e: 'send', msg: string): void;
  (e: 'cancel'): void;
  (e: 'mode-change', mode: AgentMode): void;
}>();

const currentInput = ref("");
const textareaRef = ref<HTMLTextAreaElement | null>(null);

const settings = ref<any>(null);

const normalizeProviderKey = (provider: string) => (provider || '').trim().toLowerCase() || 'anthropic';

const ensureActiveProfile = () => {
  if (!settings.value) return null;
  const provider = normalizeProviderKey(settings.value.provider || 'anthropic');
  settings.value.provider = provider;
  if (!settings.value.providerProfiles || typeof settings.value.providerProfiles !== 'object') {
    settings.value.providerProfiles = {};
  }
  if (!settings.value.providerProfiles[provider]) {
    settings.value.providerProfiles[provider] = {
      apiKey: settings.value.apiKey || '',
      baseUrl: settings.value.baseUrl || '',
      model: settings.value.model || '',
    };
  }
  return settings.value.providerProfiles[provider];
};

const availableModels = computed(() => {
  if (!settings.value || !settings.value.provider || !settings.value.customModels) return [];
  return settings.value.customModels[settings.value.provider] || [];
});

const currentModel = computed({
  get: () => {
    const profile = ensureActiveProfile();
    return profile?.model || settings.value?.model || '';
  },
  set: (value: string) => {
    const profile = ensureActiveProfile();
    if (!profile) return;
    profile.model = value;
    settings.value.model = value;
  },
});

const localAgentMode = computed<AgentMode>({
  get: () => props.agentMode ?? 'agent',
  set: (value: AgentMode) => {
    emit('mode-change', value);
  },
});

const onAgentModeChange = (event: Event) => {
  const select = event.target as HTMLSelectElement;
  const value = (select.value as AgentMode) || 'agent';
  localAgentMode.value = value;
};

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
  currentModel.value = select.value;
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


// 发送消息，支持 Shift + Enter 换行，当 isGenerating 为 true 时禁用发送功能
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
    <div
      class="relative bg-white dark:bg-[#2a2a2a] border border-[#e5e5e5] dark:border-[#3a3a3a] rounded-2xl shadow-sm focus-within:ring-2 focus-within:ring-[#e5e5e5] dark:focus-within:ring-[#444] transition-all flex flex-col w-full">
      <textarea ref="textareaRef" v-model="currentInput" @keydown.enter="sendMessage" @input="autoResize"
        placeholder="Message Nova..." rows="1"
        class="w-full bg-transparent border-none text-[0.95rem] text-[#1a1a1a] dark:text-[#ececec] resize-none outline-none block max-h-[40vh] px-4 pt-3 pb-2 placeholder:text-[#a3a3a3]"></textarea>

      <div class="flex items-center justify-between px-3 pb-3 pt-2">
        <div class="flex gap-2 items-center">
          <button
            class="w-8 h-8 rounded-lg flex items-center justify-center text-muted-foreground hover:bg-secondary/80 transition-colors">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
              stroke-linecap="round" stroke-linejoin="round">
              <path d="M12 5v14M5 12h14" />
            </svg>
          </button>

          <div class="flex items-center gap-1 border border-[#e5e5e5] dark:border-[#3a3a3a] rounded-md px-2 py-1 hover:bg-secondary/60 transition-colors">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground">
              <path d="M9 18l6-6-6-6" />
            </svg>
            <select
              :value="localAgentMode"
              @change="onAgentModeChange"
              class="bg-transparent text-xs outline-none text-muted-foreground cursor-pointer"
            >
              <option value="agent">Agent 模式</option>
              <option value="plan">Plan 模式</option>
              <option value="auto">自动迭代模式</option>
            </select>
          </div>

          <select v-if="availableModels.length > 0 && settings" v-model="currentModel" @change="onModelChange"
            class="bg-transparent border border-[#e5e5e5] dark:border-[#3a3a3a] text-xs rounded-md px-2 py-1 outline-none text-muted-foreground hover:bg-secondary/80 transition-colors cursor-pointer max-w-[200px]">
            <option v-for="model in availableModels" :key="model" :value="model">
              {{ model }}
            </option>
          </select>
        </div>
        <button class="w-8 h-8 rounded-full flex items-center justify-center transition-colors shadow-sm"
          :class="isGenerating
            ? 'bg-[#f4d9d2] text-[#9b4b39] hover:bg-[#eacdc5]'
            : (currentInput.trim() ? 'bg-[#da7756] text-white hover:bg-[#c96c4d]' : 'bg-[#f4f4f4] dark:bg-[#333] text-muted-foreground')"
          :disabled="!isGenerating && !currentInput.trim()" @click="isGenerating ? emit('cancel') : sendMessage()">
          <svg v-if="isGenerating" width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
            <rect x="6" y="6" width="12" height="12" rx="2" ry="2" />
          </svg>
          <svg v-else-if="!currentInput.trim()" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor"
            stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
            <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
            <line x1="12" y1="19" x2="12" y2="22" />
          </svg>
          <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
            stroke-linecap="round" stroke-linejoin="round">
            <line x1="12" y1="19" x2="12" y2="5" />
            <polyline points="5 12 12 5 19 12" />
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>
