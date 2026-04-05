<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { emitToast } from "../../lib/toast";
import type { ScheduledTask } from "../../lib/chat-types";
import {
  createScheduledTask,
  deleteScheduledTask,
  listScheduledTasks,
} from "../../features/chat/services/chat-api";

type MainView = "chat" | "hooks" | "agent" | "schedule";

const emit = defineEmits<{
  (e: "change-main-view", view: MainView): void;
}>();

const loading = ref(false);
const creating = ref(false);
const deletingIds = ref<Record<string, boolean>>({});
const tasks = ref<ScheduledTask[]>([]);

const cron = ref("*/15 * * * *");
const prompt = ref("");
const recurring = ref(true);
const durable = ref(false);

const canCreate = computed(() => cron.value.trim().length > 0 && prompt.value.trim().length > 0);
const sortedTasks = computed(() => {
  return [...tasks.value].sort((a, b) => {
    const av = a.createdAt || "";
    const bv = b.createdAt || "";
    return bv.localeCompare(av);
  });
});

function formatDateTime(iso?: string): string {
  if (!iso) return "-";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString();
}

async function loadTasks() {
  loading.value = true;
  try {
    tasks.value = await listScheduledTasks();
  } catch (err) {
    emitToast({
      variant: "error",
      source: "schedule",
      message: `读取定时任务失败: ${String(err)}`,
    });
  } finally {
    loading.value = false;
  }
}

async function handleCreateTask() {
  if (!canCreate.value || creating.value) {
    return;
  }

  creating.value = true;
  try {
    await createScheduledTask({
      cron: cron.value.trim(),
      prompt: prompt.value.trim(),
      recurring: recurring.value,
      durable: durable.value,
    });

    prompt.value = "";
    await loadTasks();
    emitToast({
      variant: "success",
      source: "schedule",
      message: "定时任务已创建。",
    });
  } catch (err) {
    emitToast({
      variant: "error",
      source: "schedule",
      message: `创建定时任务失败: ${String(err)}`,
    });
  } finally {
    creating.value = false;
  }
}

async function handleDeleteTask(id: string) {
  if (!id || deletingIds.value[id]) {
    return;
  }

  deletingIds.value = {
    ...deletingIds.value,
    [id]: true,
  };

  try {
    const removed = await deleteScheduledTask(id);
    if (!removed) {
      emitToast({
        variant: "error",
        source: "schedule",
        message: `任务 ${id} 不存在或已删除。`,
      });
      return;
    }

    await loadTasks();
    emitToast({
      variant: "success",
      source: "schedule",
      message: `已删除任务 ${id}。`,
    });
  } catch (err) {
    emitToast({
      variant: "error",
      source: "schedule",
      message: `删除定时任务失败: ${String(err)}`,
    });
  } finally {
    const next = { ...deletingIds.value };
    delete next[id];
    deletingIds.value = next;
  }
}

onMounted(() => {
  loadTasks();
});
</script>

