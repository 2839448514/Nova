<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

type SkillItem = {
  name: string
  description: string
  path: string
  enabled: boolean
}

const loading = ref(false)
const saving = ref(false)
const savedTip = ref(false)
const error = ref('')
const skills = ref<SkillItem[]>([])
const rawSettings = ref<any>({})

const normalize = (name: string) => name.trim().toLowerCase()

const refresh = async () => {
  loading.value = true
  error.value = ''
  try {
    const settings: any = (await invoke('get_settings')) || {}
    rawSettings.value = settings
    const disabled = new Set<string>(
      (Array.isArray(settings.disabledSkills) ? settings.disabledSkills : [])
        .filter((v: unknown) => typeof v === 'string')
        .map((v: string) => normalize(v))
    )

    const list = await invoke<Array<{ name: string; description: string; path: string }>>('list_skills')
    skills.value = list.map((s) => ({
      ...s,
      enabled: !disabled.has(normalize(s.name)),
    }))
  } catch (e) {
    error.value = `加载技能失败: ${String(e)}`
    skills.value = []
  } finally {
    loading.value = false
  }
}

const setAllEnabled = (enabled: boolean) => {
  skills.value = skills.value.map((s) => ({ ...s, enabled }))
}

const save = async () => {
  saving.value = true
  error.value = ''
  try {
    const listed = new Set(skills.value.map((s) => normalize(s.name)))
    const existingDisabled = (Array.isArray(rawSettings.value?.disabledSkills)
      ? rawSettings.value.disabledSkills
      : [])
      .filter((v: unknown) => typeof v === 'string')

    const preservedDisabled = existingDisabled.filter((name: string) => !listed.has(normalize(name)))
    const currentDisabled = skills.value.filter((s) => !s.enabled).map((s) => s.name)

    const settings = {
      ...rawSettings.value,
      disabledSkills: [...preservedDisabled, ...currentDisabled],
    }

    await invoke('save_settings', { settings })
    rawSettings.value = settings
    savedTip.value = true
    setTimeout(() => (savedTip.value = false), 2000)
  } catch (e) {
    error.value = `保存失败: ${String(e)}`
  } finally {
    saving.value = false
  }
}

onMounted(refresh)
</script>

<template>
  <div class="px-6 py-4 flex flex-col h-full overflow-y-auto">
    <div class="flex items-center justify-between mb-4">
      <span class="text-[12.5px] text-[#aaa49a] dark:text-[#88857f]">{{ skills.length }} 个技能</span>
      <div class="flex items-center gap-2">
        <button
          class="h-[34px] px-3 bg-transparent border border-[#ddd9d0] dark:border-[#44423f] rounded-lg text-[12.5px] text-[#6b6456] dark:text-[#a09e99] cursor-pointer transition-colors duration-150 hover:bg-[#f5f4f0] dark:hover:bg-[#32312e]"
          :disabled="loading"
          @click="refresh"
        >刷新</button>
        <button
          class="h-[34px] px-3 bg-transparent border border-[#ddd9d0] dark:border-[#44423f] rounded-lg text-[12.5px] text-[#6b6456] dark:text-[#a09e99] cursor-pointer transition-colors duration-150 hover:bg-[#f5f4f0] dark:hover:bg-[#32312e]"
          :disabled="loading || skills.length === 0"
          @click="setAllEnabled(true)"
        >全部启用</button>
        <button
          class="h-[34px] px-3 bg-transparent border border-[#ddd9d0] dark:border-[#44423f] rounded-lg text-[12.5px] text-[#6b6456] dark:text-[#a09e99] cursor-pointer transition-colors duration-150 hover:bg-[#f5f4f0] dark:hover:bg-[#32312e]"
          :disabled="loading || skills.length === 0"
          @click="setAllEnabled(false)"
        >全部停用</button>
      </div>
    </div>

    <div v-if="loading" class="text-center py-8 text-[13.5px] text-[#aaa49a] dark:text-[#88857f]">技能扫描中...</div>
    <div v-else-if="skills.length === 0" class="text-center py-8 text-[13.5px] text-[#aaa49a] dark:text-[#88857f]">
      未发现技能。请将技能放在 `.github/skills/*/SKILL.md` 或 `skills/*/SKILL.md`。
    </div>
    <div v-else class="flex flex-col gap-2">
      <div
        v-for="skill in skills"
        :key="skill.path"
        class="flex items-center justify-between p-3 border border-[#ebe9e3] dark:border-[#3b3a37] rounded-xl gap-3"
      >
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <div class="text-[13.5px] font-semibold text-[#2a2820] dark:text-[#e8e3db] truncate">{{ skill.name }}</div>
            <span
              class="text-[11px] px-1.5 py-[1px] rounded shrink-0"
              :class="skill.enabled ? 'bg-[#edf7ed] dark:bg-[#233323] text-[#3a7c3a] dark:text-[#87c787]' : 'bg-[#f3f3f3] dark:bg-[#2f2f2f] text-[#7b7b7b] dark:text-[#9f9f9f]'"
            >{{ skill.enabled ? '已启用' : '已停用' }}</span>
          </div>
          <div class="text-[12px] text-[#8a8478] dark:text-[#a09e99] mt-1 line-clamp-2">{{ skill.description }}</div>
          <div class="text-[11px] text-[#b0a99f] dark:text-[#66645e] mt-1 truncate" :title="skill.path">{{ skill.path }}</div>
        </div>
        <button
          class="px-3 py-1.5 text-[12px] shrink-0 bg-transparent border rounded cursor-pointer transition-colors duration-150"
          :class="skill.enabled ? 'text-[#8a6d3b] dark:text-[#d6b77a] border-[#ead8b5] dark:border-[#5a4b2f] hover:bg-[#fff8ec] dark:hover:bg-[#3a3226]' : 'text-[#2e7d32] dark:text-[#7bc67f] border-[#cfe8d1] dark:border-[#355c37] hover:bg-[#effaf0] dark:hover:bg-[#233323]'"
          @click="skill.enabled = !skill.enabled"
        >{{ skill.enabled ? '停用' : '启用' }}</button>
      </div>
    </div>

    <div class="mt-auto pt-4 border-t border-[#f0ece4] dark:border-[#32312e]">
      <div v-if="error" class="text-[12.5px] text-[#c0392b] dark:text-[#e57373] mb-2">{{ error }}</div>
      <div class="flex items-center justify-end gap-3">
        <span v-if="savedTip" class="text-[13px] text-[#4f9c64] dark:text-[#62c07a]">✓ 已保存</span>
        <button
          class="h-9 px-5 bg-[#da7756] text-white border-none rounded-lg font-medium text-[13px] cursor-pointer shadow-[0_1px_2px_rgba(0,0,0,0.05)] transition-colors duration-150 hover:bg-[#c06548] disabled:opacity-50 disabled:cursor-not-allowed"
          :disabled="saving"
          @click="save"
        >{{ saving ? '保存中...' : '保存设置' }}</button>
      </div>
    </div>
  </div>
</template>