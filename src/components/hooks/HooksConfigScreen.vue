<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { computed, onMounted, reactive, ref } from "vue";
import { emitToast } from "../../lib/toast";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

type MainView = "chat" | "hooks";

const emit = defineEmits<{
  (e: "change-main-view", view: MainView): void;
}>();

const loading = ref(false);
const saving = ref(false);
const lastSavedAt = ref<number | null>(null);

const form = reactive({
  sessionStartContext: "",
  userPromptSubmitContext: "",
  preCompactContext: "",
  preToolDenyTools: "",
  preToolContext: "",
  postToolContext: "",
  postToolStopOnError: false,
  postToolBlockPattern: "",
  postToolFailureContext: "",
  postToolFailureStop: false,
  subagentStartContext: "",
  subagentStopContext: "",
  stopHookMaxAssistantMessages: "",
  stopHookBlockPattern: "",
  stopHookAppendContext: "",
  sessionEndContext: "",
  errorContext: "",
});

const fieldClass =
  "border-[#ddd3c4] bg-white/95 text-[#2f2b24] focus-visible:border-[#d28a71] focus-visible:ring-[#da7756]/25 dark:border-[#4f473b] dark:bg-[#24221f] dark:text-[#e4dccd] dark:focus-visible:border-[#b77a63]";
const labelClass = "text-[0.86rem] text-[#5f574a] dark:text-[#d8cfbf]";
const hintClass = "text-xs text-[#8a8172] dark:text-[#b8b0a2]";

function validateStopHookMaxAssistantMessages(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  if (!/^\d+$/.test(trimmed)) {
    return "最大 Assistant 消息数仅支持非负整数。";
  }

  const parsed = Number.parseInt(trimmed, 10);
  if (!Number.isSafeInteger(parsed)) {
    return "数值过大，请输入较小的整数。";
  }

  return null;
}

function isTruthy(value: unknown): boolean {
  if (typeof value !== "string") return false;
  const normalized = value.trim().toLowerCase();
  return normalized === "1" || normalized === "true" || normalized === "yes" || normalized === "on";
}

function extractHookEnv(settings: Record<string, unknown>): Record<string, string> {
  const fromCamel = settings.hookEnv;
  if (fromCamel && typeof fromCamel === "object") {
    return fromCamel as Record<string, string>;
  }

  const fromSnake = settings.hook_env;
  if (fromSnake && typeof fromSnake === "object") {
    return fromSnake as Record<string, string>;
  }

  return {};
}

function applyHookEnvToForm(hookEnv: Record<string, string>) {
  form.sessionStartContext = hookEnv.NOVA_SESSION_START_HOOK_CONTEXT ?? "";
  form.userPromptSubmitContext = hookEnv.NOVA_USER_PROMPT_SUBMIT_HOOK_CONTEXT ?? "";
  form.preCompactContext = hookEnv.NOVA_PRE_COMPACT_HOOK_CONTEXT ?? "";
  form.preToolDenyTools = hookEnv.NOVA_PRE_TOOL_DENY_TOOLS ?? "";
  form.preToolContext = hookEnv.NOVA_PRE_TOOL_CONTEXT ?? "";
  form.postToolContext = hookEnv.NOVA_POST_TOOL_CONTEXT ?? "";
  form.postToolStopOnError = isTruthy(hookEnv.NOVA_POST_TOOL_STOP_ON_ERROR);
  form.postToolBlockPattern = hookEnv.NOVA_POST_TOOL_BLOCK_PATTERN ?? "";
  form.postToolFailureContext = hookEnv.NOVA_POST_TOOL_FAILURE_CONTEXT ?? "";
  form.postToolFailureStop = isTruthy(hookEnv.NOVA_POST_TOOL_FAILURE_STOP);
  form.subagentStartContext = hookEnv.NOVA_SUBAGENT_START_HOOK_CONTEXT ?? "";
  form.subagentStopContext = hookEnv.NOVA_SUBAGENT_STOP_HOOK_CONTEXT ?? "";
  form.stopHookMaxAssistantMessages = hookEnv.NOVA_STOP_HOOK_MAX_ASSISTANT_MESSAGES ?? "";
  form.stopHookBlockPattern = hookEnv.NOVA_STOP_HOOK_BLOCK_PATTERN ?? "";
  form.stopHookAppendContext = hookEnv.NOVA_STOP_HOOK_APPEND_CONTEXT ?? "";
  form.sessionEndContext = hookEnv.NOVA_SESSION_END_HOOK_CONTEXT ?? "";
  form.errorContext = hookEnv.NOVA_ERROR_HOOK_CONTEXT ?? "";
}

