<script setup lang="ts">
import { ref } from 'vue'

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
    <div class="flex items-center justify-between gap-4 p-4 border border-[#ebe9e3] dark:border-[#3b3a37] rounded-xl mb-3 transition-colors duration-150 hover:border-[#d8d5cc] dark:hover:border-[#52504b]">
      <div class="flex items-start gap-3">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="w-5 h-5 text-[#aaa49a] dark:text-[#88857f] shrink-0 mt-[1px]">
          <path d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-3 3v-3z" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
        <div>
          <div class="text-[13.5px] font-semibold text-[#2a2820] dark:text-[#e8e3db] mb-[3px]">聊天历史</div>
          <div class="text-[12px] text-[#aaa49a] dark:text-[#88857f] leading-relaxed">清除所有本地存储的对话记录，此操作不可撤销。</div>
        </div>
      </div>
      <div>
        <button v-if="!confirmClear" class="h-9 px-4 bg-transparent border border-transparent text-[#c0392b] dark:text-[#e57373] rounded-lg font-medium text-[13px] cursor-pointer transition-colors duration-150 hover:bg-[#fff0f0] dark:hover:bg-[#3b2a2a] hover:border-[#ffcccc] dark:hover:border-[#5c3a3a] focus:outline-none" @click="confirmClear = true">清除历史</button>
        <div v-else class="flex items-center gap-2">
          <span class="text-[12px] text-[#c0392b] dark:text-[#e57373] mr-1">确定要清除？</span>
          <button class="h-9 px-4 bg-[#c0392b] dark:bg-[#d32f2f] text-white border-none rounded-lg font-medium text-[13px] cursor-pointer shadow-[0_1px_2px_rgba(0,0,0,0.1)] transition-colors duration-150 hover:bg-[#a93226] focus:outline-none" @click="clearHistory">确定</button>
          <button class="h-9 px-4 bg-transparent border border-[#ddd9d0] dark:border-[#44423f] text-[#6b6456] dark:text-[#a09e99] rounded-lg font-medium text-[13px] cursor-pointer transition-colors duration-150 hover:bg-[#f5f4f0] dark:hover:bg-[#32312e] focus:outline-none" @click="confirmClear = false">取消</button>
        </div>
        <span v-if="clearDone" class="text-[13px] text-[#4caf50] dark:text-[#62c07a] font-medium inline-block mt-1.5 transition-opacity duration-300 opacity-100 data-[state=leave]:opacity-0">✓ 已清除</span>
      </div>
    </div>

    <div class="flex items-center justify-between gap-4 p-4 border border-[#ebe9e3] dark:border-[#3b3a37] rounded-xl mb-3 transition-colors duration-150 hover:border-[#d8d5cc] dark:hover:border-[#52504b]">
      <div class="flex items-start gap-3">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="w-5 h-5 text-[#aaa49a] dark:text-[#88857f] shrink-0 mt-[1px]">
          <path d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
        <div>
          <div class="text-[13.5px] font-semibold text-[#2a2820] dark:text-[#e8e3db] mb-[3px]">API Key</div>
          <div class="text-[12px] text-[#aaa49a] dark:text-[#88857f] leading-relaxed">从本地存储中移除已保存的 API Key。</div>
        </div>
      </div>
      <button class="h-9 px-4 bg-transparent border border-transparent text-[#c0392b] dark:text-[#e57373] rounded-lg font-medium text-[13px] cursor-pointer transition-colors duration-150 hover:bg-[#fff0f0] dark:hover:bg-[#3b2a2a] hover:border-[#ffcccc] dark:hover:border-[#5c3a3a] focus:outline-none" @click="global.setApiKey(null)">移除 Key</button>
    </div>
  </div>
</template>