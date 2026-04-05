<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

type ProviderProfile = {
  apiKey: string
  baseUrl: string
  model: string
}

const apiKeyInput = ref('')
const apiKeyVisible = ref(false)
const baseURLInput = ref('')
const savedTip = ref(false)

const providers = [
  { id: 'anthropic', label: 'Anthropic' },
  { id: 'openai', label: 'OpenAI' },
  { id: 'ollama', label: 'Ollama' },
  { id: 'dashscope-anthropic', label: 'DashScope Anthropic' }
]

const selectedProvider = ref('anthropic')
const newModelInput = ref('')
const customModels = ref<Record<string, string[]>>({})
const providerProfiles = ref<Record<string, ProviderProfile>>({})

const normalizeProviderKey = (provider: string) => provider.trim().toLowerCase()

const defaultBaseUrl = (provider: string) => {
  const key = normalizeProviderKey(provider)
  if (key === 'anthropic') return 'https://api.anthropic.com/v1'
  if (key === 'openai') return 'https://api.openai.com/v1'
  if (key === 'ollama') return 'http://localhost:11434/v1'
  if (key === 'dashscope-anthropic') return 'https://dashscope.aliyuncs.com/api/v1/apps/anthropic'
  return ''
}

const ensureProfile = (provider: string, fallback?: Partial<ProviderProfile>): ProviderProfile => {
  const key = normalizeProviderKey(provider)
  const existing = providerProfiles.value[key]
  if (existing) return existing

  const profile: ProviderProfile = {
    apiKey: fallback?.apiKey ?? '',
    baseUrl: fallback?.baseUrl ?? defaultBaseUrl(key),
    model: fallback?.model ?? '',
  }
  providerProfiles.value[key] = profile
  return profile
}

const readProviderInputs = (provider: string) => {
  const profile = ensureProfile(provider)
  apiKeyInput.value = profile.apiKey || ''
  baseURLInput.value = profile.baseUrl || ''
}

const writeProviderInputs = (provider: string) => {
  const profile = ensureProfile(provider)
  profile.apiKey = apiKeyInput.value.trim()
  profile.baseUrl = baseURLInput.value.trim()
}

const selectProvider = (id: string) => {
  writeProviderInputs(selectedProvider.value)
  selectedProvider.value = id
  ensureProfile(id)
  if (!customModels.value[id]) {
    customModels.value[id] = []
  }
  readProviderInputs(id)
}

onMounted(async () => {
  try {
    const settings: any = await invoke('get_settings')
    if (settings) {
      if (settings.customModels) {
        customModels.value = settings.customModels
      }
      if (settings.providerProfiles && typeof settings.providerProfiles === 'object') {
        providerProfiles.value = settings.providerProfiles
      }
      if (settings.provider) {
        selectedProvider.value = settings.provider
      } else {
        selectedProvider.value = settings.baseUrl?.includes('dashscope.aliyuncs.com/apps/anthropic')
          ? 'dashscope-anthropic'
          : 'anthropic'
      }

      ensureProfile(selectedProvider.value, {
        apiKey: settings.apiKey || '',
        baseUrl: settings.baseUrl || defaultBaseUrl(selectedProvider.value),
        model: settings.model || '',
      })
      
      if (!customModels.value[selectedProvider.value]) {
        customModels.value[selectedProvider.value] = []
      }

      readProviderInputs(selectedProvider.value)
    }
  } catch (error) {
    console.error('Failed to load settings:', error)
  }
})

const addModel = () => {
  const m = newModelInput.value.trim()
  if (m) {
    if (!customModels.value[selectedProvider.value]) {
      customModels.value[selectedProvider.value] = []
    }
    if (!customModels.value[selectedProvider.value].includes(m)) {
      customModels.value[selectedProvider.value].push(m)
    }
    newModelInput.value = ''
  }
}

const removeModel = (m: string) => {
  if (customModels.value[selectedProvider.value]) {
    customModels.value[selectedProvider.value] = customModels.value[selectedProvider.value].filter(model => model !== m)
  }
}

const save = async () => {
  try {
    writeProviderInputs(selectedProvider.value)
    const prevSettings: any = (await invoke('get_settings')) || {}
    const active = ensureProfile(selectedProvider.value)
    const settings = {
      ...prevSettings,
      apiKey: active.apiKey,
      baseUrl: active.baseUrl,
      model: active.model || prevSettings.model || '',
      provider: selectedProvider.value,
      customModels: customModels.value,
      providerProfiles: providerProfiles.value,
    }
    await invoke('save_settings', { settings })
    window.dispatchEvent(new CustomEvent('settings-updated'))
    savedTip.value = true
    setTimeout(() => (savedTip.value = false), 2000)
  } catch (error) {
    console.error('Failed to save settings:', error)
  }
}
</script>

