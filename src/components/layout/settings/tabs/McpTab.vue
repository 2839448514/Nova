<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

type MCPServerConfig = { type: 'stdio'; command: string; args: string[]; env?: Record<string, string> } | { type: 'sse'; url: string }
type ServerStatus = { name: string; status: 'connected' | 'error' | 'connecting' | 'disconnected'; type: 'stdio' | 'sse'; enabled: boolean; toolCount?: number; error?: string }
type MCPForm = { name: string; type: 'stdio' | 'sse'; command: string; args: string; env: string; url: string }
type ToastItem = { id: number; message: string; variant: 'error' | 'success' }

const addServer = async (name: string, config: MCPServerConfig) => {
  await invoke('add_mcp_server', { name, config })
}
const removeServer = async (name: string) => {
  await invoke('remove_mcp_server', { name })
}
const getServerStatuses = async (): Promise<ServerStatus[]> => {
  return await invoke('get_mcp_server_statuses')
}
const reloadAllServers = async () => {
  await invoke('reload_all_mcp_servers')
}
const setServerEnabled = async (name: string, enabled: boolean) => {
  await invoke('set_mcp_server_enabled', { name, enabled })
}

const servers = ref<ServerStatus[]>([])
const loading = ref(false)
const adding = ref(false)
const reloading = ref(false)
const error = ref('')
const toasts = ref<ToastItem[]>([])
const showForm = ref(false)
const removingName = ref<string | null>(null)
const togglingName = ref<string | null>(null)
const form = ref<MCPForm>({ name: '', type: 'stdio', command: 'npx', args: '-y @playwright/mcp@latest', env: '', url: '' })

const pushToast = (message: string, variant: ToastItem['variant']) => {
  const id = Date.now() + Math.floor(Math.random() * 1000)
  toasts.value.push({ id, message, variant })
  window.setTimeout(() => {
    toasts.value = toasts.value.filter((t) => t.id !== id)
  }, 3500)
}

const resetForm = () => {
  form.value = { name: '', type: 'stdio', command: 'npx', args: '-y @playwright/mcp@latest', env: '', url: '' }
  error.value = ''
  showForm.value = false
}

const refresh = async () => {
  loading.value = true
  try { servers.value = await getServerStatuses() }
  catch (e) {
    servers.value = []
    pushToast(`MCP 加载失败: ${String(e)}`, 'error')
  }
  finally { loading.value = false }
}

const submit = async () => {
  const name = form.value.name.trim()
  if (!name) { error.value = '请填写名称'; return }
  let config: MCPServerConfig
  if (form.value.type === 'stdio') {
    if (!form.value.command.trim()) { error.value = '请填写命令'; return }
    const args = form.value.args.trim() ? form.value.args.trim().split(/\s+/) : []
    const env: Record<string, string> = {}
    form.value.env.trim().split('\n').forEach(line => {
      const eq = line.indexOf('=')
      if (eq > 0) env[line.slice(0, eq).trim()] = line.slice(eq + 1).trim()
    })
    config = { type: 'stdio', command: form.value.command.trim(), args, ...(Object.keys(env).length ? { env } : {}) }
  } else {
    if (!form.value.url.trim()) { error.value = '请填写 URL'; return }
    config = { type: 'sse', url: form.value.url.trim() }
  }
  adding.value = true; error.value = ''
  try {
    await addServer(name, config)
    resetForm()
    await refresh()
    pushToast('MCP 服务已添加并触发连接。', 'success')
  }
  catch (e) {
    const msg = `添加失败: ${String(e)}`
    error.value = msg
    pushToast(msg, 'error')
  }
  finally { adding.value = false }
}

const handleRemove = async (name: string) => {
  removingName.value = name
  try {
    await removeServer(name)
    await refresh()
    pushToast(`已删除 MCP 服务: ${name}`, 'success')
  }
  catch (e) {
    pushToast(`删除失败(${name}): ${String(e)}`, 'error')
  }
  finally { removingName.value = null }
}

