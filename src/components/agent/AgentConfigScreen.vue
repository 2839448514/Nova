<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { emitToast } from "@/lib/toast";

type MainView = "chat" | "hooks" | "agent";

type AgentProfileMeta = {
  id: string;
  name: string;
  fileName: string;
  updatedAt: number;
  path: string;
};

const emit = defineEmits<{
  (e: "change-main-view", view: MainView): void;
}>();

const loadingList = ref(false);
const loadingContent = ref(false);
const saving = ref(false);
const creating = ref(false);
const deleting = ref(false);
const showCreatePanel = ref(false);
const showDeletePanel = ref(false);
const newProfileName = ref("new-agent");
const profiles = ref<AgentProfileMeta[]>([]);
const selectedProfileId = ref("");
const selectedProfilePath = ref("");
const content = ref("");
const originalContent = ref("");

const hasChanges = computed(() => content.value !== originalContent.value);
const hasSelectedProfile = computed(() => selectedProfileId.value.trim().length > 0);
const hasProfiles = computed(() => profiles.value.length > 0);
const isBusy = computed(
  () => loadingList.value || loadingContent.value || saving.value || creating.value || deleting.value,
);

const selectedProfile = computed(() =>
  profiles.value.find((item) => item.id === selectedProfileId.value) ?? null,
);

const selectedProfileLabel = computed(() => selectedProfile.value?.name ?? "未选择智能体");

