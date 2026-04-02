<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue';

interface PendingQuestion {
  question?: string;
  context?: string;
  options?: string[];
  allow_freeform?: boolean;
}

const props = defineProps<{
  request: PendingQuestion | null;
}>();

const emit = defineEmits<{
  (e: 'select', value: string): void;
  (e: 'other', value: string): void;
  (e: 'skip'): void;
}>();

const otherValue = ref('');
const activeIndex = ref(0);

const submitOther = () => {
  const value = otherValue.value.trim();
  if (!value) return;
  emit('other', value);
  otherValue.value = '';
};

const handleKeydown = (event: KeyboardEvent) => {
  if (!props.request) return;

  const options = props.request.options ?? [];
  if (event.key === 'Escape') {
    event.preventDefault();
    emit('skip');
    return;
  }

  if (!options.length) return;

  if (event.key === 'ArrowDown') {
    event.preventDefault();
    activeIndex.value = (activeIndex.value + 1) % options.length;
    return;
  }

  if (event.key === 'ArrowUp') {
    event.preventDefault();
    activeIndex.value = (activeIndex.value - 1 + options.length) % options.length;
    return;
  }

  if (event.key === 'Enter' && !(event.target instanceof HTMLInputElement)) {
    const option = options[activeIndex.value];
    if (!option) return;
    event.preventDefault();
    emit('select', option);
  }
};

watch(
  () => props.request,
  () => {
    activeIndex.value = 0;
    otherValue.value = '';
  },
  { immediate: true }
);

onMounted(() => {
  window.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <div v-if="request" class="ask-shell">
    <div class="ask-card">
      <div class="ask-header">
        <div class="ask-title">{{ request.question }}</div>
        <button class="ask-close" title="关闭" @click="emit('skip')">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
            <path d="M6 6l12 12M18 6L6 18" stroke-linecap="round" />
          </svg>
        </button>
      </div>

      <div v-if="request.context" class="ask-context">{{ request.context }}</div>

      <div v-if="request.options?.length" class="ask-options">
        <button
          v-for="(option, index) in request.options"
          :key="`${index}-${option}`"
          class="ask-option"
          :class="{ 'is-selected': activeIndex === index }"
          @mouseenter="activeIndex = index"
          @click="emit('select', option)"
        >
          <span class="ask-index">{{ index + 1 }}</span>
          <span class="ask-label">{{ option }}</span>
          <svg class="ask-arrow" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
            <path d="M8 12h8m0 0-4-4m4 4-4 4" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
      </div>

      <div v-if="request.allow_freeform !== false" class="ask-other">
        <div class="ask-other-icon">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
            <path d="M12 20h9" stroke-linecap="round" />
            <path d="M16.5 3.5a2.12 2.12 0 1 1 3 3L7 19l-4 1 1-4 12.5-12.5Z" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </div>
        <input
          v-model="otherValue"
          class="ask-input"
          placeholder="Something else"
          @keydown.enter.prevent="submitOther"
        />
        <button class="ask-skip" @click="emit('skip')">Skip</button>
      </div>
    </div>

    <div v-if="request.options?.length" class="ask-hint">↑↓ to navigate ・ Enter to select ・ Esc to skip</div>
  </div>
</template>

<style scoped>
.ask-shell {
  width: 100%;
  box-sizing: border-box;
}

.ask-card {
  width: 100%;
  max-width: 720px;
  margin: 0 auto;
  box-sizing: border-box;
  border: 1px solid #ddd7ca;
  border-radius: 18px;
  background: #fffdfa;
  padding: 12px;
  box-shadow: 0 10px 30px rgba(45, 34, 18, 0.08);
}

.ask-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 2px 2px 10px;
}

.ask-title {
  min-width: 0;
  color: #262117;
  font-size: 14px;
  line-height: 1.45;
  white-space: pre-wrap;
  word-break: break-word;
}

.ask-close {
  flex-shrink: 0;
  width: 28px;
  height: 28px;
  border: 0;
  border-radius: 999px;
  background: transparent;
  color: #746d60;
}

.ask-close:hover {
  background: #f3eee4;
}

.ask-context {
  padding: 0 2px 8px;
  color: #938a7b;
  font-size: 12px;
  line-height: 1.45;
}

.ask-options {
  display: flex;
  flex-direction: column;
}

.ask-option {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 8px;
  border: 0;
  border-top: 1px solid #ece6da;
  background: transparent;
  text-align: left;
}

.ask-option:first-child {
  border-top: 0;
}

.ask-option:hover {
  background: #faf6ed;
}

.ask-option.is-selected {
  background: #f6f1e7;
  border-radius: 14px;
}

.ask-index {
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  border-radius: 12px;
  background: #ece7dd;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: #6a6357;
  font-size: 14px;
}

.ask-label {
  min-width: 0;
  flex: 1;
  color: #262117;
  font-size: 14px;
  line-height: 1.45;
}

.ask-arrow {
  flex-shrink: 0;
  color: #9d9587;
}

.ask-other {
  display: flex;
  align-items: center;
  gap: 10px;
  border-top: 1px solid #ece6da;
  margin-top: 4px;
  padding: 12px 4px 4px;
}

.ask-other-icon {
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  border-radius: 12px;
  background: #ece7dd;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: #6a6357;
}

.ask-input {
  min-width: 0;
  flex: 1;
  border: 0;
  outline: 0;
  background: transparent;
  color: #262117;
  font-size: 14px;
  line-height: 1.4;
}

.ask-input::placeholder {
  color: #aca495;
}

.ask-skip {
  flex-shrink: 0;
  border-radius: 10px;
  border: 1px solid #d4ccbf;
  background: #fffdfa;
  color: #262117;
  padding: 7px 14px;
  font-size: 13px;
}

.ask-skip:hover {
  background: #f7f2e8;
}

.ask-hint {
  margin-top: 8px;
  text-align: center;
  color: #a39a8c;
  font-size: 11px;
  line-height: 1.4;
}
</style>
