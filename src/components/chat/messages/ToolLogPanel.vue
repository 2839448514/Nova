<script setup lang="ts">
import { computed } from 'vue';
import { Card, CardContent } from '@/components/ui/card';

const props = defineProps<{
  items: string[];
  defaultOpen?: boolean;
}>();

const previewText = computed(() => {
  const first = props.items[0]?.trim() ?? '';
  if (!first) {
    return '';
  }
  return first.length > 80 ? `${first.slice(0, 80)}...` : first;
});
</script>

<template>
  <Card v-if="items.length > 0" class="tool-log-panel py-0">
    <CardContent class="px-3 py-2.5">
      <details class="tool-log-details" :open="defaultOpen ?? false">
        <summary class="tool-log-summary">
          <span class="tool-log-title">工具调用</span>
          <span class="tool-log-count">{{ items.length }}</span>
          <span v-if="previewText" class="tool-log-preview">{{ previewText }}</span>
        </summary>
        <div class="tool-log-list">
          <div
            v-for="(item, toolIndex) in items"
            :key="`tool-${toolIndex}`"
            class="tool-log-item"
          >
            {{ item }}
          </div>
        </div>
      </details>
    </CardContent>
  </Card>
</template>

<style scoped>
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
}

.tool-log-details {
  width: 100%;
}

.tool-log-summary {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  list-style: none;
  user-select: none;
  color: #7d7667;
}

.tool-log-summary::-webkit-details-marker {
  display: none;
}

.tool-log-summary::before {
  content: "▸";
  display: inline-block;
  color: #8e8678;
  transition: transform 0.15s ease;
}

.tool-log-details[open] .tool-log-summary::before {
  transform: rotate(90deg);
}

.tool-log-count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  border-radius: 999px;
  border: 1px solid #e3ddcf;
  background: #f2ede2;
  color: #746d5f;
  font-size: 10px;
  line-height: 1;
  font-family: "SF Mono", "Fira Code", monospace;
}

.tool-log-preview {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #938b7e;
  font-size: 11px;
  font-family: "SF Mono", "Fira Code", monospace;
}

.tool-log-list {
  margin-top: 6px;
}

.tool-log-item {
  font-size: 12px;
  line-height: 1.6;
  color: #5e584c;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: 'SF Mono', 'Fira Code', monospace;
}

.dark .tool-log-summary {
  color: #b9b1a3;
}

.dark .tool-log-summary::before {
  color: #b8b0a2;
}

.dark .tool-log-count {
  border-color: #4c463b;
  background: #383328;
  color: #d6cdbf;
}

.dark .tool-log-preview {
  color: #a69e90;
}
</style>
