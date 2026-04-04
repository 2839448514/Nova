<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { computed, onMounted, reactive, ref } from "vue";
import { emitToast } from "../../lib/toast";

type MainView = "chat" | "hooks";

const emit = defineEmits<{
  (e: "change-main-view", view: MainView): void;
}>();

const loading = ref(false);
const saving = ref(false);
const lastSavedAt = ref<number | null>(null);

const form = reactive({
  preToolDenyTools: "",
  preToolContext: "",
  postToolContext: "",
  postToolStopOnError: false,
  postToolBlockPattern: "",
  postToolFailureContext: "",
  postToolFailureStop: false,
  stopHookMaxAssistantMessages: "",
  stopHookBlockPattern: "",
  stopHookAppendContext: "",
});

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
  form.preToolDenyTools = hookEnv.NOVA_PRE_TOOL_DENY_TOOLS ?? "";
  form.preToolContext = hookEnv.NOVA_PRE_TOOL_CONTEXT ?? "";
  form.postToolContext = hookEnv.NOVA_POST_TOOL_CONTEXT ?? "";
  form.postToolStopOnError = isTruthy(hookEnv.NOVA_POST_TOOL_STOP_ON_ERROR);
  form.postToolBlockPattern = hookEnv.NOVA_POST_TOOL_BLOCK_PATTERN ?? "";
  form.postToolFailureContext = hookEnv.NOVA_POST_TOOL_FAILURE_CONTEXT ?? "";
  form.postToolFailureStop = isTruthy(hookEnv.NOVA_POST_TOOL_FAILURE_STOP);
  form.stopHookMaxAssistantMessages = hookEnv.NOVA_STOP_HOOK_MAX_ASSISTANT_MESSAGES ?? "";
  form.stopHookBlockPattern = hookEnv.NOVA_STOP_HOOK_BLOCK_PATTERN ?? "";
  form.stopHookAppendContext = hookEnv.NOVA_STOP_HOOK_APPEND_CONTEXT ?? "";
}