const handleReload = async () => {
  reloading.value = true
  try {
    await reloadAllServers()
    await refresh()
    pushToast('MCP 服务重连完成。', 'success')
  }
  catch (e) {
    pushToast(`MCP 重连失败: ${String(e)}`, 'error')
  }
  finally { reloading.value = false }
}

const handleToggleEnabled = async (name: string, enabled: boolean) => {
  togglingName.value = name
  try {
    await setServerEnabled(name, enabled)
    await refresh()
    pushToast(`${enabled ? '已启用' : '已停用'} MCP 服务: ${name}`, 'success')
  }
  catch (e) {
    pushToast(`更新状态失败(${name}): ${String(e)}`, 'error')
  }
  finally { togglingName.value = null }
}

defineExpose({ refresh })
refresh()
</script>

<template>
  <div class="px-6 py-4 flex flex-col h-full overflow-y-auto">
    <TransitionGroup
      name="mcp-toast"
      tag="div"
      class="fixed top-5 right-5 z-[80] flex flex-col gap-2 pointer-events-none"
    >
      <div
        v-for="toast in toasts"
        :key="toast.id"
        class="min-w-[260px] max-w-[360px] px-4 py-3 rounded-lg border shadow-[0_8px_20px_rgba(0,0,0,0.12)] text-[13px] leading-relaxed pointer-events-auto"
        :class="toast.variant === 'error'
          ? 'bg-[#fff4f4] dark:bg-[#3a2222] border-[#f2c9c9] dark:border-[#6a3535] text-[#9f2f2f] dark:text-[#ffb3b3]'
          : 'bg-[#f2fbf4] dark:bg-[#1f3325] border-[#cde8d3] dark:border-[#3a6b48] text-[#1f6a34] dark:text-[#9ae2ad]'"
      >
        {{ toast.message }}
      </div>
    </TransitionGroup>

    <div class="flex items-center justify-between mb-4">
      <span class="text-[12.5px] text-[#aaa49a] dark:text-[#88857f]">{{ servers.length }} 个服务</span>
      <div class="flex items-center gap-2">
        <button class="flex items-center gap-[6px] h-[36px] px-[12px] bg-transparent border-none rounded-lg text-[13px] font-medium text-[#6b6456] dark:text-[#a09e99] cursor-pointer transition-colors duration-150 hover:bg-[#f5f4f0] dark:hover:bg-[#32312e] hover:text-[#1a1915] dark:hover:text-[#e8e3db] disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none" :disabled="reloading" @click="handleReload">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-[15px] h-[15px]" :class="{ 'animate-spin': reloading }">
            <path d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
          重新连接
        </button>
        <button class="h-[36px] px-[16px] bg-[#da7756] text-white border-none rounded-lg font-medium text-[13px] cursor-pointer shadow-[0_1px_2px_rgba(0,0,0,0.05)] transition-colors duration-150 hover:bg-[#c06548] focus:outline-none" @click="showForm = !showForm">
          {{ showForm ? '取消' : '+ 添加' }}
        </button>
      </div>
    </div>

    <div v-if="showForm" class="bg-[#faf9f7] dark:bg-[#252422] border border-[#ebe9e3] dark:border-[#3b3a37] rounded-xl p-4 mb-4 flex flex-col transition-all overflow-hidden origin-top">
      <div class="flex gap-3 items-end mb-2.5">
        <div class="flex-1 flex flex-col text-[14px]">
          <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">名称</label>
          <input v-model="form.name" placeholder="filesystem" class="w-full h-9 px-3 text-[14px] bg-white dark:bg-[#2e2d2a] border border-[#e8e3db] dark:border-[#44423f] rounded-lg text-[#1a1915] dark:text-[#d3d0c9] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] focus:outline-none focus:border-[#d7a16f]"/>
        </div>
        <div class="w-[120px] flex flex-col text-[14px]">
          <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">类型</label>
          <div class="flex p-[2px] bg-[#f0ede7] dark:bg-[#32312e] rounded-[8px]">
            <button class="flex-1 border-none bg-transparent rounded-[6px] py-[5px] text-[12px] font-medium text-[#8a8478] dark:text-[#a09e99] cursor-pointer transition-all duration-150 focus:outline-none" :class="{ 'bg-white dark:bg-[#44423f] text-[#1a1915] dark:text-[#e8e3db] shadow-[0_1px_3px_rgba(0,0,0,0.05)]': form.type === 'stdio' }" @click="form.type = 'stdio'">stdio</button>
            <button class="flex-1 border-none bg-transparent rounded-[6px] py-[5px] text-[12px] font-medium text-[#8a8478] dark:text-[#a09e99] cursor-pointer transition-all duration-150 focus:outline-none" :class="{ 'bg-white dark:bg-[#44423f] text-[#1a1915] dark:text-[#e8e3db] shadow-[0_1px_3px_rgba(0,0,0,0.05)]': form.type === 'sse' }" @click="form.type = 'sse'">SSE</button>
          </div>
        </div>
      </div>
      <template v-if="form.type === 'stdio'">
        <div class="mb-2.5 flex flex-col text-[14px]">
          <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">命令</label>
          <input v-model="form.command" placeholder="npx / uvx / node" class="w-full h-9 px-3 text-[14px] bg-white dark:bg-[#2e2d2a] border border-[#e8e3db] dark:border-[#44423f] rounded-lg text-[#1a1915] dark:text-[#d3d0c9] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] focus:outline-none focus:border-[#d7a16f] font-mono"/>
        </div>
        <div class="mb-2.5 flex flex-col text-[14px]">
          <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">参数 <span class="font-normal text-[#aaa49a] dark:text-[#88857f] ml-1 lowercase">空格分隔</span></label>
          <input v-model="form.args" placeholder="-y @playwright/mcp@latest" class="w-full h-9 px-3 text-[14px] bg-white dark:bg-[#2e2d2a] border border-[#e8e3db] dark:border-[#44423f] rounded-lg text-[#1a1915] dark:text-[#d3d0c9] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] focus:outline-none focus:border-[#d7a16f] font-mono"/>
        </div>
        <div class="mb-2.5 flex flex-col text-[14px]">
          <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">环境变量 <span class="font-normal text-[#aaa49a] dark:text-[#88857f] ml-1 lowercase">每行 KEY=VALUE（可选）</span></label>
          <textarea v-model="form.env" placeholder="API_KEY=xxx" rows="2" class="w-full px-3 py-2 text-[14px] bg-white dark:bg-[#2e2d2a] border border-[#e8e3db] dark:border-[#44423f] rounded-lg text-[#1a1915] dark:text-[#d3d0c9] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] focus:outline-none focus:border-[#d7a16f] font-mono resize-y"/>
        </div>
      </template>
      <template v-else>
        <div class="mb-2.5 flex flex-col text-[14px]">
          <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">SSE URL</label>
          <input v-model="form.url" placeholder="http://localhost:8080/sse" class="w-full h-9 px-3 text-[14px] bg-white dark:bg-[#2e2d2a] border border-[#e8e3db] dark:border-[#44423f] rounded-lg text-[#1a1915] dark:text-[#d3d0c9] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] focus:outline-none focus:border-[#d7a16f] font-mono"/>
        </div>
      </template>
      <div v-if="error" class="text-[12.5px] text-[#c0392b] dark:text-[#e57373] mb-2.5">{{ error }}</div>
      <div class="flex items-center justify-end gap-3 mt-2">
        <button class="h-9 px-5 bg-[#da7756] text-white border-none rounded-lg font-medium text-[13px] cursor-pointer shadow-[0_1px_2px_rgba(0,0,0,0.05)] transition-colors duration-150 hover:bg-[#c06548] focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed" :disabled="adding" @click="submit">{{ adding ? '连接中...' : '添加并连接' }}</button>
        <button class="h-9 px-4 bg-transparent border border-[#ddd9d0] dark:border-[#44423f] text-[#6b6456] dark:text-[#a09e99] rounded-lg font-medium text-[13px] cursor-pointer transition-colors duration-150 hover:bg-[#f5f4f0] dark:hover:bg-[#32312e] focus:outline-none" @click="resetForm">取消</button>
      </div>
    </div>

    <div v-if="loading" class="text-center py-8 text-[13.5px] text-[#aaa49a] dark:text-[#88857f]">加载中...</div>
    <div v-else-if="servers.length === 0 && !showForm" class="text-center py-8 text-[13.5px] text-[#aaa49a] dark:text-[#88857f]">暂无 MCP 服务，点击「添加」接入工具服务</div>
    <div v-else class="flex flex-col gap-2">
      <div v-for="s in servers" :key="s.name" class="flex items-center justify-between p-3 border border-[#ebe9e3] dark:border-[#3b3a37] rounded-xl gap-3 transition-colors duration-150 hover:border-[#d8d5cc] dark:hover:border-[#52504b]">
        <div class="flex items-center gap-2.5 flex-1 min-w-0">
          <span class="w-2 h-2 rounded-full shrink-0" :class="{
            'bg-[#4caf50]': s.status === 'connected',
            'bg-[#e53935]': s.status === 'error',
            'bg-[#fb8c00]': s.status === 'connecting',
            'bg-[#bdbdbd] dark:bg-[#666]': s.status === 'disconnected'
          }"></span>
          <div class="min-w-0">
            <div class="text-[13.5px] font-semibold text-[#2a2820] dark:text-[#e8e3db] truncate">{{ s.name }}</div>
            <div class="flex items-center gap-2 mt-0.5">
              <span class="text-[11px] px-1.5 py-[1px] rounded bg-[#f0ede7] dark:bg-[#32312e] text-[#8a8478] dark:text-[#a09e99] font-mono shrink-0">{{ s.type }}</span>
              <span v-if="s.enabled" class="text-[11px] px-1.5 py-[1px] rounded bg-[#edf7ed] dark:bg-[#233323] text-[#3a7c3a] dark:text-[#87c787] shrink-0">已启用</span>
              <span v-else class="text-[11px] px-1.5 py-[1px] rounded bg-[#f3f3f3] dark:bg-[#2f2f2f] text-[#7b7b7b] dark:text-[#9f9f9f] shrink-0">已停用</span>
              <span v-if="s.status === 'connected'" class="text-[12px] text-[#8a8478] dark:text-[#a09e99] whitespace-nowrap shrink-0">{{ s.toolCount }} 个工具</span>
              <span v-if="s.error" class="text-[12px] text-[#e53935] dark:text-[#e57373] truncate" :title="s.error">{{ s.error }}</span>
            </div>
          </div>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <button class="px-3 py-1.5 text-[12px] shrink-0 bg-transparent border rounded cursor-pointer transition-colors duration-150 disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none" :class="s.enabled ? 'text-[#8a6d3b] dark:text-[#d6b77a] border-[#ead8b5] dark:border-[#5a4b2f] hover:bg-[#fff8ec] dark:hover:bg-[#3a3226]' : 'text-[#2e7d32] dark:text-[#7bc67f] border-[#cfe8d1] dark:border-[#355c37] hover:bg-[#effaf0] dark:hover:bg-[#233323]'" :disabled="togglingName === s.name" @click="handleToggleEnabled(s.name, !s.enabled)">
            {{ togglingName === s.name ? '处理中...' : (s.enabled ? '停用' : '启用') }}
          </button>
          <button class="px-3 py-1.5 text-[12.5px] text-[#c0392b] dark:text-[#e57373] shrink-0 bg-transparent border border-transparent rounded hover:bg-[#fff0f0] dark:hover:bg-[#3b2a2a] hover:border-[#ffcccc] dark:hover:border-[#5c3a3a] cursor-pointer transition-colors duration-150 disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none" :disabled="removingName === s.name" @click="handleRemove(s.name)">
            {{ removingName === s.name ? '删除中...' : '删除' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.mcp-toast-enter-active,
.mcp-toast-leave-active {
  transition: all 0.22s ease;
}

.mcp-toast-enter-from,
.mcp-toast-leave-to {
  opacity: 0;
  transform: translateY(-8px) translateX(8px);
}
</style>