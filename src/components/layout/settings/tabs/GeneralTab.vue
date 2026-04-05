<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { emitToast } from '../../../../lib/toast'
import {
  applyUiTheme,
  getStoredUiLanguage,
  getStoredUiTheme,
  normalizeUiLanguage,
  normalizeUiTheme,
  setStoredUiLanguage,
  setStoredUiTheme,
  type UiLanguage,
  type UiTheme,
} from '../../../../lib/ui-preferences'

const theme = ref<UiTheme>(getStoredUiTheme())
const language = ref<UiLanguage>(getStoredUiLanguage())
const isSavingPreferences = ref(false)
const isClearingHistory = ref(false)
const cachedSettings = ref<Record<string, unknown> | null>(null)

const localeTexts = {
  'zh-CN': {
    appearanceTitle: '外观',
    appearanceDesc: '选择 Nova 在你的设备上的显示方式。',
    languageTitle: '语言',
    languageDesc: '切换界面显示语言。',
    clearHistoryTitle: '清空聊天历史',
    clearHistoryDesc: '删除本地消息和会话级数据。',
    clearHistoryButton: '清空历史',
    clearHistoryWorking: '清理中...',
    clearHistoryConfirm: '确认清空全部聊天历史吗？该操作不可撤销。',
    clearHistoryDone: '已清空聊天历史。',
    clearHistoryFailed: '清空聊天历史失败：',
    settingsSaveFailed: '保存设置失败：',
    themeSystem: '系统',
    themeLight: '浅色',
    themeDark: '深色',
    languageEnglish: 'English',
    languageChinese: '简体中文',
  },
  'en-US': {
    appearanceTitle: 'Appearance',
    appearanceDesc: 'Select how Nova looks on your device.',
    languageTitle: 'Language',
    languageDesc: 'Change the interface language.',
    clearHistoryTitle: 'Clear Chat History',
    clearHistoryDesc: 'Remove all local messages and session data.',
    clearHistoryButton: 'Clear History',
    clearHistoryWorking: 'Clearing...',
    clearHistoryConfirm: 'Clear all chat history? This action cannot be undone.',
    clearHistoryDone: 'Chat history cleared.',
    clearHistoryFailed: 'Failed to clear chat history: ',
    settingsSaveFailed: 'Failed to save settings: ',
    themeSystem: 'System',
    themeLight: 'Light',
    themeDark: 'Dark',
    languageEnglish: 'English',
    languageChinese: '简体中文',
  },
} as const

const t = computed(() => localeTexts[language.value])

const themeOptions = computed(() => [
  { value: 'system' as UiTheme, label: t.value.themeSystem },
  { value: 'light' as UiTheme, label: t.value.themeLight },
  { value: 'dark' as UiTheme, label: t.value.themeDark },
])

const dispatchLanguageUpdated = () => {
  window.dispatchEvent(
    new CustomEvent('ui-language-updated', {
      detail: { language: language.value },
    }),
  )
}

const loadSettings = async () => {
  try {
    const settings = await invoke<Record<string, unknown>>('get_settings')
    cachedSettings.value = settings

    const nextLanguage = normalizeUiLanguage(settings.uiLanguage)
    const nextTheme = normalizeUiTheme(settings.uiTheme)
    language.value = nextLanguage
    theme.value = nextTheme

    setStoredUiLanguage(nextLanguage)
    setStoredUiTheme(nextTheme)
    applyUiTheme(nextTheme)
    dispatchLanguageUpdated()
  } catch (error) {
    console.error('Failed to load general settings:', error)
    applyUiTheme(theme.value)
    dispatchLanguageUpdated()
  }
}

