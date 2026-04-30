<script setup lang="ts">
import { computed } from "vue";
import type { ToolExecutionEntry, ToolTurnSummary } from "../../../lib/chat-types";
import {
  renderToolTurnCategoryLine,
  renderToolTurnSummaryLine,
} from "../../../features/chat/utils/tool-activity-summary";
import CurrentTurnActivityRail from "./CurrentTurnActivityRail.vue";

const props = defineProps<{
  summary: ToolTurnSummary;
}>();

const summaryLine = computed(() => renderToolTurnSummaryLine(props.summary));
const categoryLine = computed(() => renderToolTurnCategoryLine(props.summary));
const detailEntries = computed<ToolExecutionEntry[]>(() => props.summary.entries.map((entry) => ({ ...entry })));
</script>

<template>
  <details class="turn-summary-card">
    <summary class="turn-summary-card__summary">
      <div class="turn-summary-card__header">
        <div class="turn-summary-card__title">{{ summaryLine }}</div>
        <div class="turn-summary-card__meta">{{ categoryLine }}</div>
      </div>
      <span class="turn-summary-card__toggle">点开看详情</span>
    </summary>

    <div class="turn-summary-card__body">
      <CurrentTurnActivityRail :entries="detailEntries" />
    </div>
  </details>
</template>

<style scoped>
.turn-summary-card {
  margin: 10px 0 12px;
  border: 1px solid rgba(226, 219, 205, 0.95);
  background:
    linear-gradient(180deg, rgba(252, 249, 244, 0.96), rgba(248, 244, 236, 0.92)),
    #faf7f1;
  border-radius: 14px;
  overflow: hidden;
}

.turn-summary-card__summary {
  list-style: none;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 14px;
}

.turn-summary-card__summary::-webkit-details-marker {
  display: none;
}

.turn-summary-card__header {
  min-width: 0;
}

.turn-summary-card__title {
  font-size: 13px;
  line-height: 1.25;
  font-weight: 700;
  color: #574f43;
}

.turn-summary-card__meta {
  margin-top: 3px;
  font-size: 11px;
  line-height: 1.4;
  color: #8a8173;
}

.turn-summary-card__toggle {
  flex: 0 0 auto;
  font-size: 11px;
  color: #9a927f;
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.turn-summary-card__toggle::before {
  content: "▸";
  transition: transform 0.18s ease;
}

.turn-summary-card[open] .turn-summary-card__toggle::before {
  transform: rotate(90deg);
}

.turn-summary-card__body {
  padding: 0 14px 12px;
}

.dark .turn-summary-card {
  border-color: #4b4439;
  background:
    linear-gradient(180deg, rgba(44, 39, 33, 0.96), rgba(36, 32, 28, 0.94)),
    #2c2721;
}

.dark .turn-summary-card__title {
  color: #ece4d8;
}

.dark .turn-summary-card__meta {
  color: #b8aea0;
}

.dark .turn-summary-card__toggle {
  color: #a79f93;
}
</style>