function buildHookEnvFromForm(): Record<string, string> {
  const next: Record<string, string> = {};

  const put = (key: string, value: string) => {
    const trimmed = value.trim();
    if (trimmed) {
      next[key] = trimmed;
    }
  };

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
  put("NOVA_STOP_HOOK_MAX_ASSISTANT_MESSAGES", form.stopHookMaxAssistantMessages);
  put("NOVA_STOP_HOOK_BLOCK_PATTERN", form.stopHookBlockPattern);
  put("NOVA_STOP_HOOK_APPEND_CONTEXT", form.stopHookAppendContext);

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

const savedAtText = computed(() => {
  if (!lastSavedAt.value) return "";
  return `已保存: ${new Date(lastSavedAt.value).toLocaleTimeString()}`;
});

onMounted(() => {
  loadHookConfig();
});
</script>

<template>
  <div class="hooks-shell">
    <header class="hooks-header">
      <div>
        <h2 class="hooks-title">挂钩配置</h2>
        <p class="hooks-subtitle">管理 PreToolUse / PostToolUse / StopHook 的行为开关与上下文注入。</p>
      </div>
      <div class="hooks-header-actions">
        <button class="btn ghost" @click="emit('change-main-view', 'chat')">返回聊天</button>
        <button class="btn ghost" :disabled="loading || saving" @click="loadHookConfig">刷新</button>
        <button class="btn" :disabled="loading || saving" @click="saveHookConfig">{{ saving ? '保存中...' : '保存配置' }}</button>
      </div>
    </header>

    <section v-if="loading" class="hooks-loading">正在读取配置...</section>

    <div v-else class="hooks-grid">
      <section class="card">
        <h3>PreToolUse</h3>
        <label>
          <span>禁用工具列表</span>
          <input v-model="form.preToolDenyTools" placeholder="例如: execute_bash,write_file" />
          <small>对应 NOVA_PRE_TOOL_DENY_TOOLS，逗号分隔，名称按小写匹配。</small>
        </label>
        <label>
          <span>注入上下文</span>
          <textarea v-model="form.preToolContext" rows="3" placeholder="进入工具执行前追加的提示内容" />
          <small>对应 NOVA_PRE_TOOL_CONTEXT。</small>
        </label>
      </section>

      <section class="card">
        <h3>PostToolUse</h3>
        <label>
          <span>注入上下文</span>
          <textarea v-model="form.postToolContext" rows="3" placeholder="工具执行后追加的提示内容" />
          <small>对应 NOVA_POST_TOOL_CONTEXT。</small>
        </label>
        <label class="checkbox">
          <input v-model="form.postToolStopOnError" type="checkbox" />
          <span>工具报错时终止续跑</span>
        </label>
        <small class="row-help">对应 NOVA_POST_TOOL_STOP_ON_ERROR。</small>
        <label>
          <span>输出拦截关键字</span>
          <input v-model="form.postToolBlockPattern" placeholder="命中该文本即停止续跑" />
          <small>对应 NOVA_POST_TOOL_BLOCK_PATTERN。</small>
        </label>
      </section>

      <section class="card">
        <h3>PostToolUseFailure</h3>
        <label>
          <span>失败上下文</span>
          <textarea v-model="form.postToolFailureContext" rows="3" placeholder="工具失败后追加的提示内容" />
          <small>对应 NOVA_POST_TOOL_FAILURE_CONTEXT。</small>
        </label>
        <label class="checkbox">
          <input v-model="form.postToolFailureStop" type="checkbox" />
          <span>失败后直接终止续跑</span>
        </label>
        <small class="row-help">对应 NOVA_POST_TOOL_FAILURE_STOP。</small>
      </section>

      <section class="card">
        <h3>StopHook</h3>
        <label>
          <span>最大 Assistant 消息数</span>
          <input v-model="form.stopHookMaxAssistantMessages" inputmode="numeric" placeholder="例如: 12" />
          <small>对应 NOVA_STOP_HOOK_MAX_ASSISTANT_MESSAGES。</small>
        </label>
        <label>
          <span>停止关键字</span>
          <input v-model="form.stopHookBlockPattern" placeholder="命中 assistant 文本即终止" />
          <small>对应 NOVA_STOP_HOOK_BLOCK_PATTERN。</small>
        </label>
        <label>
          <span>附加上下文</span>
          <textarea v-model="form.stopHookAppendContext" rows="3" placeholder="回合结束前追加的上下文" />
          <small>对应 NOVA_STOP_HOOK_APPEND_CONTEXT。</small>
        </label>
      </section>
    </div>

    <footer class="hooks-footer">
      <button class="btn ghost" :disabled="loading || saving" @click="resetHookConfig">清空表单</button>
      <span class="saved-at">{{ savedAtText }}</span>
    </footer>
  </div>
</template>

<style scoped>
.hooks-shell {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 72px 22px 20px;
  box-sizing: border-box;
  overflow: auto;
  gap: 14px;
}

.hooks-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.hooks-title {
  margin: 0;
  font-size: 1.1rem;
  font-weight: 700;
}

.hooks-subtitle {
  margin: 6px 0 0;
  color: #7f7a70;
  font-size: 0.9rem;
}

.hooks-header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.hooks-loading {
  border: 1px solid #e7e2d8;
  border-radius: 12px;
  padding: 16px;
  color: #7f7a70;
  background: #fffdfa;
}

.hooks-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(280px, 1fr));
  gap: 12px;
}

.card {
  border: 1px solid #e8e2d8;
  border-radius: 14px;
  padding: 14px;
  background: #fffdfa;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.card h3 {
  margin: 0;
  font-size: 0.95rem;
  font-weight: 700;
}

label {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 0.86rem;
}

input,
textarea {
  border: 1px solid #d9d0c1;
  border-radius: 10px;
  padding: 8px 10px;
  font-size: 0.86rem;
  background: #ffffff;
  color: #24211d;
}

textarea {
  resize: vertical;
}

small,
.row-help {
  color: #857a69;
  font-size: 0.75rem;
}

.checkbox {
  flex-direction: row;
  align-items: center;
  gap: 8px;
}

.checkbox input {
  width: 16px;
  height: 16px;
  margin: 0;
}

.hooks-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.saved-at {
  color: #7f7a70;
  font-size: 0.8rem;
}

.btn {
  border: 1px solid #b8a98f;
  border-radius: 10px;
  padding: 7px 12px;
  font-size: 0.82rem;
  font-weight: 600;
  background: #8c6a3d;
  color: #fff;
}

.btn.ghost {
  background: #fff;
  color: #4f4434;
  border-color: #d8ccba;
}

.btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

@media (max-width: 980px) {
  .hooks-grid {
    grid-template-columns: 1fr;
  }
}
</style>
