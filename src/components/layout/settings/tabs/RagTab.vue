<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { emitToast, normalizeErrorMessage } from '@/lib/toast'

type RagSettings = {
  embeddingModel: string
  chunkSize: number
  chunkOverlap: number
  maxFileSizeKb: number
}

type AppSettings = {
  rag?: Partial<RagSettings>
  [key: string]: unknown
}

type RagDocumentMeta = {
  id: string
  sourceName: string
  sourceType: string
  mimeType?: string
  contentChars: number
  preview: string
  checksum: string
  createdAt: number
  updatedAt: number
}

type RagStats = {
  documentCount: number
  totalChars: number
  lastUpdatedAt: number | null
}

type RagRejectedItem = {
  sourceName: string
  reason: string
}

type RagUpsertResult = {
  added: number
  updated: number
  rejected: RagRejectedItem[]
  totalDocuments: number
  totalChars: number
}

const DEFAULT_RAG_SETTINGS: RagSettings = {
  embeddingModel: '',
  chunkSize: 900,
  chunkOverlap: 120,
  maxFileSizeKb: 2048,
}

const MAX_FILES_PER_BATCH = 20
const SUPPORTED_EXTENSIONS = new Set([
  'txt', 'md', 'markdown', 'csv', 'json', 'yaml', 'yml', 'xml', 'log', 'sql',
  'js', 'jsx', 'ts', 'tsx', 'py', 'rs', 'java', 'go', 'c', 'cpp', 'h', 'hpp',
])

const textInput = ref('')
const embeddingModel = ref('')
const chunkSizeInput = ref(String(DEFAULT_RAG_SETTINGS.chunkSize))
const chunkOverlapInput = ref(String(DEFAULT_RAG_SETTINGS.chunkOverlap))
const maxFileSizeKbInput = ref(String(DEFAULT_RAG_SETTINGS.maxFileSizeKb))

const documents = ref<RagDocumentMeta[]>([])
const stats = ref<RagStats>({
  documentCount: 0,
  totalChars: 0,
  lastUpdatedAt: null,
})

const selectedFiles = ref<File[]>([])
const fileInputRef = ref<HTMLInputElement | null>(null)

const loading = ref(false)
const importingText = ref(false)
const uploadingFiles = ref(false)
const savingSettings = ref(false)
const clearing = ref(false)
const deletingDocumentId = ref<string | null>(null)
const confirmClear = ref(false)

const status = ref('')
const statusType = ref<'success' | 'error' | 'muted'>('muted')

const setStatus = (message: string, type: typeof statusType.value = 'muted') => {
  status.value = message
  statusType.value = type
}

const parsePositiveInt = (raw: string, fallback: number) => {
  const parsed = Number.parseInt(raw, 10)
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return fallback
  }
  return parsed
}

const normalizeRagSettings = (rag: Partial<RagSettings> | undefined): RagSettings => {
  const chunkSize = parsePositiveInt(String(rag?.chunkSize ?? ''), DEFAULT_RAG_SETTINGS.chunkSize)
  const chunkOverlap = parsePositiveInt(String(rag?.chunkOverlap ?? ''), DEFAULT_RAG_SETTINGS.chunkOverlap)
  const maxFileSizeKb = parsePositiveInt(String(rag?.maxFileSizeKb ?? ''), DEFAULT_RAG_SETTINGS.maxFileSizeKb)

  return {
    embeddingModel: String(rag?.embeddingModel ?? '').trim(),
    chunkSize,
    chunkOverlap: chunkOverlap >= chunkSize ? Math.max(chunkSize - 1, 1) : chunkOverlap,
    maxFileSizeKb,
  }
}

const maxFileSizeKb = computed(() => parsePositiveInt(maxFileSizeKbInput.value, DEFAULT_RAG_SETTINGS.maxFileSizeKb))
const maxFileSizeBytes = computed(() => maxFileSizeKb.value * 1024)
const isEmbeddingModelMissing = computed(() => embeddingModel.value.trim().length === 0)
const lastUpdatedText = computed(() => {
  if (!stats.value.lastUpdatedAt) {
    return '暂无'
  }
  return new Date(stats.value.lastUpdatedAt * 1000).toLocaleString()
})

