<script setup lang="ts">
import { ref } from "vue";
import SettingsModal from "./settings/SettingsModal.vue";

interface ConversationItem {
  id: string;
  title: string;
}

const props = defineProps<{
  recents: ConversationItem[];
  activeConversationId: string;
}>();

const emit = defineEmits<{
  (e: "toggle-sidebar"): void;
  (e: "new-chat"): void;
  (e: "select-conversation", id: string): void;
  (e: "delete-conversation", id: string): void;
}>();

const isSettingsOpen = ref(false);
const openSettings = () => {
  isSettingsOpen.value = true;
};
</script>

<template>
  <aside class="w-[260px] flex-shrink-0 flex flex-col bg-[#faecd/30] bg-[#f9f9f8] dark:bg-[#1f1f1f] border-r border-[#e5e5e5] dark:border-[#333] transition-all duration-300">
    <div class="p-3 flex flex-col gap-1 overflow-y-auto flex-1 custom-scrollbar">
      <!-- Top Actions -->
      <button @click="emit('new-chat')" class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] transition-colors w-full text-left font-medium">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" class="text-muted-foreground"><path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        <span class="text-[0.9rem]">新对话</span>
      </button>
      <button class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] transition-colors w-full text-left font-medium">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" class="text-muted-foreground"><circle cx="11" cy="11" r="8" stroke="currentColor" stroke-width="2"/><path d="M21 21l-4.35-4.35" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
        <span class="text-[0.9rem]">搜索</span>
      </button>
      <button @click="openSettings" class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] transition-colors w-full text-left font-medium mb-4">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" class="text-muted-foreground"><path d="M12 20.5V20m0-16v-.5m0 0a2.5 2.5 0 100 5 2.5 2.5 0 000-5zm0 16a2.5 2.5 0 100-5 2.5 2.5 0 000 5zm-8.5-8H4m16 0h-.5m0 0a2.5 2.5 0 10-5 0 2.5 2.5 0 005 0zm-16 0a2.5 2.5 0 105 0 2.5 2.5 0 00-5 0z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        <span class="text-[0.9rem]">自定义</span>
      </button>

      <!-- 导航（已根据图片调整为中文标签与顺序） -->
      <h3 class="text-xs font-semibold text-[#8b8b8b] px-3 mt-2 mb-1">导航</h3>
      <button class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] transition-colors w-full text-left text-muted-foreground">
        <!-- 智能体（复用聊天气泡图标，样式与项目一致） -->
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        <span class="text-[0.9rem]">智能体</span>
      </button>
      <button class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] transition-colors w-full text-left text-muted-foreground">
        <!-- 技能（复用文件/项目图标） -->
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        <span class="text-[0.9rem]">技能</span>
      </button>
      <button class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] transition-colors w-full text-left text-muted-foreground">
        <!-- 指令（使用方形资源图标，视觉上与项目一致） -->
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"><rect x="3" y="3" width="18" height="18" rx="2" stroke="currentColor" stroke-width="2"/><path d="M3 9h18M9 21V9" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
        <span class="text-[0.9rem]">指令</span>
      </button>
      <button class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] transition-colors w-full text-left text-muted-foreground">
        <!-- 提示（复用 Code 风格箭头图标，简单明了） -->
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"><path d="M16 18l6-6-6-6M8 6l-6 6 6 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        <span class="text-[0.9rem]">提示</span>
      </button>
      <button class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] transition-colors w-full text-left text-muted-foreground">
        <!-- 挂钩（复用搜索图标样式） -->
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"><circle cx="11" cy="11" r="8" stroke="currentColor" stroke-width="2"/><path d="M21 21l-4.35-4.35" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
        <span class="text-[0.9rem]">挂钩</span>
      </button>
      <button class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] transition-colors w-full text-left text-muted-foreground">
        <!-- MCP 服务器（使用齿轮/自定义图标） -->
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"><path d="M12 20.5V20m0-16v-.5m0 0a2.5 2.5 0 100 5 2.5 2.5 0 000-5zm0 16a2.5 2.5 0 100-5 2.5 2.5 0 000 5z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        <span class="text-[0.9rem]">MCP 服务器</span>
      </button>
      <button class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] transition-colors w-full text-left text-muted-foreground mb-4">
        <!-- 插件（复用箭头/展开图标，与项目图标风格匹配） -->
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"><path d="M16 18l6-6-6-6M8 6l-6 6 6 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        <span class="text-[0.9rem]">插件</span>
      </button>

      <!-- Recents -->
      <h3 class="text-xs font-semibold text-[#8b8b8b] px-3 mt-2 mb-1">Recents</h3>
      <button 
        v-for="recent in props.recents" 
        :key="recent.id"
        class="group flex items-center gap-2 px-3 py-1.5 rounded-lg transition-colors w-full text-left text-[0.85rem]"
        :class="recent.id === props.activeConversationId
          ? 'bg-[#ebebeb] dark:bg-[#2d2d2d] text-[#222] dark:text-[#f2f2f2]'
          : 'hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] text-[#333] dark:text-[#ccc]'"
        @click="emit('select-conversation', recent.id)"
      >
        <span class="truncate block flex-1">{{ recent.title }}</span>
        <span
          class="opacity-0 group-hover:opacity-100 transition-opacity text-[#9b9b9b] hover:text-[#da7756] p-1 rounded"
          title="Delete conversation"
          @click.stop="emit('delete-conversation', recent.id)"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 6h18"/>
            <path d="M8 6V4h8v2"/>
            <path d="M19 6l-1 14H6L5 6"/>
            <path d="M10 11v6M14 11v6"/>
          </svg>
        </span>
      </button>
      <div v-if="props.recents.length === 0" class="px-3 py-1.5 text-[0.85rem] text-[#8b8b8b]">暂无历史会话</div>

    </div>

    <!-- User Profile -->
    <div @click="openSettings" class="p-3 border-t border-[#e5e5e5] dark:border-[#333] flex items-center justify-between hover:bg-[#ebebeb] dark:hover:bg-[#2d2d2d] transition-colors cursor-pointer rounded-b-xl">
      <div class="flex items-center gap-2">
        <div class="w-8 h-8 rounded-full bg-[#3d3d3d] text-white flex items-center justify-center font-medium text-sm">Y</div>
        <div class="flex flex-col">
          <span class="text-sm font-medium leading-tight">yileina</span>
          <span class="text-[0.7rem] text-muted-foreground leading-tight">Free plan</span>
        </div>
      </div>
      <div class="flex items-center gap-1">
        <button class="w-7 h-7 flex items-center justify-center rounded-md hover:bg-[#d4d4d4] dark:hover:bg-[#444] text-muted-foreground">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
        <button class="w-7 h-7 flex items-center justify-center rounded-md hover:bg-[#d4d4d4] dark:hover:bg-[#444] text-muted-foreground">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none"><path d="M8 9l4-4 4 4M16 15l-4 4-4-4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
      </div>
    </div>
    <SettingsModal v-model="isSettingsOpen" />
  </aside>
</template>

<style scoped>
.custom-scrollbar::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}
.custom-scrollbar::-webkit-scrollbar-track {
  background: transparent;
}
.custom-scrollbar::-webkit-scrollbar-thumb {
  background-color: var(--color-border, #e5e5e5);
  border-radius: 10px;
}
.dark .custom-scrollbar::-webkit-scrollbar-thumb {
  background-color: #444;
}
</style>
