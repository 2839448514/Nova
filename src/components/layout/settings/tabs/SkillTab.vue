<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'

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
    <div class="mb-4 flex items-center justify-between">
      <span class="text-[12.5px] text-[#aaa49a] dark:text-[#88857f]">{{ skills.length }} 个技能</span>
      <div class="flex items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          class="border-[#ddd9d0] text-[#6b6456] hover:bg-[#f5f4f0] dark:border-[#44423f] dark:text-[#a09e99] dark:hover:bg-[#32312e]"
          :disabled="loading"
          @click="refresh"
        >刷新</Button>
        <Button
          variant="outline"
          size="sm"
          class="border-[#ddd9d0] text-[#6b6456] hover:bg-[#f5f4f0] dark:border-[#44423f] dark:text-[#a09e99] dark:hover:bg-[#32312e]"
          :disabled="loading || skills.length === 0"
          @click="setAllEnabled(true)"
        >全部启用</Button>
        <Button
          variant="outline"
          size="sm"
          class="border-[#ddd9d0] text-[#6b6456] hover:bg-[#f5f4f0] dark:border-[#44423f] dark:text-[#a09e99] dark:hover:bg-[#32312e]"
          :disabled="loading || skills.length === 0"
          @click="setAllEnabled(false)"
        >全部停用</Button>
      </div>
    </div>

    <Card
      v-if="loading"
      class="border-[#ebe9e3] bg-[#faf9f7] dark:border-[#3b3a37] dark:bg-[#252422]"
    >
      <CardContent class="py-8 text-center text-[13.5px] text-[#aaa49a] dark:text-[#88857f]">技能扫描中...</CardContent>
    </Card>

    <Card
      v-else-if="skills.length === 0"
      class="border-[#ebe9e3] bg-[#faf9f7] dark:border-[#3b3a37] dark:bg-[#252422]"
    >
      <CardContent class="py-8 text-center text-[13.5px] text-[#aaa49a] dark:text-[#88857f]">
        未发现技能。请将技能放在应用数据目录的 skills 子目录（.../com.tauri-app.nova/skills/*/SKILL.md）。
      </CardContent>
    </Card>

    <div v-else class="flex flex-col gap-2">
      <Card
        v-for="skill in skills"
        :key="skill.path"
        class="gap-0 border-[#ebe9e3] py-3 dark:border-[#3b3a37]"
      >
        <CardHeader class="px-3 pb-1">
          <div class="flex min-w-0 items-center justify-between gap-3">
            <div class="min-w-0">
              <CardTitle class="truncate text-[13.5px] text-[#2a2820] dark:text-[#e8e3db]">{{ skill.name }}</CardTitle>
              <CardDescription class="mt-1 line-clamp-2 text-[12px] text-[#8a8478] dark:text-[#a09e99]">{{ skill.description }}</CardDescription>
              <div class="mt-1 truncate text-[11px] text-[#b0a99f] dark:text-[#66645e]" :title="skill.path">{{ skill.path }}</div>
            </div>
            <div class="flex shrink-0 items-center gap-2">
              <span
                class="rounded px-1.5 py-[1px] text-[11px]"
                :class="skill.enabled ? 'bg-[#edf7ed] text-[#3a7c3a] dark:bg-[#233323] dark:text-[#87c787]' : 'bg-[#f3f3f3] text-[#7b7b7b] dark:bg-[#2f2f2f] dark:text-[#9f9f9f]'"
              >{{ skill.enabled ? '已启用' : '已停用' }}</span>
              <Button
                variant="outline"
                size="sm"
                class="h-7 px-3 text-[12px]"
                @click="skill.enabled = !skill.enabled"
              >{{ skill.enabled ? '停用' : '启用' }}</Button>
            </div>
          </div>
        </CardHeader>
      </Card>
    </div>

    <div class="mt-auto border-t border-[#f0ece4] pt-4 dark:border-[#32312e]">
      <div v-if="error" class="mb-2 text-[12.5px] text-[#c0392b] dark:text-[#e57373]">{{ error }}</div>
      <div class="flex items-center justify-end gap-3">
        <span v-if="savedTip" class="text-[13px] text-[#4f9c64] dark:text-[#62c07a]">✓ 已保存</span>
        <Button
          size="sm"
          class="bg-[#da7756] text-white hover:bg-[#c06548]"
          :disabled="saving"
          @click="save"
        >{{ saving ? '保存中...' : '保存设置' }}</Button>
      </div>
    </div>
  </div>
</template>