const clearFileSelection = () => {
  selectedFiles.value = []
  if (fileInputRef.value) {
    fileInputRef.value.value = ''
  }
}

const extensionOf = (name: string) => {
  const index = name.lastIndexOf('.')
  if (index < 0) return ''
  return name.slice(index + 1).toLowerCase()
}

const ensureEmbeddingReady = () => {
  if (!isEmbeddingModelMissing.value) {
    return true
  }
  setStatus('请先在本页填写并保存 Embedding 模型，然后再导入知识库。', 'error')
  emitToast({ message: '未设置 Embedding 模型，无法导入 RAG 内容。', variant: 'error' })
  return false
}

const validateRagInputs = (): { ok: true; value: RagSettings } | { ok: false } => {
  const normalized: RagSettings = {
    embeddingModel: embeddingModel.value.trim(),
    chunkSize: parsePositiveInt(chunkSizeInput.value, DEFAULT_RAG_SETTINGS.chunkSize),
    chunkOverlap: parsePositiveInt(chunkOverlapInput.value, DEFAULT_RAG_SETTINGS.chunkOverlap),
    maxFileSizeKb: parsePositiveInt(maxFileSizeKbInput.value, DEFAULT_RAG_SETTINGS.maxFileSizeKb),
  }

  if (normalized.chunkSize < 100 || normalized.chunkSize > 8000) {
    setStatus('Chunk Size 需在 100 到 8000 之间。', 'error')
    return { ok: false }
  }
  if (normalized.chunkOverlap >= normalized.chunkSize) {
    setStatus('Chunk Overlap 必须小于 Chunk Size。', 'error')
    return { ok: false }
  }
  if (normalized.maxFileSizeKb < 64 || normalized.maxFileSizeKb > 10240) {
    setStatus('单文件大小上限需在 64KB 到 10240KB 之间。', 'error')
    return { ok: false }
  }

  return { ok: true, value: normalized }
}

const loadSettings = async () => {
  const settings = await invoke<AppSettings>('get_settings')
  const rag = normalizeRagSettings(settings.rag)
  embeddingModel.value = rag.embeddingModel
  chunkSizeInput.value = String(rag.chunkSize)
  chunkOverlapInput.value = String(rag.chunkOverlap)
  maxFileSizeKbInput.value = String(rag.maxFileSizeKb)
}

const loadKnowledgeBase = async () => {
  const [nextStats, nextDocs] = await Promise.all([
    invoke<RagStats>('rag_get_stats'),
    invoke<RagDocumentMeta[]>('rag_list_documents'),
  ])
  stats.value = nextStats
  documents.value = nextDocs
}

const refresh = async () => {
  loading.value = true
  try {
    await Promise.all([loadSettings(), loadKnowledgeBase()])
    if (stats.value.documentCount === 0) {
      setStatus('当前知识库为空。先保存 Embedding 模型，再导入文本或文件。', 'muted')
    } else {
      setStatus(
        `当前已索引 ${stats.value.documentCount} 条文档，总计 ${stats.value.totalChars.toLocaleString()} 字符。`,
        'success',
      )
    }
  } catch (error) {
    setStatus(`加载 RAG 数据失败：${normalizeErrorMessage(error)}`, 'error')
  } finally {
    loading.value = false
  }
}

const saveRagSettings = async () => {
  const validation = validateRagInputs()
  if (!validation.ok) {
    return
  }

  savingSettings.value = true
  try {
    const currentSettings = await invoke<AppSettings>('get_settings')
    const nextSettings = {
      ...currentSettings,
      rag: validation.value,
    }
    await invoke('save_settings', { settings: nextSettings })
    window.dispatchEvent(new CustomEvent('settings-updated'))

    setStatus('RAG 设置已保存。', 'success')
    emitToast({ message: 'RAG 设置已保存', variant: 'success' })
  } catch (error) {
    setStatus(`保存 RAG 设置失败：${normalizeErrorMessage(error)}`, 'error')
  } finally {
    savingSettings.value = false
  }
}