const persistPreferences = async () => {
  if (isSavingPreferences.value) {
    return
  }

  isSavingPreferences.value = true
  try {
    const baseSettings = cachedSettings.value ?? await invoke<Record<string, unknown>>('get_settings')
    const nextSettings: Record<string, unknown> = {
      ...baseSettings,
      uiLanguage: language.value,
      uiTheme: theme.value,
    }

    cachedSettings.value = nextSettings
    await invoke('save_settings', { settings: nextSettings })
    window.dispatchEvent(new CustomEvent('settings-updated'))
  } catch (error) {
    console.error('Failed to save general settings:', error)
    emitToast({
      variant: 'error',
      source: 'settings',
      message: `${t.value.settingsSaveFailed}${String(error)}`,
    })
  } finally {
    isSavingPreferences.value = false
  }
}

const setTheme = (value: UiTheme) => {
  const normalized = normalizeUiTheme(value)
  theme.value = normalized
  setStoredUiTheme(normalized)
  applyUiTheme(normalized)
  void persistPreferences()
}

const onLanguageChange = () => {
  const normalized = normalizeUiLanguage(language.value)
  language.value = normalized
  setStoredUiLanguage(normalized)
  dispatchLanguageUpdated()
  void persistPreferences()
}

const clearHistory = async () => {
  if (isClearingHistory.value) {
    return
  }

  if (!window.confirm(t.value.clearHistoryConfirm)) {
    return
  }

  isClearingHistory.value = true
  try {
    await invoke('clear_history', { conversationId: null })
    window.dispatchEvent(new CustomEvent('history-cleared'))
    emitToast({
      variant: 'success',
      source: 'history',
      message: t.value.clearHistoryDone,
    })
  } catch (error) {
    console.error('Failed to clear history:', error)
    emitToast({
      variant: 'error',
      source: 'history',
      message: `${t.value.clearHistoryFailed}${String(error)}`,
    })
  } finally {
    isClearingHistory.value = false
  }
}

onMounted(() => {
  void loadSettings()
})
</script>

<template>
  <div class="flex flex-col">
    <!-- Theme -->
    <div class="flex items-center justify-between py-4 border-b border-[#f0ede7] dark:border-[#333] gap-4">
      <div class="flex flex-col gap-0.5">
        <span class="text-[0.9rem] font-medium text-[#2a2820] dark:text-[#ececec]">{{ t.appearanceTitle }}</span>
        <span class="text-xs text-muted-foreground">{{ t.appearanceDesc }}</span>
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
        <span class="text-[0.9rem] font-medium text-[#2a2820] dark:text-[#ececec]">{{ t.languageTitle }}</span>
        <span class="text-xs text-muted-foreground">{{ t.languageDesc }}</span>
      </div>
      <select 
        v-model="language"
        @change="onLanguageChange"
        class="px-3 py-1.5 border border-[#ddd9d0] dark:border-[#444] rounded-lg text-[0.85rem] text-[#2a2820] dark:text-[#ececec] bg-white dark:bg-[#2a2a2a] cursor-pointer outline-none min-w-[120px] focus:border-black/30 dark:focus:border-white/30"
      >
        <option value="zh-CN">{{ t.languageChinese }}</option>
        <option value="en-US">{{ t.languageEnglish }}</option>
      </select>
    </div>

    <!-- Sidebar Default State -->
    <div class="flex items-center justify-between py-4 gap-4">
      <div class="flex flex-col gap-0.5">
        <span class="text-[0.9rem] font-medium text-[#2a2820] dark:text-[#ececec]">{{ t.clearHistoryTitle }}</span>
        <span class="text-xs text-muted-foreground">{{ t.clearHistoryDesc }}</span>
      </div>
      <button
        class="px-3 py-1.5 border border-[#e8c5c5] dark:border-[#522] text-[#c0392b] dark:text-[#f87171] rounded-md text-[0.85rem] font-medium hover:bg-[#fdf0f0] dark:hover:bg-[#311] transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
        :disabled="isClearingHistory"
        @click="clearHistory"
      >
        {{ isClearingHistory ? t.clearHistoryWorking : t.clearHistoryButton }}
      </button>
    </div>
  </div>
</template>
