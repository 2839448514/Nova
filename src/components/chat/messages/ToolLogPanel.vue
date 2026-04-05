<script setup lang="ts">
import { Card, CardContent } from '@/components/ui/card';

defineProps<{
  items: string[];
}>();
</script>

<template>
  <Card v-if="items.length > 0" class="tool-log-panel py-0">
    <CardContent class="px-3 py-2.5">
      <div class="tool-log-title">工具调用</div>
      <div
        v-for="(item, toolIndex) in items"
        :key="`tool-${toolIndex}`"
        class="tool-log-item"
      >
        {{ item }}
      </div>
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
</style>