const summarizeUpsertResult = (result: RagUpsertResult, label: string) => {
  const rejectedCount = result.rejected.length
  const summary = `${label}导入完成：新增 ${result.added}，更新 ${result.updated}，拒绝 ${rejectedCount}。`
  const detail = rejectedCount > 0
    ? `拒绝示例：${result.rejected.slice(0, 2).map((v) => `${v.sourceName}(${v.reason})`).join('；')}`
    : ''
  return detail ? `${summary} ${detail}` : summary
}

const importText = async () => {
  if (!ensureEmbeddingReady()) {
    return
  }

  const content = textInput.value.trim()
  if (!content) {
    setStatus('请输入要导入的文本内容。', 'error')
    return
  }

  importingText.value = true
  try {
    const now = new Date().toISOString().slice(0, 19).replace('T', ' ')
    const result = await invoke<RagUpsertResult>('rag_upsert_documents', {
      documents: [
        {
          sourceName: `manual-${now}`,
          sourceType: 'text',
          mimeType: 'text/plain',
          content,
        },
      ],
    })

    await loadKnowledgeBase()
    textInput.value = ''

    const msg = summarizeUpsertResult(result, '文本')
    setStatus(msg, result.added > 0 || result.updated > 0 ? 'success' : 'error')
  } catch (error) {
    setStatus(`文本导入失败：${normalizeErrorMessage(error)}`, 'error')
  } finally {
    importingText.value = false
  }
}

const triggerFilePicker = () => {
  fileInputRef.value?.click()
}

const onFileChange = (event: Event) => {
  const input = event.target as HTMLInputElement
  selectedFiles.value = input.files ? Array.from(input.files) : []
}

const removeSelectedFile = (index: number) => {
  selectedFiles.value.splice(index, 1)
  if (selectedFiles.value.length === 0 && fileInputRef.value) {
    fileInputRef.value.value = ''
  }
}

const importFiles = async () => {
  if (!ensureEmbeddingReady()) {
    return
  }
  if (selectedFiles.value.length === 0) {
    setStatus('请先选择要导入的文件。', 'error')
    return
  }
  if (selectedFiles.value.length > MAX_FILES_PER_BATCH) {
    setStatus(`单次最多导入 ${MAX_FILES_PER_BATCH} 个文件。`, 'error')
    return
  }

  uploadingFiles.value = true
  try {
    const frontendRejected: RagRejectedItem[] = []
    const payload: Array<{ sourceName: string; sourceType: string; mimeType?: string; content: string }> = []

    for (const file of selectedFiles.value) {
      const ext = extensionOf(file.name)
      if (!SUPPORTED_EXTENSIONS.has(ext)) {
        frontendRejected.push({ sourceName: file.name, reason: `暂不支持的类型 .${ext || 'unknown'}` })
        continue
      }

      if (file.size > maxFileSizeBytes.value) {
        frontendRejected.push({
          sourceName: file.name,
          reason: `文件超过大小限制 ${maxFileSizeKb.value}KB`,
        })
        continue
      }

      const content = (await file.text()).trim()
      if (!content) {
        frontendRejected.push({ sourceName: file.name, reason: '文件内容为空' })
        continue
      }

      payload.push({
        sourceName: file.name,
        sourceType: 'file',
        mimeType: file.type || undefined,
        content,
      })
    }

    if (payload.length === 0) {
      setStatus('没有可导入的文件，请检查类型、大小和内容。', 'error')
      return
    }

    const backendResult = await invoke<RagUpsertResult>('rag_upsert_documents', {
      documents: payload,
    })

    const mergedResult: RagUpsertResult = {
      ...backendResult,
      rejected: [...backendResult.rejected, ...frontendRejected],
    }

    await loadKnowledgeBase()
    clearFileSelection()

    const msg = summarizeUpsertResult(mergedResult, '文件')
    setStatus(msg, mergedResult.added > 0 || mergedResult.updated > 0 ? 'success' : 'error')
  } catch (error) {
    setStatus(`文件导入失败：${normalizeErrorMessage(error)}`, 'error')
  } finally {
    uploadingFiles.value = false
  }
}

