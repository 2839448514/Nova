<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const apiKeyInput = ref('')
const apiKeyVisible = ref(false)
const baseURLInput = ref('https://api.anthropic.com/v1')
const savedTip = ref(false)

const providers = [
  { id: 'anthropic', label: 'Anthropic', url: 'https://api.anthropic.com/v1' },
  { id: 'dashscope-anthropic', label: 'DashScope Anthropic', url: 'https://dashscope.aliyuncs.com/apps/anthropic' },
]

const selectedProvider = ref('anthropic')
const selectedModel = ref('claude-3-5-sonnet-20241022')

const selectProvider = (id: string) => {
  selectedProvider.value = id
  const p = providers.find(p => p.id === id)!
  if (p.url) baseURLInput.value = p.url
}

onMounted(async () => {
  try {
    const settings: any = await invoke('get_settings')
    if (settings) {
      if (settings.apiKey) apiKeyInput.value = settings.apiKey
      if (settings.baseUrl) baseURLInput.value = settings.baseUrl
      if (settings.model) selectedModel.value = settings.model
      selectedProvider.value = settings.baseUrl?.includes('dashscope.aliyuncs.com/apps/anthropic')
        ? 'dashscope-anthropic'
        : 'anthropic'
    }
  } catch (error) {
    console.error('Failed to load settings:', error)
  }
})

const save = async () => {
  try {
    const settings = {
      apiKey: apiKeyInput.value.trim() || '',
      baseUrl: baseURLInput.value.trim(),
      model: selectedModel.value.trim(),
      provider: selectedProvider.value
    }
    await invoke('save_settings', { settings })
    savedTip.value = true
    setTimeout(() => savedTip.value = false, 2000)
  } catch (error) {
    console.error('Failed to save settings:', error)
  }
}
</script>

<template>
  <div class="px-6 py-4 flex flex-col h-full overflow-y-auto">

    <div class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">服务商</div>
    <div class="flex gap-1.5 mb-5 flex-wrap">
      <button
        v-for="p in providers"
        :key="p.id"
        class="px-4 py-1.5 rounded-full text-[13px] border cursor-pointer transition-all duration-150 focus:outline-none"
        :class="selectedProvider === p.id 
          ? 'bg-[#2a2820] dark:bg-[#e8e3db] text-white dark:text-[#1a1915] border-[#2a2820] dark:border-[#e8e3db]' 
          : 'bg-transparent text-[#6b6456] dark:text-[#a09e99] border-[#ddd9d0] dark:border-[#3b3a37] hover:bg-[#f5f4f0] dark:hover:bg-[#32312e]'"
        @click="selectProvider(p.id)"
      >
        {{ p.label }}
      </button>
    </div>

    <!-- Model Input -->
    <div class="mb-4 flex flex-col text-[14px]">
      <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">模型</label>
      <input
        v-model="selectedModel"
        placeholder="输入模型名称，如 claude-3-5-sonnet-20241022"
        class="w-full h-9 px-3 text-[14px] bg-white dark:bg-[#252422] border border-[#e8e3db] dark:border-[#3b3a37] rounded-lg text-[#1a1915] dark:text-[#d3d0c9] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] focus:outline-none focus:border-[#d7a16f]"
      />
    </div>

    <!-- Base URL -->
    <div class="mb-4 flex flex-col text-[14px]">
      <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">Base URL</label>
      <input v-model="baseURLInput" placeholder="https://..." class="w-full h-9 px-3 text-[14px] bg-white dark:bg-[#252422] border border-[#e8e3db] dark:border-[#3b3a37] rounded-lg text-[#1a1915] dark:text-[#d3d0c9] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] focus:outline-none focus:border-[#d7a16f]" />
    </div>

    <!-- API Key -->
    <div class="mb-4 flex flex-col text-[14px]">
      <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">API Key</label>
      <div class="relative w-full">
        <input
          :type="apiKeyVisible ? 'text' : 'password'"
          v-model="apiKeyInput"
          placeholder="sk-xxxxxxxxxxxxxxxx"
          class="w-full h-9 px-3 pr-10 text-[14px] bg-white dark:bg-[#252422] border border-[#e8e3db] dark:border-[#3b3a37] rounded-lg text-[#1a1915] dark:text-[#d3d0c9] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] focus:outline-none focus:border-[#d7a16f]"
        />
        <button class="absolute right-2 top-1/2 -translate-y-1/2 bg-transparent border-none text-[#a09e99] dark:text-[#88857f] cursor-pointer p-1 rounded-md transition-colors duration-100 hover:text-[#1a1915] dark:hover:text-[#e8e3db] hover:bg-[#f5f4f0] dark:hover:bg-[#32312e] flex items-center justify-center h-7 w-7 focus:outline-none" @click="apiKeyVisible = !apiKeyVisible">
          <svg viewBox="0 0 24 24" fill="none" class="w-[18px] h-[18px]" stroke="currentColor" stroke-width="2">
            <path v-if="apiKeyVisible" d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" stroke-linecap="round" stroke-linejoin="round"/>        
            <path v-else d="M15 12a3 3 0 11-6 0 3 3 0 016 0z M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </button>
      </div>
    </div>

    <!-- 保存 -->
    <div class="mt-auto flex items-center justify-end gap-3 pt-4 border-t border-[#f0ece4] dark:border-[#32312e]">
      <span v-if="savedTip" class="text-[13px] text-[#4f9c64] dark:text-[#62c07a] transition-opacity duration-300 opacity-100 data-[state=leave]:opacity-0">✓ 已保存</span>
      <button class="h-9 px-5 bg-[#da7756] dark:bg-[#da7756] text-white border-none rounded-lg font-medium text-[13px] cursor-pointer shadow-[0_1px_2px_rgba(0,0,0,0.05)] transition-colors duration-150 hover:bg-[#c06548] dark:hover:bg-[#c06548] focus:outline-none" @click="save">保存</button>      
    </div>

  </div>
</template>
