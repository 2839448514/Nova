<script setup lang="ts">
</script>

<template>
  <div class="flex flex-col items-start pt-2">
    <div class="text-3xl font-extrabold text-[#1a1a1a] dark:text-[#ececec] tracking-tighter mb-1 font-serif flex items-center gap-2">
      <svg class="text-[#da7756]" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>
      </svg>
      Nova
    </div>
    <div class="text-xs text-muted-foreground mb-4 font-mono">v0.1.0</div>
    <p class="text-[0.95rem] text-[#666] dark:text-[#aaa] leading-relaxed mb-4 max-w-[360px]">
      基于 Tauri 2 + Vue 3 + TailwindCSS 构建的桌面 AI 助手，支持本地文件、Bash 与多 MCP Server 工具链。
    </p>

    <div class="flex items-center gap-2 mb-6">
      <a href="#" class="text-[0.85rem] font-medium text-[#1a1a1a] dark:text-[#ececec] hover:underline decoration-black/30 dark:decoration-white/30 underline-offset-2">GitHub</a>
      <span class="text-[#ccc] dark:text-[#555]">·</span>
      <a href="#" class="text-[0.85rem] font-medium text-[#1a1a1a] dark:text-[#ececec] hover:underline decoration-black/30 dark:decoration-white/30 underline-offset-2">Document</a>
      <span class="text-[#ccc] dark:text-[#555]">·</span>
      <a href="#" class="text-[0.85rem] font-medium text-[#1a1a1a] dark:text-[#ececec] hover:underline decoration-black/30 dark:decoration-white/30 underline-offset-2">Feedback</a>
    </div>

    <div class="flex flex-wrap gap-2">
      <span class="px-2.5 py-1 bg-[#f4f4f4] dark:bg-[#333] rounded-md text-xs text-[#555] dark:text-[#ccc] font-medium">Tauri v2</span>
      <span class="px-2.5 py-1 bg-[#f4f4f4] dark:bg-[#333] rounded-md text-xs text-[#555] dark:text-[#ccc] font-medium">Rust</span>
      <span class="px-2.5 py-1 bg-[#f4f4f4] dark:bg-[#333] rounded-md text-xs text-[#555] dark:text-[#ccc] font-medium">Vue 3</span>
      <span class="px-2.5 py-1 bg-[#f4f4f4] dark:bg-[#333] rounded-md text-xs text-[#555] dark:text-[#ccc] font-medium">TailwindCSS 4</span>
      <span class="px-2.5 py-1 bg-[#f4f4f4] dark:bg-[#333] rounded-md text-xs text-[#555] dark:text-[#ccc] font-medium">MCP Protocol</span>
    </div>
  </div>
</template>
