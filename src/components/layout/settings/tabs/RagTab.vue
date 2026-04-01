<script setup lang="ts">
import { ref } from 'vue'

const global = {
  apiKey: 'fake-key',
  baseURL: 'https://api.openai.com/v1',
}

// TODO: Replace with actual backend logic
const getRAGDocumentCount = async () => 0
const addRAGData = async (apiKey: string, baseURL: string, content: string[]) => console.log('addRAGData', content)
const clearRAGData = async () => console.log('clearRAGData')

const textInput = ref('')
const importing = ref(false)
const clearing = ref(false)
const docCount = ref(0)
const status = ref('')
const statusType = ref<'success' | 'error' | 'muted'>('muted')

const setStatus = (msg: string, type: typeof statusType.value = 'muted') => {
  status.value = msg; statusType.value = type
}

const refresh = async () => {
  docCount.value = await getRAGDocumentCount()
  setStatus(
    docCount.value === 0 ? '当前知识库为空，导入文本后即可用于检索。' : `当前已保存 ${docCount.value} 段知识。`,
    docCount.value === 0 ? 'muted' : 'success'
  )
}

const importText = async () => {
  const apiKey = global.apiKey?.trim()
  const baseURL = global.baseURL.trim()
  const content = textInput.value.trim()
  if (!apiKey) { setStatus('请先在模型设置中保存 API Key。', 'error'); return }
  if (!content) { setStatus('请输入要导入的文本。', 'error'); return }
  importing.value = true
  try {
    await addRAGData(apiKey, baseURL, [content])
    textInput.value = ''
    await refresh()
    setStatus('文本已导入知识库。', 'success')
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    setStatus(msg.includes('MODEL_NOT_FOUND') ? `embedding 模型不可用: ${msg}` : '导入失败，请检查 API Key 或网络。', 'error')
  } finally { importing.value = false }
}

const clear = async () => {
  clearing.value = true
  try { await clearRAGData(); textInput.value = ''; await refresh(); setStatus('知识库已清空。', 'success') }
  catch { setStatus('清空失败，请稍后重试。', 'error') }
  finally { clearing.value = false }
}

defineExpose({ refresh })
refresh()
</script>

<template>
  <div class="px-6 py-4 flex flex-col h-full overflow-y-auto">
    <div class="flex flex-col gap-4 mb-4">
      <div class="flex gap-6 mb-5 px-[18px] py-[14px] bg-[#f5f4f0] dark:bg-[#252422] rounded-xl border border-[#ebe9e3] dark:border-[#3b3a37]">
        <div class="flex items-baseline gap-1.5">
          <span class="text-[24px] font-bold text-[#2a2820] dark:text-[#e8e3db] tracking-tight">{{ docCount }}</span>
          <span class="text-[12px] text-[#aaa49a] dark:text-[#88857f]">知识段</span>
        </div>
      </div>
      <div class="flex flex-col">
        <label class="text-[13px] font-semibold text-[#1a1915] dark:text-[#e8e3db] mb-[6px] uppercase tracking-wider">导入文本</label>
        <textarea v-model="textInput" class="w-full min-h-[140px] resize-y px-[12px] py-[10px] border border-[#ddd9d0] dark:border-[#44423f] rounded-xl bg-[#faf9f7] dark:bg-[#2e2d2a] text-[#2a2820] dark:text-[#d3d0c9] text-[13px] leading-relaxed outline-none font-sans focus:border-[#2a2820] dark:focus:border-[#d7a16f] focus:bg-white dark:focus:bg-[#2e2d2a] placeholder:text-[#b0a99f] dark:placeholder:text-[#66645e] transition-colors duration-150" rows="8" placeholder="粘贴笔记、说明文档、FAQ、代码背景或业务规则..."/>
      </div>
      <div class="flex gap-[10px] flex-wrap mb-2.5 mt-2">
        <button class="h-[36px] px-[16px] bg-[#da7756] text-white border-none rounded-lg font-medium text-[13px] cursor-pointer shadow-[0_1px_2px_rgba(0,0,0,0.05)] transition-colors duration-150 hover:bg-[#c06548] focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed" :disabled="importing" @click="importText">{{ importing ? '导入中...' : '导入文本' }}</button>
        <button class="h-[36px] px-[16px] bg-transparent border border-transparent text-[#c0392b] dark:text-[#e57373] rounded-lg font-medium text-[13px] cursor-pointer transition-colors duration-150 hover:bg-[#fff0f0] dark:hover:bg-[#3b2a2a] hover:border-[#ffcccc] dark:hover:border-[#5c3a3a] focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed" :disabled="clearing" @click="clear">{{ clearing ? '清空中...' : '清空知识库' }}</button>
      </div>
      <div v-if="status" class="text-[12.5px] leading-relaxed mb-3" :class="{
        'text-[#2e7d32] dark:text-[#62c07a]': statusType === 'success',
        'text-[#c0392b] dark:text-[#e57373]': statusType === 'error',
        'text-[#8a8478] dark:text-[#a09e99]': statusType === 'muted'
      }">{{ status }}</div>
      <div class="text-[12px] text-[#aaa49a] dark:text-[#88857f] leading-relaxed px-[12px] py-[10px] bg-[#f5f4f0] dark:bg-[#252422] rounded-xl border border-[#ebe9e3] dark:border-[#3b3a37]">导入后，对话时模型会自动检索相关内容。适合导入项目文档、规则、笔记等私有知识。</div>
    </div>
  </div>
</template>