function buildHookEnvFromForm(): Record<string, string> {
  const next: Record<string, string> = {};

  const put = (key: string, value: string) => {
    const trimmed = value.trim();
    if (trimmed) {
      next[key] = trimmed;
    }
  };

  put("NOVA_SESSION_START_HOOK_CONTEXT", form.sessionStartContext);
  put("NOVA_USER_PROMPT_SUBMIT_HOOK_CONTEXT", form.userPromptSubmitContext);
  put("NOVA_PRE_COMPACT_HOOK_CONTEXT", form.preCompactContext);
  put("NOVA_PRE_TOOL_DENY_TOOLS", form.preToolDenyTools);
  put("NOVA_PRE_TOOL_CONTEXT", form.preToolContext);
  put("NOVA_POST_TOOL_CONTEXT", form.postToolContext);
  if (form.postToolStopOnError) {
    next.NOVA_POST_TOOL_STOP_ON_ERROR = "true";
  }
  put("NOVA_POST_TOOL_BLOCK_PATTERN", form.postToolBlockPattern);
  put("NOVA_POST_TOOL_FAILURE_CONTEXT", form.postToolFailureContext);
  if (form.postToolFailureStop) {
    next.NOVA_POST_TOOL_FAILURE_STOP = "true";
  }
  put("NOVA_SUBAGENT_START_HOOK_CONTEXT", form.subagentStartContext);
  put("NOVA_SUBAGENT_STOP_HOOK_CONTEXT", form.subagentStopContext);
  put("NOVA_STOP_HOOK_MAX_ASSISTANT_MESSAGES", form.stopHookMaxAssistantMessages);
  put("NOVA_STOP_HOOK_BLOCK_PATTERN", form.stopHookBlockPattern);
  put("NOVA_STOP_HOOK_APPEND_CONTEXT", form.stopHookAppendContext);
  put("NOVA_SESSION_END_HOOK_CONTEXT", form.sessionEndContext);
  put("NOVA_ERROR_HOOK_CONTEXT", form.errorContext);

  return next;
}

async function loadHookConfig() {
  loading.value = true;
  try {
    const settings = (await invoke("get_settings")) as Record<string, unknown>;
    const hookEnv = extractHookEnv(settings ?? {});
    applyHookEnvToForm(hookEnv);
  } catch (err) {
    emitToast({
      variant: "error",
      source: "hooks",
      message: `读取钩子配置失败: ${String(err)}`,
    });
  } finally {
    loading.value = false;
  }
}

async function saveHookConfig() {
  const validationError = stopHookMaxAssistantMessagesError.value;
  if (validationError) {
    emitToast({
      variant: "error",
      source: "hooks",
      message: validationError,
    });
    return;
  }

  saving.value = true;
  try {
    const settings = (await invoke("get_settings")) as Record<string, unknown>;
    const nextSettings = {
      ...(settings ?? {}),
      hookEnv: buildHookEnvFromForm(),
    };
    await invoke("save_settings", { settings: nextSettings });

    lastSavedAt.value = Date.now();
    emitToast({
      variant: "success",
      source: "hooks",
      message: "钩子配置已保存并生效。",
    });
  } catch (err) {
    emitToast({
      variant: "error",
      source: "hooks",
      message: `保存钩子配置失败: ${String(err)}`,
    });
  } finally {
    saving.value = false;
  }
}