<template>
  <div class="box-border flex h-full flex-col gap-4 overflow-auto bg-[#fcfcfb] px-5 pb-5 pt-[72px] dark:bg-transparent">
    <header class="flex flex-wrap items-start justify-between gap-3">
      <div class="space-y-1">
        <h2 class="text-base font-semibold text-[#2f2a24] dark:text-[#ece8de]">定时任务</h2>
        <p class="text-sm text-[#8a8174] dark:text-[#b5ada0]">管理 CronCreate / CronList / CronDelete 对应的任务列表。</p>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <Button
          variant="ghost"
          size="sm"
          class="border border-[#e3d8c7] bg-white text-[#5d5448] hover:bg-[#f6f1e8] dark:border-[#474136] dark:bg-[#2a2824] dark:text-[#d9d1c3] dark:hover:bg-[#34312b]"
          @click="emit('change-main-view', 'chat')"
        >
          返回聊天
        </Button>
        <Button
          variant="ghost"
          size="sm"
          class="border border-[#e3d8c7] bg-white text-[#5d5448] hover:bg-[#f6f1e8] dark:border-[#474136] dark:bg-[#2a2824] dark:text-[#d9d1c3] dark:hover:bg-[#34312b]"
          :disabled="loading || creating"
          @click="loadTasks"
        >
          刷新
        </Button>
      </div>
    </header>

    <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
      <CardHeader class="px-4 pb-0">
        <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">新建任务</CardTitle>
      </CardHeader>
      <CardContent class="space-y-3 px-4">
        <div class="space-y-1">
          <label class="text-[0.86rem] text-[#5f574a] dark:text-[#d8cfbf]">Cron 表达式</label>
          <Input
            v-model="cron"
            class="border-[#ddd3c4] bg-white/95 text-[#2f2b24] focus-visible:border-[#d28a71] focus-visible:ring-[#da7756]/25 dark:border-[#4f473b] dark:bg-[#24221f] dark:text-[#e4dccd] dark:focus-visible:border-[#b77a63]"
            placeholder="例如: */15 * * * *"
          />
        </div>

        <div class="space-y-1">
          <label class="text-[0.86rem] text-[#5f574a] dark:text-[#d8cfbf]">任务内容</label>
          <Textarea
            v-model="prompt"
            rows="3"
            class="border-[#ddd3c4] bg-white/95 text-[#2f2b24] focus-visible:border-[#d28a71] focus-visible:ring-[#da7756]/25 dark:border-[#4f473b] dark:bg-[#24221f] dark:text-[#e4dccd] dark:focus-visible:border-[#b77a63]"
            placeholder="到点要执行的提示词"
          />
        </div>

        <div class="flex flex-wrap items-center gap-4 text-sm text-[#6a6256] dark:text-[#c7beaf]">
          <label class="inline-flex items-center gap-2 cursor-pointer">
            <input v-model="recurring" type="checkbox" class="rounded border-[#c8baa3]" />
            <span>周期任务（recurring）</span>
          </label>
          <label class="inline-flex items-center gap-2 cursor-pointer">
            <input v-model="durable" type="checkbox" class="rounded border-[#c8baa3]" />
            <span>跨重启持久化（durable）</span>
          </label>
        </div>

        <div class="pt-1">
          <Button
            size="sm"
            class="bg-[#da7756] text-white hover:bg-[#c96c4d] focus-visible:ring-[#da7756]/35 disabled:bg-[#e4b2a1]"
            :disabled="!canCreate || creating"
            @click="handleCreateTask"
          >
            {{ creating ? '创建中...' : '创建定时任务' }}
          </Button>
        </div>
      </CardContent>
    </Card>

    <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
      <CardHeader class="px-4 pb-0">
        <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">当前任务</CardTitle>
      </CardHeader>
      <CardContent class="space-y-2 px-4">
        <div v-if="loading" class="text-sm text-[#8a8172] dark:text-[#b8b0a2]">正在读取任务...</div>
        <div v-else-if="sortedTasks.length === 0" class="text-sm text-[#8a8172] dark:text-[#b8b0a2]">暂无定时任务。</div>

        <div v-else class="space-y-2">
          <div
            v-for="task in sortedTasks"
            :key="task.id"
            class="flex items-start justify-between gap-3 rounded-xl border border-[#e8dece] bg-white/80 p-3 dark:border-[#464034] dark:bg-[#25231f]"
          >
            <div class="min-w-0 flex-1 space-y-1">
              <div class="flex flex-wrap items-center gap-2">
                <span class="text-[0.82rem] font-semibold text-[#4d463c] dark:text-[#ddd3c5]">{{ task.id }}</span>
                <span class="text-[11px] rounded px-2 py-0.5 bg-[#efe7da] text-[#6d604f] dark:bg-[#3a342c] dark:text-[#cfbda5]">{{ task.recurring ? '周期' : '一次性' }}</span>
                <span class="text-[11px] rounded px-2 py-0.5 bg-[#efe7da] text-[#6d604f] dark:bg-[#3a342c] dark:text-[#cfbda5]">{{ task.durable ? 'durable' : 'session' }}</span>
              </div>
              <div class="text-[0.82rem] text-[#6e665a] dark:text-[#bbb2a4]">cron: {{ task.cron }}</div>
              <div class="text-[0.82rem] text-[#6e665a] dark:text-[#bbb2a4]">conversation: {{ task.conversationId || '-' }}</div>
              <div class="text-[0.86rem] text-[#302c26] dark:text-[#e5ddd0] break-words">{{ task.prompt }}</div>
              <div class="text-[11px] text-[#9c9487] dark:text-[#a59c8e]">创建于 {{ formatDateTime(task.createdAt) }}</div>
            </div>

            <Button
              variant="ghost"
              size="sm"
              class="text-[#9c5b4a] hover:bg-[#f4e6df] dark:hover:bg-[#3a2d2a]"
              :disabled="!!deletingIds[task.id]"
              @click="handleDeleteTask(task.id)"
            >
              {{ deletingIds[task.id] ? '删除中...' : '删除' }}
            </Button>
          </div>
        </div>
      </CardContent>
    </Card>
  </div>
</template>