const formatUpdatedAt = (unixSeconds: number) => {
  if (!Number.isFinite(unixSeconds) || unixSeconds <= 0) {
    return "--";
  }
  return new Date(unixSeconds * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
};

async function loadAgentProfiles() {
  loadingList.value = true;
  try {
    const items = await invoke<AgentProfileMeta[]>("list_agent_profiles");
    profiles.value = items ?? [];

    if (profiles.value.length === 0) {
      selectedProfileId.value = "";
      selectedProfilePath.value = "";
      content.value = "";
      originalContent.value = "";
      return;
    }

    const exists = profiles.value.some((item) => item.id === selectedProfileId.value);
    if (!exists) {
      selectedProfileId.value = profiles.value[0].id;
    }

    await loadSelectedProfileContent();
  } catch (err) {
    emitToast({
      variant: "error",
      source: "agent-config",
      message: `读取智能体列表失败: ${String(err)}`,
    });
  } finally {
    loadingList.value = false;
  }
}

async function loadSelectedProfileContent() {
  if (!selectedProfileId.value) {
    selectedProfilePath.value = "";
    content.value = "";
    originalContent.value = "";
    return;
  }

  loadingContent.value = true;
  try {
    const text = await invoke<string>("load_agent_profile_markdown", {
      profileId: selectedProfileId.value,
    });

    const matched = selectedProfile.value;
    selectedProfilePath.value = matched?.path ?? "";
    content.value = text ?? "";
    originalContent.value = content.value;
  } catch (err) {
    emitToast({
      variant: "error",
      source: "agent-config",
      message: `读取智能体配置失败: ${String(err)}`,
    });
  } finally {
    loadingContent.value = false;
  }
}

function openCreatePanel() {
  if (isBusy.value) {
    return;
  }
  showDeletePanel.value = false;
  newProfileName.value = "new-agent";
  showCreatePanel.value = true;
}

function cancelCreatePanel() {
  if (creating.value) {
    return;
  }
  showCreatePanel.value = false;
}

function openDeletePanel() {
  if (isBusy.value || !hasSelectedProfile.value) {
    return;
  }

  if (hasChanges.value) {
    emitToast({
      variant: "error",
      source: "agent-config",
      message: "当前有未保存内容，请先保存或撤销改动后再删除。",
    });
    return;
  }

  showCreatePanel.value = false;
  showDeletePanel.value = true;
}

function cancelDeletePanel() {
  if (deleting.value) {
    return;
  }
  showDeletePanel.value = false;
}

async function createAgentProfile() {
  if (creating.value) {
    return;
  }

  const nextName = newProfileName.value.trim();
  if (!nextName) {
    emitToast({
      variant: "error",
      source: "agent-config",
      message: "请输入智能体名称。",
    });
    return;
  }

  creating.value = true;
  try {
    const created = await invoke<AgentProfileMeta>("create_agent_profile", {
      name: nextName,
    });

    await loadAgentProfiles();
    if (created?.id) {
      selectedProfileId.value = created.id;
      await loadSelectedProfileContent();
    }

    showCreatePanel.value = false;
    emitToast({
      variant: "success",
      source: "agent-config",
      message: "已创建智能体。",
    });
  } catch (err) {
    emitToast({
      variant: "error",
      source: "agent-config",
      message: `创建智能体失败: ${String(err)}`,
    });
  } finally {
    creating.value = false;
  }
}

async function saveAgentMarkdown() {
  if (saving.value || !hasChanges.value || !hasSelectedProfile.value) {
    return;
  }

  saving.value = true;
  try {
    await invoke("save_agent_profile_markdown", {
      profileId: selectedProfileId.value,
      content: content.value,
    });
    originalContent.value = content.value;
    await loadAgentProfiles();

    emitToast({
      variant: "success",
      source: "agent-config",
      message: "智能体配置已保存。",
    });
  } catch (err) {
    emitToast({
      variant: "error",
      source: "agent-config",
      message: `保存智能体配置失败: ${String(err)}`,
    });
  } finally {
    saving.value = false;
  }
}

async function deleteSelectedProfile() {
  if (deleting.value || !hasSelectedProfile.value) {
    return;
  }

  deleting.value = true;
  try {
    const targetName = selectedProfileLabel.value;
    const targetId = selectedProfileId.value;

    await invoke("delete_agent_profile", {
      profileId: targetId,
    });

    showDeletePanel.value = false;
    await loadAgentProfiles();

    emitToast({
      variant: "success",
      source: "agent-config",
      message: `已删除智能体: ${targetName}`,
    });
  } catch (err) {
    emitToast({
      variant: "error",
      source: "agent-config",
      message: `删除智能体失败: ${String(err)}`,
    });
  } finally {
    deleting.value = false;
  }
}

function resetContent() {
  content.value = originalContent.value;
}

async function handleSelectProfile(profileId: string) {
  if (!profileId || profileId === selectedProfileId.value) {
    return;
  }

  if (hasChanges.value) {
    emitToast({
      variant: "error",
      source: "agent-config",
      message: "当前有未保存内容，请先保存或撤销改动后再切换。",
    });
    return;
  }

  selectedProfileId.value = profileId;
  await loadSelectedProfileContent();
}

onMounted(() => {
  void loadAgentProfiles();
});
</script>

<template>
  <div class="box-border flex h-full flex-col gap-4 overflow-auto bg-[#fcfcfb] px-5 pb-5 pt-[72px] dark:bg-transparent">
    <header class="flex flex-wrap items-start justify-between gap-3">
      <div class="space-y-1">
        <h2 class="text-base font-semibold text-[#2f2a24] dark:text-[#ece8de]">智能体配置</h2>
        <p class="text-sm text-[#8a8174] dark:text-[#b5ada0]">
          智能体列表保存在应用数据目录，支持按条目编辑 agent markdown。
        </p>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          class="bg-[#da7756] text-white hover:bg-[#c96c4d] focus-visible:ring-[#da7756]/35"
          :disabled="isBusy"
          @click="openCreatePanel"
        >
          添加智能体
        </Button>
        <Button
          variant="outline"
          size="sm"
          class="border-[#dcb3a4] bg-white text-[#9a3f28] hover:bg-[#fff4f1] dark:border-[#6f4338] dark:bg-[#2a2824] dark:text-[#e3a592] dark:hover:bg-[#3a2a25]"
          :disabled="isBusy || !hasSelectedProfile"
          @click="openDeletePanel"
        >
          删除智能体
        </Button>
        <Button
          variant="outline"
          size="sm"
          class="border-[#e3d8c7] bg-white text-[#5d5448] hover:bg-[#f6f1e8] dark:border-[#474136] dark:bg-[#2a2824] dark:text-[#d9d1c3] dark:hover:bg-[#34312b]"
          @click="emit('change-main-view', 'chat')"
        >
          返回聊天
        </Button>
        <Button
          variant="outline"
          size="sm"
          class="border-[#e3d8c7] bg-white text-[#5d5448] hover:bg-[#f6f1e8] dark:border-[#474136] dark:bg-[#2a2824] dark:text-[#d9d1c3] dark:hover:bg-[#34312b]"
          :disabled="isBusy"
          @click="loadAgentProfiles"
        >
          刷新
        </Button>
        <Button
          variant="outline"
          size="sm"
          class="border-[#e3d8c7] bg-white text-[#5d5448] hover:bg-[#f6f1e8] dark:border-[#474136] dark:bg-[#2a2824] dark:text-[#d9d1c3] dark:hover:bg-[#34312b]"
          :disabled="loadingContent || saving || !hasChanges"
          @click="resetContent"
        >
          撤销改动
        </Button>
        <Button
          size="sm"
          class="bg-[#da7756] text-white hover:bg-[#c96c4d] focus-visible:ring-[#da7756]/35"
          :disabled="loadingContent || saving || !hasChanges || !hasSelectedProfile"
          @click="saveAgentMarkdown"
        >
          {{ saving ? "保存中..." : "保存" }}
        </Button>
      </div>
    </header>

    <Card
      v-if="showCreatePanel"
      class="gap-3 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]"
    >
      <CardHeader class="space-y-1 px-4 pb-0">
        <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">创建智能体</CardTitle>
        <CardDescription>输入智能体名称，文件将保存到应用数据目录。</CardDescription>
      </CardHeader>
      <CardContent class="space-y-3 px-4">
        <Input
          v-model="newProfileName"
          class="border-[#ddd3c4] bg-white/95 text-[#2f2b24] focus-visible:border-[#d28a71] focus-visible:ring-[#da7756]/25 dark:border-[#4f473b] dark:bg-[#24221f] dark:text-[#e4dccd]"
          placeholder="例如: code-review-agent"
          :disabled="creating"
          @keydown.enter.prevent="createAgentProfile"
        />
        <div class="flex items-center justify-end gap-2">
          <Button variant="outline" size="sm" :disabled="creating" @click="cancelCreatePanel">
            取消
          </Button>
          <Button
            size="sm"
            class="bg-[#da7756] text-white hover:bg-[#c96c4d] focus-visible:ring-[#da7756]/35"
            :disabled="creating"
            @click="createAgentProfile"
          >
            {{ creating ? "创建中..." : "确认创建" }}
          </Button>
        </div>
      </CardContent>
    </Card>

    <Card
      v-if="showDeletePanel"
      class="gap-3 border-[#f1cfc5] bg-[#fff8f6] py-4 shadow-sm dark:border-[#5a3a33] dark:bg-[#332722]"
    >
      <CardHeader class="space-y-1 px-4 pb-0">
        <CardTitle class="text-sm text-[#8c3520] dark:text-[#e5ab9d]">确认删除智能体</CardTitle>
        <CardDescription>
          将删除 {{ selectedProfileLabel }} 对应的 markdown 文件，此操作不可恢复。
        </CardDescription>
      </CardHeader>
      <CardContent class="space-y-3 px-4">
        <div class="rounded-md border border-[#f0d4cc] bg-white px-3 py-2 text-xs text-[#9a4d3a] dark:border-[#5d4038] dark:bg-[#2d231f] dark:text-[#d9a898]">
          {{ selectedProfilePath || "未找到路径" }}
        </div>
        <div class="flex items-center justify-end gap-2">
          <Button variant="outline" size="sm" :disabled="deleting" @click="cancelDeletePanel">
            取消
          </Button>
          <Button
            variant="destructive"
            size="sm"
            :disabled="deleting"
            @click="deleteSelectedProfile"
          >
            {{ deleting ? "删除中..." : "确认删除" }}
          </Button>
        </div>
      </CardContent>
    </Card>

    <Card
      v-if="loadingList"
      class="gap-2 border-[#eadfcd] bg-[#fffdf8] py-4 dark:border-[#4a4237] dark:bg-[#292621]"
    >
      <CardContent class="px-4 text-sm text-[#8a8172] dark:text-[#b8b0a2]">正在读取智能体列表...</CardContent>
    </Card>

    <div v-else class="flex-1 min-h-[420px] grid grid-cols-[280px_minmax(0,1fr)] gap-3">
      <Card class="gap-3 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="space-y-1 px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">智能体列表</CardTitle>
          <CardDescription>
            共 {{ profiles.length }} 个智能体
          </CardDescription>
        </CardHeader>

        <CardContent class="px-3">
          <div class="max-h-[calc(100vh-280px)] space-y-1 overflow-y-auto pr-1 custom-scrollbar">
            <Button
              v-for="item in profiles"
              :key="item.id"
              variant="ghost"
              class="h-auto w-full justify-start border px-2.5 py-2 text-left"
              :class="item.id === selectedProfileId
                ? 'border-[#d8a08a] bg-[#fff2ed] text-[#3b3229] hover:bg-[#fff2ed] dark:border-[#a36f5d] dark:bg-[#3b2a24] dark:text-[#ece1d4] dark:hover:bg-[#3b2a24]'
                : 'border-transparent text-[#5d5448] hover:border-[#e6dccb] hover:bg-[#f7f2e9] dark:text-[#cfc6b8] dark:hover:border-[#4a4237] dark:hover:bg-[#2f2a24]'"
              @click="handleSelectProfile(item.id)"
            >
              <div class="w-full">
                <div class="truncate text-[13px] font-medium">{{ item.name }}</div>
                <div class="truncate text-[11px] opacity-75">{{ formatUpdatedAt(item.updatedAt) }}</div>
              </div>
            </Button>

            <div
              v-if="!hasProfiles"
              class="rounded-md border border-dashed border-[#e4d8c8] px-3 py-4 text-xs text-[#9a9184] dark:border-[#4a4237] dark:text-[#9f978b]"
            >
              暂无智能体，点击上方“添加智能体”。
            </div>
          </div>
        </CardContent>
      </Card>

      <Card class="gap-3 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="space-y-1 px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">{{ selectedProfileLabel }}</CardTitle>
          <CardDescription v-if="selectedProfilePath" class="break-all">
            {{ selectedProfilePath }}
          </CardDescription>
          <CardDescription v-if="hasChanges" class="text-[#bf5f44] dark:text-[#e2a08c]">
            当前有未保存改动
          </CardDescription>
        </CardHeader>

        <CardContent class="h-full px-4">
          <div
            v-if="loadingContent"
            class="flex h-full min-h-[440px] items-center justify-center text-sm text-[#8a8174] dark:text-[#b5ada0]"
          >
            正在读取智能体配置...
          </div>

          <div
            v-else-if="!hasSelectedProfile"
            class="flex h-full min-h-[440px] items-center justify-center text-sm text-[#8a8174] dark:text-[#b5ada0]"
          >
            请选择或创建一个智能体。
          </div>

          <Textarea
            v-else
            v-model="content"
            class="min-h-[440px] w-full resize-y border-[#ddd3c4] bg-white/95 font-mono text-[13px] leading-6 text-[#2f2b24] focus-visible:border-[#d28a71] focus-visible:ring-[#da7756]/25 dark:border-[#4f473b] dark:bg-[#24221f] dark:text-[#e4dccd]"
            spellcheck="false"
            placeholder="# Agent\n\n在这里编写智能体配置..."
          />
        </CardContent>
      </Card>
    </div>
  </div>
</template>
