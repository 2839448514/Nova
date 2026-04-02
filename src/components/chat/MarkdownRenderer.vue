<script setup lang="ts">
import { computed } from 'vue'
import MarkdownIt from 'markdown-it'
import hljs from 'highlight.js'
import markdownItKatex from '@traptitech/markdown-it-katex'

const props = defineProps<{ content: string }>()

const md = new MarkdownIt({
  html: true,
  linkify: true,
  typographer: true,
  highlight(code: string, lang: string): string {
    if (lang && hljs.getLanguage(lang)) {
      try {
        return `<pre class="hljs-block"><div class="hljs-header"><span class="hljs-lang">${lang}</span><button class="hljs-copy" onclick="navigator.clipboard.writeText(this.closest('pre').querySelector('code').innerText)">复制</button></div><code class="hljs language-${lang}">${hljs.highlight(code, { language: lang }).value}</code></pre>`
      } catch { }
    }
    return `<pre class="hljs-block"><code class="hljs">${md.utils.escapeHtml(code)}</code></pre>`
  },
})

md.use(markdownItKatex)

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const defaultRender: any = md.renderer.rules.link_open
  || function (tokens: any[], idx: number, options: any, _env: any, self: any) {
    return self.renderToken(tokens, idx, options)
  }

// eslint-disable-next-line @typescript-eslint/no-explicit-any
md.renderer.rules.link_open = function (tokens: any[], idx: number, options: any, env: any, self: any): string {
  tokens[idx].attrSet('target', '_blank')
  tokens[idx].attrSet('rel', 'noopener noreferrer')
  return defaultRender(tokens, idx, options, env, self)
}

const rendered = computed(() => {
  let html = md.render(props.content || '')

  // 对 <details> 内部（<summary> 之后的部分）再跑一次 Markdown 渲染
  html = html.replace(
    /(<details[^>]*>)([\s\S]*?)(<\/details>)/g,
    (_: string, open: string, inner: string, close: string) => {
      const processed = inner.replace(
        /(<\/summary>)([\s\S]*?)$/,
        (__: string, summaryClose: string, rest: string) => {
          const trimmed = rest.trim()
          if (!trimmed) return summaryClose
          return summaryClose + '<div class="details-body">' + md.render(trimmed) + '</div>'
        }
      )
      return open + processed + close
    }
  )

  return html
})
</script>

<template>
  <div class="md-body" v-html="rendered" />
</template>

<style>
@import 'highlight.js/styles/github-dark.css';
@import 'katex/dist/katex.min.css';

.md-body {
  font-size: 14px;
  line-height: 1.75;
  color: inherit;
  word-break: break-word;
}

.md-body>*:first-child {
  margin-top: 0 !important;
}

.md-body>*:last-child {
  margin-bottom: 0 !important;
}

.md-body h1,
.md-body h2,
.md-body h3,
.md-body h4,
.md-body h5,
.md-body h6 {
  font-weight: 700;
  line-height: 1.3;
  margin: 1.2em 0 0.5em;
  color: #1a1a18;
}

.md-body h1 {
  font-size: 1.5em;
}

.md-body h2 {
  font-size: 1.25em;
  border-bottom: 1px solid #ebe9e3;
  padding-bottom: 0.3em;
}

.md-body h3 {
  font-size: 1.1em;
}

.md-body p {
  margin: 0.6em 0;
}

.md-body strong {
  font-weight: 700;
  color: #1a1a18;
}

.md-body em {
  font-style: italic;
}

.md-body code:not(.hljs) {
  font-family: 'SF Mono', 'Fira Code', 'Cascadia Code', monospace;
  font-size: 0.85em;
  background: #f0ede7;
  color: #b03a2e;
  padding: 1px 5px;
  border-radius: 4px;
  border: 1px solid #e5e1d8;
}

.hljs-block {
  margin: 0.8em 0;
  border-radius: 10px;
  overflow: hidden;
  border: 1px solid #e5e1d8;
  background: #1e1e1e;
}

.hljs-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 14px;
  background: #2a2a2a;
  border-bottom: 1px solid #333;
}

.hljs-lang {
  font-size: 11px;
  color: #888;
  font-family: 'SF Mono', monospace;
  text-transform: lowercase;
}

.hljs-copy {
  font-size: 11px;
  color: #888;
  background: transparent;
  border: 1px solid #444;
  border-radius: 4px;
  padding: 2px 8px;
  cursor: pointer;
  transition: all 0.15s;
  font-family: system-ui, sans-serif;
}

.hljs-copy:hover {
  color: #ccc;
  border-color: #666;
}

.hljs-block code {
  display: block;
  padding: 14px 16px;
  overflow-x: auto;
  font-size: 13px;
  line-height: 1.6;
  font-family: 'SF Mono', 'Fira Code', 'Cascadia Code', monospace;
}

.md-body blockquote {
  margin: 0.8em 0;
  padding: 10px 16px;
  border-left: 3px solid #c8c4bb;
  background: #f9f8f5;
  border-radius: 0 6px 6px 0;
  color: #6b6456;
}

.md-body blockquote p {
  margin: 0;
}

.md-body ul,
.md-body ol {
  margin: 0.5em 0;
  padding-left: 1.6em;
}

.md-body li {
  margin: 0.25em 0;
}

.md-body li::marker {
  color: #aaa49a;
}

.md-body ul>li {
  list-style-type: disc;
}

.md-body ol>li {
  list-style-type: decimal;
}

.md-body table {
  border-collapse: collapse;
  margin: 0.8em 0;
  font-size: 13px;
  display: block;
  overflow-x: auto;
  max-width: 100%;
}

.md-body th {
  background: #f5f4f0;
  font-weight: 600;
  color: #2a2820;
  padding: 8px 12px;
  border: 1px solid #e5e1d8;
  text-align: left;
  white-space: nowrap;
}

.md-body td {
  padding: 7px 12px;
  border: 1px solid #e5e1d8;
  color: #3d3929;
  white-space: nowrap;
}

.md-body tr:nth-child(even) td {
  background: #faf9f7;
}

.md-body hr {
  border: none;
  border-top: 1px solid #e5e1d8;
  margin: 1.2em 0;
}

.md-body a {
  color: #2a6496;
  text-decoration: none;
}

.md-body a:hover {
  text-decoration: underline;
}

.md-body img {
  max-width: 100%;
  border-radius: 6px;
}

.md-body .katex-display {
  margin: 0.8em 0;
  overflow-x: auto;
  overflow-y: hidden;
}

.md-body .katex {
  font-size: 1em;
}

/* details / summary */
.md-body details {
  margin: 0.8em 0;
  border: 1px solid #e5e1d8;
  border-radius: 6px;
  overflow: hidden;
}

.md-body summary {
  padding: 8px 14px;
  cursor: pointer;
  font-weight: 500;
  background: #f9f8f5;
  user-select: none;
  list-style: revert;
}

.md-body summary:hover {
  background: #f0ede7;
}

.md-body .details-body {
  padding: 8px 14px;
}

.md-body .details-body>*:first-child {
  margin-top: 0;
}

.md-body .details-body>*:last-child {
  margin-bottom: 0;
}
</style>