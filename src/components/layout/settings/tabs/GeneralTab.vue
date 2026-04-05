<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { emitToast } from '../../../../lib/toast'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
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

const onLanguageSelect = (value: string) => {
  language.value = normalizeUiLanguage(value)
  onLanguageChange()
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
  <div class="flex flex-col gap-3">
    <Card class="border-[#ebe9e3] dark:border-[#3b3a37]">
      <CardHeader class="pb-2">
        <CardTitle class="text-[0.9rem]">{{ t.appearanceTitle }}</CardTitle>
        <CardDescription>{{ t.appearanceDesc }}</CardDescription>
      </CardHeader>
      <CardContent>
        <div class="flex flex-wrap gap-2">
          <Button
            v-for="opt in themeOptions"
            :key="opt.value"
            size="sm"
            :variant="theme === opt.value ? 'default' : 'outline'"
            class="min-w-[88px]"
            @click="setTheme(opt.value)"
          >
            {{ opt.label }}
          </Button>
        </div>
      </CardContent>
    </Card>

    <Card class="border-[#ebe9e3] dark:border-[#3b3a37]">
      <CardHeader class="pb-2">
        <CardTitle class="text-[0.9rem]">{{ t.languageTitle }}</CardTitle>
        <CardDescription>{{ t.languageDesc }}</CardDescription>
      </CardHeader>
      <CardContent>
        <Select :model-value="language" @update:model-value="(value) => onLanguageSelect(String(value))">
          <SelectTrigger class="w-[180px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="zh-CN">{{ t.languageChinese }}</SelectItem>
            <SelectItem value="en-US">{{ t.languageEnglish }}</SelectItem>
          </SelectContent>
        </Select>
      </CardContent>
    </Card>

    <Card class="border-[#ebe9e3] dark:border-[#3b3a37]">
      <CardHeader class="pb-2">
        <CardTitle class="text-[0.9rem]">{{ t.clearHistoryTitle }}</CardTitle>
        <CardDescription>{{ t.clearHistoryDesc }}</CardDescription>
      </CardHeader>
      <CardContent>
        <Button
          variant="destructive"
          size="sm"
          :disabled="isClearingHistory"
          @click="clearHistory"
        >
          {{ isClearingHistory ? t.clearHistoryWorking : t.clearHistoryButton }}
        </Button>
      </CardContent>
    </Card>
  </div>
</template>