<template>
  <div class="px-6 py-4 flex flex-col h-full overflow-y-auto">

    <div class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">服务商</div>
    <div class="flex gap-1.5 mb-5 flex-wrap">
      <Button
        v-for="p in providers"
        :key="p.id"
        size="sm"
        class="rounded-full text-[13px]"
        :class="selectedProvider === p.id 
          ? 'bg-[#2a2820] dark:bg-[#e8e3db] text-white dark:text-[#1a1915] border-[#2a2820] dark:border-[#e8e3db]' 
          : 'bg-transparent text-[#6b6456] dark:text-[#a09e99] border-[#ddd9d0] dark:border-[#3b3a37] hover:bg-[#f5f4f0] dark:hover:bg-[#32312e]'"
        @click="selectProvider(p.id)"
      >
        {{ p.label }}
      </Button>
    </div>

    <!-- Custom Models List -->
    <div class="mb-4 flex flex-col text-[14px]">
      <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">自定义模型</label>
      <div class="flex gap-2 mb-2">
        <Input
          v-model="newModelInput"
          @keydown.enter="addModel"
          placeholder="添加新模型名称 (如 claude-3-opus)"
          class="flex-1 h-9 px-3 text-[14px] bg-white dark:bg-[#252422] border border-[#e8e3db] dark:border-[#3b3a37] rounded-lg text-[#1a1915] dark:text-[#d3d0c9] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] focus:outline-none focus:border-[#d7a16f]"
        />
        <Button
          size="sm"
          @click="addModel"
          class="h-9 px-4 bg-[#f5f4f0] dark:bg-[#32312e] text-[#1a1915] dark:text-[#e8e3db] border border-[#ddd9d0] dark:border-[#3b3a37] rounded-lg text-[13px] hover:bg-[#eae8e4] dark:hover:bg-[#3c3a37] transition-colors"
        >添加</Button>
      </div>
      <div v-if="customModels[selectedProvider]?.length" class="flex flex-col gap-1.5 mt-2 max-h-48 overflow-y-auto pr-1">
        <div 
          v-for="model in customModels[selectedProvider]" 
          :key="model"
          class="group flex items-center justify-between px-3 py-2 bg-white dark:bg-[#252422] border border-[#f0ece4] dark:border-[#32312e] rounded-md text-[13px] text-[#6b6456] dark:text-[#a09e99]"
        >
          <span>{{ model }}</span>
          <Button
            variant="ghost"
            size="icon-sm"
            @click="removeModel(model)"
            class="opacity-0 group-hover:opacity-100 text-[#da7756] hover:text-[#c06548] transition-opacity"
            title="移除"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" class="w-[14px] h-[14px]" stroke="currentColor" stroke-width="2"><path d="M18 6L6 18M6 6l12 12"/></svg>
          </Button>
        </div>
      </div>
      <div v-else class="text-[13px] text-[#b0a99f] dark:text-[#66645e] mt-1 italic">
        暂无自定义模型。请在上方输入模型名称并点击添加。
      </div>
    </div>

    <!-- Base URL -->
    <div class="mb-4 flex flex-col text-[14px]">
      <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">Base URL</label>
      <Input v-model="baseURLInput" placeholder="https://..." class="w-full h-9 px-3 text-[14px] bg-white dark:bg-[#252422] border border-[#e8e3db] dark:border-[#3b3a37] rounded-lg text-[#1a1915] dark:text-[#d3d0c9] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] focus:outline-none focus:border-[#d7a16f]" />
    </div>

    <!-- API Key -->
    <div class="mb-4 flex flex-col text-[14px]">
      <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">API Key</label>
      <div class="relative w-full">
        <Input
          :type="apiKeyVisible ? 'text' : 'password'"
          v-model="apiKeyInput"
          placeholder="sk-xxxxxxxxxxxxxxxx"
          class="w-full h-9 px-3 pr-10 text-[14px] bg-white dark:bg-[#252422] border border-[#e8e3db] dark:border-[#3b3a37] rounded-lg text-[#1a1915] dark:text-[#d3d0c9] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] focus:outline-none focus:border-[#d7a16f]"
        />
        <Button
          variant="ghost"
          size="icon-sm"
          class="absolute right-2 top-1/2 h-7 w-7 -translate-y-1/2 text-[#a09e99] hover:bg-[#f5f4f0] hover:text-[#1a1915] dark:text-[#88857f] dark:hover:bg-[#32312e] dark:hover:text-[#e8e3db]"
          @click="apiKeyVisible = !apiKeyVisible"
        >
          <svg viewBox="0 0 24 24" fill="none" class="w-[18px] h-[18px]" stroke="currentColor" stroke-width="2">
            <path v-if="apiKeyVisible" d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" stroke-linecap="round" stroke-linejoin="round"/>        
            <path v-else d="M15 12a3 3 0 11-6 0 3 3 0 016 0z M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </Button>
      </div>
    </div>

    <!-- 保存 -->
    <div class="mt-auto flex items-center justify-end gap-3 pt-4 border-t border-[#f0ece4] dark:border-[#32312e]">
      <span v-if="savedTip" class="text-[13px] text-[#4f9c64] dark:text-[#62c07a] transition-opacity duration-300 opacity-100 data-[state=leave]:opacity-0">✓ 已保存</span>
      <Button class="h-9 bg-[#da7756] text-white hover:bg-[#c06548]" @click="save">保存</Button>
    </div>

  </div>
</template>
