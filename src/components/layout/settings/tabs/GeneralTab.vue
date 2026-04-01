<script setup lang="ts">
import { ref } from 'vue'

type Theme = 'light' | 'dark' | 'system'
const theme = ref<Theme>('system')
const themeOptions: { value: Theme; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'light',  label: 'Light' },
  { value: 'dark',   label: 'Dark' },
]

const language = ref('en-US')
const isSidebarOpen = ref(true)

const setTheme = (val: Theme) => {
  theme.value = val;
  // TODO: Trigger actual dark mode changes based on class toggle
}
</script>

<template>
  <div class="flex flex-col">
    <!-- Theme -->
    <div class="flex items-center justify-between py-4 border-b border-[#f0ede7] dark:border-[#333] gap-4">
      <div class="flex flex-col gap-0.5">
        <span class="text-[0.9rem] font-medium text-[#2a2820] dark:text-[#ececec]">Appearance</span>
        <span class="text-xs text-muted-foreground">Select how Nova looks on your device.</span>
      </div>
      <div class="flex bg-[#f4f4f4] dark:bg-[#1f1f1f] rounded-lg p-1 gap-1 border border-[#e5e5e5] dark:border-[#444]">
        <button 
          v-for="opt in themeOptions" 
          :key="opt.value"
          class="px-3 py-1.5 border-none rounded-md text-[0.85rem] font-medium cursor-pointer transition-colors"
          :class="[
            theme === opt.value 
              ? 'bg-white dark:bg-[#333] text-[#2a2820] dark:text-[#ececec] shadow-sm' 
              : 'bg-transparent text-muted-foreground hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d]'
          ]"
          @click="setTheme(opt.value)"
        >
          {{ opt.label }}
        </button>
      </div>
    </div>
    
    <!-- Language -->
    <div class="flex items-center justify-between py-4 border-b border-[#f0ede7] dark:border-[#333] gap-4">
      <div class="flex flex-col gap-0.5">
        <span class="text-[0.9rem] font-medium text-[#2a2820] dark:text-[#ececec]">Language</span>
        <span class="text-xs text-muted-foreground">Change the interface language.</span>
      </div>
      <select 
        v-model="language"
        class="px-3 py-1.5 border border-[#ddd9d0] dark:border-[#444] rounded-lg text-[0.85rem] text-[#2a2820] dark:text-[#ececec] bg-white dark:bg-[#2a2a2a] cursor-pointer outline-none min-w-[120px] focus:border-black/30 dark:focus:border-white/30"
      >
        <option value="en-US">English</option>
        <option value="zh-CN">简体中文</option>
      </select>
    </div>

    <!-- Sidebar Default State -->
    <div class="flex items-center justify-between py-4 gap-4">
      <div class="flex flex-col gap-0.5">
        <span class="text-[0.9rem] font-medium text-[#2a2820] dark:text-[#ececec]">Clear Chat History</span>
        <span class="text-xs text-muted-foreground">Remove all local messages and session data.</span>
      </div>
      <button class="px-3 py-1.5 border border-[#e8c5c5] dark:border-[#522] text-[#c0392b] dark:text-[#f87171] rounded-md text-[0.85rem] font-medium hover:bg-[#fdf0f0] dark:hover:bg-[#311] transition-colors">
        Clear History
      </button>
    </div>
  </div>
</template>