const removeDocument = async (id: string) => {
  deletingDocumentId.value = id
  try {
    const removed = await invoke<boolean>('rag_remove_document', { documentId: id })
    if (removed) {
      await loadKnowledgeBase()
      setStatus('文档已删除。', 'success')
      return
    }
    setStatus('文档不存在或已被删除。', 'muted')
  } catch (error) {
    setStatus(`删除文档失败：${normalizeErrorMessage(error)}`, 'error')
  } finally {
    deletingDocumentId.value = null
  }
}

const clearKnowledgeBase = async () => {
  clearing.value = true
  try {
    await invoke('rag_clear_documents')
    confirmClear.value = false
    await loadKnowledgeBase()
    setStatus('知识库已清空。', 'success')
    emitToast({ message: 'RAG 知识库已清空', variant: 'success' })
  } catch (error) {
    setStatus(`清空知识库失败：${normalizeErrorMessage(error)}`, 'error')
  } finally {
    clearing.value = false
  }
}

const formatDocumentTime = (timestamp: number) => new Date(timestamp * 1000).toLocaleString()

onMounted(() => {
  refresh()
})

defineExpose({ refresh })
</script>

<template>
  <div class="px-6 py-4 flex flex-col h-full overflow-y-auto gap-4">
    <Card class="border-[#ebe9e3] dark:border-[#3b3a37] bg-[#f7f6f2] dark:bg-[#252422] py-4 gap-4">
      <CardContent class="px-4">
        <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
          <div class="rounded-lg border border-[#e6e1d6] dark:border-[#3b3a37] bg-white/80 dark:bg-[#1f1e1b] p-3">
            <div class="text-[12px] text-[#8a8478] dark:text-[#a09e99] mb-1">文档数</div>
            <div class="text-[20px] font-semibold text-[#2a2820] dark:text-[#e8e3db]">{{ stats.documentCount }}</div>
          </div>
          <div class="rounded-lg border border-[#e6e1d6] dark:border-[#3b3a37] bg-white/80 dark:bg-[#1f1e1b] p-3">
            <div class="text-[12px] text-[#8a8478] dark:text-[#a09e99] mb-1">总字符</div>
            <div class="text-[20px] font-semibold text-[#2a2820] dark:text-[#e8e3db]">{{ stats.totalChars.toLocaleString() }}</div>
          </div>
          <div class="rounded-lg border border-[#e6e1d6] dark:border-[#3b3a37] bg-white/80 dark:bg-[#1f1e1b] p-3">
            <div class="text-[12px] text-[#8a8478] dark:text-[#a09e99] mb-1">最后更新</div>
            <div class="text-[13px] font-medium text-[#2a2820] dark:text-[#e8e3db] truncate">{{ lastUpdatedText }}</div>
          </div>
        </div>
      </CardContent>
    </Card>

    <Card class="border-[#ebe9e3] dark:border-[#3b3a37] py-5 gap-4">
      <CardHeader class="pb-2">
        <CardTitle class="text-[15px] text-[#2a2820] dark:text-[#e8e3db]">RAG 参数设置</CardTitle>
        <CardDescription>Embedding 模型为空时将无法导入文档。</CardDescription>
      </CardHeader>
      <CardContent class="space-y-3">
        <div class="space-y-1.5">
          <Label class="text-[13px] text-[#2a2820] dark:text-[#e8e3db]">Embedding Model</Label>
          <Input
            v-model="embeddingModel"
            placeholder="例如：text-embedding-3-large"
            class="h-9 border-[#ddd9d0] dark:border-[#44423f]"
          />
        </div>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
          <div class="space-y-1.5">
            <Label class="text-[12px] text-[#6b6456] dark:text-[#a09e99]">Chunk Size</Label>
            <Input v-model="chunkSizeInput" type="number" min="100" max="8000" class="h-9" />
          </div>
          <div class="space-y-1.5">
            <Label class="text-[12px] text-[#6b6456] dark:text-[#a09e99]">Chunk Overlap</Label>
            <Input v-model="chunkOverlapInput" type="number" min="1" max="7999" class="h-9" />
          </div>
          <div class="space-y-1.5">
            <Label class="text-[12px] text-[#6b6456] dark:text-[#a09e99]">文件大小上限 (KB)</Label>
            <Input v-model="maxFileSizeKbInput" type="number" min="64" max="10240" class="h-9" />
          </div>
        </div>
        <div
          v-if="isEmbeddingModelMissing"
          class="rounded-md border border-[#f2c9c9] dark:border-[#5c3a3a] bg-[#fff6f6] dark:bg-[#3b2a2a] px-3 py-2 text-[12px] text-[#b24a4a] dark:text-[#f0aaaa]"
        >
          Embedding 模型未填写。请先保存后再导入文本或文件。
        </div>
      </CardContent>
      <CardFooter class="justify-end">
        <Button
          class="bg-[#da7756] hover:bg-[#c06548] text-white"
          :disabled="savingSettings"
          @click="saveRagSettings"
        >
          {{ savingSettings ? '保存中...' : '保存 RAG 设置' }}
        </Button>
      </CardFooter>
    </Card>

    <Card class="border-[#ebe9e3] dark:border-[#3b3a37] py-5 gap-4">
      <CardHeader class="pb-2">
        <CardTitle class="text-[15px] text-[#2a2820] dark:text-[#e8e3db]">导入知识</CardTitle>
        <CardDescription>支持粘贴文本或上传文件。建议先上传结构化文档（md/txt/csv/json）。</CardDescription>
      </CardHeader>
      <CardContent class="space-y-3">
        <div class="space-y-1.5">
          <Label class="text-[13px] text-[#2a2820] dark:text-[#e8e3db]">文本导入</Label>
          <Textarea
            v-model="textInput"
            class="min-h-[130px] border-[#ddd9d0] dark:border-[#44423f]"
            placeholder="粘贴 SOP、FAQ、产品说明、项目文档等..."
          />
        </div>

        <div class="flex flex-wrap gap-2">
          <Button
            class="bg-[#da7756] hover:bg-[#c06548] text-white"
            :disabled="importingText || uploadingFiles"
            @click="importText"
          >
            {{ importingText ? '文本导入中...' : '导入文本' }}
          </Button>
          <Button variant="outline" :disabled="uploadingFiles" @click="triggerFilePicker">
            选择文件
          </Button>
          <Button variant="outline" :disabled="uploadingFiles || selectedFiles.length === 0" @click="importFiles">
            {{ uploadingFiles ? '文件导入中...' : `导入文件 (${selectedFiles.length})` }}
          </Button>
          <input
            ref="fileInputRef"
            type="file"
            class="hidden"
            multiple
            @change="onFileChange"
          >
        </div>

        <div class="text-[12px] text-[#8a8478] dark:text-[#a09e99]">
          单次最多 {{ MAX_FILES_PER_BATCH }} 个文件，单文件上限 {{ maxFileSizeKb }}KB，支持扩展名：{{ Array.from(SUPPORTED_EXTENSIONS).join(', ') }}
        </div>

        <div v-if="selectedFiles.length > 0" class="rounded-lg border border-[#ebe9e3] dark:border-[#3b3a37] p-2.5 space-y-1.5">
          <div
            v-for="(file, index) in selectedFiles"
            :key="`${file.name}-${file.size}-${index}`"
            class="flex items-center justify-between gap-3 rounded-md bg-[#faf9f7] dark:bg-[#2b2a27] px-2.5 py-2 text-[12px]"
          >
            <div class="min-w-0">
              <div class="truncate text-[#2a2820] dark:text-[#e8e3db]">{{ file.name }}</div>
              <div class="text-[#8a8478] dark:text-[#a09e99]">{{ Math.ceil(file.size / 1024) }} KB</div>
            </div>
            <Button size="sm" variant="ghost" class="h-7 px-2" @click="removeSelectedFile(index)">
              移除
            </Button>
          </div>
        </div>
      </CardContent>
    </Card>

    <Card class="border-[#ebe9e3] dark:border-[#3b3a37] py-5 gap-4">
      <CardHeader class="pb-2">
        <CardTitle class="text-[15px] text-[#2a2820] dark:text-[#e8e3db]">知识库文档</CardTitle>
        <CardDescription>管理已导入文档，删除后不会再参与后续检索。</CardDescription>
      </CardHeader>
      <CardContent class="space-y-2.5">
        <div v-if="loading" class="text-[13px] text-[#8a8478] dark:text-[#a09e99]">加载中...</div>
        <div v-else-if="documents.length === 0" class="text-[13px] text-[#8a8478] dark:text-[#a09e99]">
          暂无文档。可先导入文本或上传文件。
        </div>
        <div v-else class="space-y-2">
          <div
            v-for="doc in documents"
            :key="doc.id"
            class="rounded-lg border border-[#ebe9e3] dark:border-[#3b3a37] p-3 bg-[#faf9f7] dark:bg-[#2b2a27]"
          >
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <div class="text-[13px] font-medium text-[#2a2820] dark:text-[#e8e3db] truncate">{{ doc.sourceName }}</div>
                <div class="text-[12px] text-[#8a8478] dark:text-[#a09e99] mt-0.5">
                  {{ doc.sourceType }} · {{ doc.contentChars.toLocaleString() }} 字符 · 更新于 {{ formatDocumentTime(doc.updatedAt) }}
                </div>
                <div class="text-[12px] text-[#6b6456] dark:text-[#b6b2aa] mt-1.5 leading-relaxed">
                  {{ doc.preview || '（无预览内容）' }}
                </div>
              </div>
              <Button
                size="sm"
                variant="ghost"
                class="text-[#c0392b] hover:text-[#a93226]"
                :disabled="deletingDocumentId === doc.id"
                @click="removeDocument(doc.id)"
              >
                {{ deletingDocumentId === doc.id ? '删除中...' : '删除' }}
              </Button>
            </div>
          </div>
        </div>
      </CardContent>
      <CardFooter class="justify-end gap-2">
        <Button
          v-if="!confirmClear"
          variant="outline"
          class="text-[#c0392b] border-[#f0d7d5] hover:bg-[#fff3f2]"
          :disabled="clearing"
          @click="confirmClear = true"
        >
          清空知识库
        </Button>
        <template v-else>
          <Button variant="outline" :disabled="clearing" @click="confirmClear = false">取消</Button>
          <Button
            variant="destructive"
            :disabled="clearing"
            @click="clearKnowledgeBase"
          >
            {{ clearing ? '清空中...' : '确认清空' }}
          </Button>
        </template>
      </CardFooter>
    </Card>

    <div
      v-if="status"
      class="text-[12.5px] leading-relaxed px-3 py-2 rounded-md border"
      :class="{
        'text-[#2e7d32] dark:text-[#9de5ad] bg-[#f2fbf4] dark:bg-[#1f3325] border-[#cde8d3] dark:border-[#3a6b48]': statusType === 'success',
        'text-[#c0392b] dark:text-[#ffb3b3] bg-[#fff4f4] dark:bg-[#3a2222] border-[#f2c9c9] dark:border-[#6a3535]': statusType === 'error',
        'text-[#6b6456] dark:text-[#b6b2aa] bg-[#f7f5ef] dark:bg-[#252422] border-[#ebe9e3] dark:border-[#3b3a37]': statusType === 'muted',
      }"
    >
      {{ status }}
    </div>
  </div>
</template>
