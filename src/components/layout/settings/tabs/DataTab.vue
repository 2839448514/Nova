<script setup lang="ts">
import { ref } from 'vue'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'

const global = {
  setApiKey: (key: string | null) => console.log('setApiKey', key),
}

const confirmClear = ref(false)
const clearDone = ref(false)

const clearHistory = async () => {
  // await deleteAllConversations()
  console.log('deleteAllConversations')
  window.dispatchEvent(new Event('axon:history-cleared'))
  confirmClear.value = false
  clearDone.value = true
  setTimeout(() => clearDone.value = false, 2000)
}
</script>

<template>
  <div class="px-6 py-4 flex flex-col h-full overflow-y-auto">
    <Card class="mb-3 border-[#ebe9e3] dark:border-[#3b3a37]">
      <CardHeader class="px-4 pb-2">
        <CardTitle class="text-[13.5px] text-[#2a2820] dark:text-[#e8e3db]">聊天历史</CardTitle>
        <CardDescription class="text-[12px]">清除所有本地存储的对话记录，此操作不可撤销。</CardDescription>
      </CardHeader>
      <CardContent class="px-4 pt-0">
        <div>
          <Button
            v-if="!confirmClear"
            variant="outline"
            size="sm"
            class="border-[#e8c5c5] text-[#c0392b] hover:bg-[#fff0f0] dark:border-[#5c3a3a] dark:text-[#e57373] dark:hover:bg-[#3b2a2a]"
            @click="confirmClear = true"
          >清除历史</Button>
          <div v-else class="flex items-center gap-2">
            <span class="mr-1 text-[12px] text-[#c0392b] dark:text-[#e57373]">确定要清除？</span>
            <Button size="sm" variant="destructive" @click="clearHistory">确定</Button>
            <Button variant="outline" size="sm" @click="confirmClear = false">取消</Button>
          </div>
          <span v-if="clearDone" class="mt-1.5 inline-block text-[13px] font-medium text-[#4caf50] dark:text-[#62c07a]">✓ 已清除</span>
        </div>
      </CardContent>
    </Card>

    <Card class="mb-3 border-[#ebe9e3] dark:border-[#3b3a37]">
      <CardHeader class="px-4 pb-2">
        <CardTitle class="text-[13.5px] text-[#2a2820] dark:text-[#e8e3db]">API Key</CardTitle>
        <CardDescription class="text-[12px]">从本地存储中移除已保存的 API Key。</CardDescription>
      </CardHeader>
      <CardContent class="px-4 pt-0">
        <Button
          variant="outline"
          size="sm"
          class="border-[#e8c5c5] text-[#c0392b] hover:bg-[#fff0f0] dark:border-[#5c3a3a] dark:text-[#e57373] dark:hover:bg-[#3b2a2a]"
          @click="global.setApiKey(null)"
        >移除 Key</Button>
      </CardContent>
    </Card>
  </div>
</template>