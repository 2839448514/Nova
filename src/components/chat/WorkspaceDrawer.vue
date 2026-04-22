<script setup lang="ts">
import { ref, watch, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import AgentFlowGraph from './AgentFlowGraph.vue';
import FileDiffView from './FileDiffView.vue';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import type { ToolExecutionEntry, FlowNodeEntry } from '../../lib/chat-types';

const props = defineProps<{
  open: boolean;
  entries: ToolExecutionEntry[];
  flowNodes: FlowNodeEntry[];
  isGenerating: boolean;
  hasMessages: boolean;
  lastUserMessage?: string;
  lastAssistantMessage?: string;
  codingMode?: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

// ── Coding workspace ─────────────────────────────────────────────────────────
const codingWorkspace = ref<string | null>(null);

async function loadCodingWorkspace() {
  try {
    const settings = await invoke<{ codingWorkspace?: string }>('get_settings');
    codingWorkspace.value = settings.codingWorkspace ?? null;
  } catch { /* ignore */ }
}

async function pickWorkspace() {
  try {
    const picked = await invoke<string | null>('pick_coding_workspace');
    if (picked) {
      codingWorkspace.value = picked;
      const settings = await invoke<Record<string, unknown>>('get_settings');
      settings.codingWorkspace = picked;
      await invoke('save_settings', { settings });
    }
  } catch (e) {
    console.error('Failed to pick workspace:', e);
  }
}

onMounted(loadCodingWorkspace);
watch(() => props.codingMode, (v) => { if (v) loadCodingWorkspace(); });

/** Display name: just the last path segment */
const workspaceLabel = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
</script>

<template>
  <!-- Backdrop -->
  <Transition name="fade">
    <div
      v-if="open"
      class="absolute inset-0 z-20 bg-black/20 dark:bg-black/40"
      @click="emit('close')"
    />
  </Transition>

  <!-- Drawer panel -->
  <Transition name="slide-right">
    <div
      v-if="open"
      class="absolute top-0 right-0 h-full z-30 flex flex-col"
      style="width: 90%"
    >
      <!-- Panel surface -->
      <div class="flex flex-col h-full bg-[#faf9f6] dark:bg-[#1e1e1e] border-l border-[#e7e2d7] dark:border-[#333] shadow-2xl overflow-hidden">
        <Tabs default-value="agent-flow" class="flex flex-col h-full gap-0">

          <!-- Header -->
          <div class="h-14 flex items-center justify-between px-4 border-b border-[#e7e2d7] dark:border-[#333] shrink-0">
            <!-- shadcn TabsList — transparent background to match current theme -->
            <TabsList class="bg-transparent p-0 h-auto gap-1 rounded-none">
              <TabsTrigger
                value="agent-flow"
                class="px-3 py-1.5 text-sm font-medium rounded-md transition-colors border-transparent shadow-none h-auto flex-none
                  data-[state=active]:bg-[#e8e3d8] dark:data-[state=active]:bg-[#333]
                  data-[state=active]:text-[#1a1a1a] dark:data-[state=active]:text-[#ececec]
                  data-[state=active]:shadow-none
                  data-[state=inactive]:text-muted-foreground
                  data-[state=inactive]:hover:bg-black/5 dark:data-[state=inactive]:hover:bg-white/5"
              >Agent 流图</TabsTrigger>
              <TabsTrigger
                value="diff"
                class="px-3 py-1.5 text-sm font-medium rounded-md transition-colors border-transparent shadow-none h-auto flex-none
                  data-[state=active]:bg-[#e8e3d8] dark:data-[state=active]:bg-[#333]
                  data-[state=active]:text-[#1a1a1a] dark:data-[state=active]:text-[#ececec]
                  data-[state=active]:shadow-none
                  data-[state=inactive]:text-muted-foreground
                  data-[state=inactive]:hover:bg-black/5 dark:data-[state=inactive]:hover:bg-white/5"
              >Code Diff</TabsTrigger>
            </TabsList>

            <!-- Close button -->
            <button
              class="w-8 h-8 flex items-center justify-center rounded-md text-muted-foreground hover:bg-black/5 dark:hover:bg-white/5 transition-colors"
              @click="emit('close')"
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18"/>
                <line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>

          <!-- Agent Flow tab content -->
          <TabsContent value="agent-flow" class="min-h-0 overflow-hidden m-0 p-0">
            <AgentFlowGraph
              :entries="entries"
              :flowNodes="flowNodes"
              :isGenerating="isGenerating"
              :hasMessages="hasMessages"
              :lastUserMessage="lastUserMessage"
              :lastAssistantMessage="lastAssistantMessage"
              class="w-full h-full"
            />
          </TabsContent>

          <!-- Code Diff tab content -->
          <TabsContent value="diff" class="min-h-0 overflow-hidden m-0 p-0 flex flex-col">

            <!-- Coding workspace selector bar (coding mode only) -->
            <div
              v-if="props.codingMode"
              class="flex items-center gap-2 px-4 py-2 border-b border-[#e7e2d7] dark:border-[#333] shrink-0 bg-[#faf9f6] dark:bg-[#1a1a1a]"
            >
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="shrink-0 text-muted-foreground">
                <path d="M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z"/>
              </svg>
              <span
                v-if="codingWorkspace"
                class="text-xs font-medium text-[#1a1a1a] dark:text-[#ececec] truncate flex-1 min-w-0"
                :title="codingWorkspace"
              >{{ workspaceLabel(codingWorkspace) }}</span>
              <span v-else class="text-xs text-muted-foreground italic flex-1">未选择工作区</span>
              <button
                class="shrink-0 text-[11px] px-2 py-0.5 rounded border border-[#e7e2d7] dark:border-[#333] text-muted-foreground hover:bg-black/5 dark:hover:bg-white/5 transition-colors"
                @click="pickWorkspace"
              >{{ codingWorkspace ? '更换' : '选择工作区' }}</button>
            </div>

            <FileDiffView
              :entries="entries"
              :codingMode="codingMode"
              :workspaceReady="codingMode ? !!codingWorkspace : true"
              class="w-full flex-1 min-h-0"
            />
          </TabsContent>

        </Tabs>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.25s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.slide-right-enter-active,
.slide-right-leave-active {
  transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}
.slide-right-enter-from,
.slide-right-leave-to {
  transform: translateX(100%);
}
.slide-right-enter-to,
.slide-right-leave-from {
  transform: translateX(0%);
}
</style>