function resetHookConfig() {
  applyHookEnvToForm({});
}

function onPostToolStopOnErrorChange(value: boolean | "indeterminate") {
  form.postToolStopOnError = value === true;
}

function onPostToolFailureStopChange(value: boolean | "indeterminate") {
  form.postToolFailureStop = value === true;
}

const savedAtText = computed(() => {
  if (!lastSavedAt.value) return "";
  return `已保存: ${new Date(lastSavedAt.value).toLocaleTimeString()}`;
});

const stopHookMaxAssistantMessagesError = computed(() =>
  validateStopHookMaxAssistantMessages(form.stopHookMaxAssistantMessages),
);

onMounted(() => {
  loadHookConfig();
});
</script>

<template>
  <div class="box-border flex h-full flex-col gap-4 overflow-auto bg-[#fcfcfb] px-5 pb-5 pt-[72px] dark:bg-transparent">
    <header class="flex flex-wrap items-start justify-between gap-3">
      <div class="space-y-1">
        <h2 class="text-base font-semibold text-[#2f2a24] dark:text-[#ece8de]">挂钩配置</h2>
        <p class="text-sm text-[#8a8174] dark:text-[#b5ada0]">管理会话、提示提交、预压缩、工具前后、子智能体、停止与错误等全流程 Hook。</p>
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
          :disabled="loading || saving"
          @click="loadHookConfig"
        >
          刷新
        </Button>
        <Button
          size="sm"
          class="bg-[#da7756] text-white hover:bg-[#c96c4d] focus-visible:ring-[#da7756]/35 disabled:bg-[#e4b2a1] dark:bg-[#c56c4d] dark:hover:bg-[#b35f43]"
          :disabled="loading || saving || !!stopHookMaxAssistantMessagesError"
          @click="saveHookConfig"
        >
          {{ saving ? '保存中...' : '保存配置' }}
        </Button>
      </div>
    </header>

    <Card v-if="loading" class="gap-2 border-[#eadfcd] bg-[#fffdf8] py-4 dark:border-[#4a4237] dark:bg-[#292621]">
      <CardContent class="px-4 text-sm text-[#8a8172] dark:text-[#b8b0a2]">正在读取配置...</CardContent>
    </Card>

    <div v-else class="grid grid-cols-1 gap-3 xl:grid-cols-2">
      <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">会话开始</CardTitle>
        </CardHeader>
        <CardContent class="space-y-2 px-4">
          <Label :class="labelClass">上下文注入</Label>
          <Textarea v-model="form.sessionStartContext" :class="fieldClass" rows="3" placeholder="新的会话开始时追加的上下文" />
          <p :class="hintClass">对应 NOVA_SESSION_START_HOOK_CONTEXT。</p>
        </CardContent>
      </Card>

      <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">用户提示提交</CardTitle>
        </CardHeader>
        <CardContent class="space-y-2 px-4">
          <Label :class="labelClass">上下文注入</Label>
          <Textarea v-model="form.userPromptSubmitContext" :class="fieldClass" rows="3" placeholder="每次用户提交提示时追加的上下文" />
          <p :class="hintClass">对应 NOVA_USER_PROMPT_SUBMIT_HOOK_CONTEXT。</p>
        </CardContent>
      </Card>

      <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">预压缩</CardTitle>
        </CardHeader>
        <CardContent class="space-y-2 px-4">
          <Label :class="labelClass">上下文注入</Label>
          <Textarea v-model="form.preCompactContext" :class="fieldClass" rows="3" placeholder="压缩上下文前追加的提示" />
          <p :class="hintClass">对应 NOVA_PRE_COMPACT_HOOK_CONTEXT。</p>
        </CardContent>
      </Card>

      <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">PreToolUse</CardTitle>
        </CardHeader>
        <CardContent class="space-y-4 px-4">
          <div class="space-y-2">
            <Label :class="labelClass">禁用工具列表</Label>
            <Input v-model="form.preToolDenyTools" :class="fieldClass" placeholder="例如: execute_bash,write_file" />
            <p :class="hintClass">对应 NOVA_PRE_TOOL_DENY_TOOLS，逗号分隔，名称按小写匹配。</p>
          </div>
          <div class="space-y-2">
            <Label :class="labelClass">注入上下文</Label>
            <Textarea v-model="form.preToolContext" :class="fieldClass" rows="3" placeholder="进入工具执行前追加的提示内容" />
            <p :class="hintClass">对应 NOVA_PRE_TOOL_CONTEXT。</p>
          </div>
        </CardContent>
      </Card>

      <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">PostToolUse</CardTitle>
        </CardHeader>
        <CardContent class="space-y-4 px-4">
          <div class="space-y-2">
            <Label :class="labelClass">注入上下文</Label>
            <Textarea v-model="form.postToolContext" :class="fieldClass" rows="3" placeholder="工具执行后追加的提示内容" />
            <p :class="hintClass">对应 NOVA_POST_TOOL_CONTEXT。</p>
          </div>

          <div class="space-y-2">
            <div class="flex items-center gap-2">
              <Checkbox
                id="post-tool-stop-on-error"
                class="border-[#c8baa3] data-[state=checked]:border-[#da7756] data-[state=checked]:bg-[#da7756]"
                :model-value="form.postToolStopOnError"
                @update:model-value="onPostToolStopOnErrorChange"
              />
              <Label for="post-tool-stop-on-error" class="text-[0.86rem] font-normal text-[#5f574a] dark:text-[#d8cfbf]">工具报错时终止续跑</Label>
            </div>
            <p :class="hintClass">对应 NOVA_POST_TOOL_STOP_ON_ERROR。</p>
          </div>

          <div class="space-y-2">
            <Label :class="labelClass">输出拦截关键字</Label>
            <Input v-model="form.postToolBlockPattern" :class="fieldClass" placeholder="命中该文本即停止续跑" />
            <p :class="hintClass">对应 NOVA_POST_TOOL_BLOCK_PATTERN。</p>
          </div>
        </CardContent>
      </Card>

      <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">PostToolUseFailure</CardTitle>
        </CardHeader>
        <CardContent class="space-y-4 px-4">
          <div class="space-y-2">
            <Label :class="labelClass">失败上下文</Label>
            <Textarea v-model="form.postToolFailureContext" :class="fieldClass" rows="3" placeholder="工具失败后追加的提示内容" />
            <p :class="hintClass">对应 NOVA_POST_TOOL_FAILURE_CONTEXT。</p>
          </div>

          <div class="space-y-2">
            <div class="flex items-center gap-2">
              <Checkbox
                id="post-tool-failure-stop"
                class="border-[#c8baa3] data-[state=checked]:border-[#da7756] data-[state=checked]:bg-[#da7756]"
                :model-value="form.postToolFailureStop"
                @update:model-value="onPostToolFailureStopChange"
              />
              <Label for="post-tool-failure-stop" class="text-[0.86rem] font-normal text-[#5f574a] dark:text-[#d8cfbf]">失败后直接终止续跑</Label>
            </div>
            <p :class="hintClass">对应 NOVA_POST_TOOL_FAILURE_STOP。</p>
          </div>
        </CardContent>
      </Card>

      <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">StopHook</CardTitle>
        </CardHeader>
        <CardContent class="space-y-4 px-4">
          <div class="space-y-2">
            <Label :class="labelClass">最大 Assistant 消息数</Label>
            <Input
              v-model="form.stopHookMaxAssistantMessages"
              inputmode="numeric"
              placeholder="例如: 12"
              :class="[
                fieldClass,
                stopHookMaxAssistantMessagesError
                  ? 'border-destructive focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40'
                  : '',
              ]"
            />
            <p :class="hintClass">对应 NOVA_STOP_HOOK_MAX_ASSISTANT_MESSAGES，留空表示不启用限制。</p>
            <p v-if="stopHookMaxAssistantMessagesError" class="text-xs text-destructive">{{ stopHookMaxAssistantMessagesError }}</p>
          </div>

          <div class="space-y-2">
            <Label :class="labelClass">停止关键字</Label>
            <Input v-model="form.stopHookBlockPattern" :class="fieldClass" placeholder="命中 assistant 文本即终止" />
            <p :class="hintClass">对应 NOVA_STOP_HOOK_BLOCK_PATTERN。</p>
          </div>

          <div class="space-y-2">
            <Label :class="labelClass">附加上下文</Label>
            <Textarea v-model="form.stopHookAppendContext" :class="fieldClass" rows="3" placeholder="回合结束前追加的上下文" />
            <p :class="hintClass">对应 NOVA_STOP_HOOK_APPEND_CONTEXT。</p>
          </div>
        </CardContent>
      </Card>

      <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">子智能体启动</CardTitle>
        </CardHeader>
        <CardContent class="space-y-2 px-4">
          <Label :class="labelClass">上下文注入</Label>
          <Textarea v-model="form.subagentStartContext" :class="fieldClass" rows="3" placeholder="子智能体启动时追加上下文" />
          <p :class="hintClass">对应 NOVA_SUBAGENT_START_HOOK_CONTEXT。</p>
        </CardContent>
      </Card>

      <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">子智能体停止</CardTitle>
        </CardHeader>
        <CardContent class="space-y-2 px-4">
          <Label :class="labelClass">上下文注入</Label>
          <Textarea v-model="form.subagentStopContext" :class="fieldClass" rows="3" placeholder="子智能体停止时追加上下文" />
          <p :class="hintClass">对应 NOVA_SUBAGENT_STOP_HOOK_CONTEXT。</p>
        </CardContent>
      </Card>

      <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">会话结束</CardTitle>
        </CardHeader>
        <CardContent class="space-y-2 px-4">
          <Label :class="labelClass">结束原因附加文本</Label>
          <Textarea v-model="form.sessionEndContext" :class="fieldClass" rows="3" placeholder="会话结束时附加到 stop reason" />
          <p :class="hintClass">对应 NOVA_SESSION_END_HOOK_CONTEXT。</p>
        </CardContent>
      </Card>

      <Card class="gap-4 border-[#eadfcd] bg-[#fffdf8] py-4 shadow-sm dark:border-[#4a4237] dark:bg-[#292621]">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm text-[#5b5347] dark:text-[#ddd5c7]">出错</CardTitle>
        </CardHeader>
        <CardContent class="space-y-2 px-4">
          <Label :class="labelClass">错误附加文本</Label>
          <Textarea v-model="form.errorContext" :class="fieldClass" rows="3" placeholder="发生错误时附加到错误信息" />
          <p :class="hintClass">对应 NOVA_ERROR_HOOK_CONTEXT。</p>
        </CardContent>
      </Card>
    </div>

    <footer class="flex flex-wrap items-center justify-between gap-2">
      <Button
        variant="outline"
        size="sm"
        class="border-[#dfd4c3] bg-white text-[#5d5448] hover:bg-[#f6f1e8] dark:border-[#474136] dark:bg-[#2a2824] dark:text-[#d9d1c3] dark:hover:bg-[#34312b]"
        :disabled="loading || saving"
        @click="resetHookConfig"
      >
        清空表单
      </Button>
      <span class="text-xs text-[#8a8172] dark:text-[#b8b0a2]">{{ savedAtText }}</span>
    </footer>
  </div>
</template>
