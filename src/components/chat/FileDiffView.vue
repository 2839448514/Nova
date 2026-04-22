<script setup lang="ts">
import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { ToolExecutionEntry } from '../../lib/chat-types';

const props = defineProps<{ entries: ToolExecutionEntry[]; codingMode?: boolean; workspaceReady?: boolean }>();

// ── Types ────────────────────────────────────────────────────────────────────

type LineKind = 'add' | 'del' | 'eq';

interface DiffLine {
  kind: LineKind;
  text: string;
  oldLine?: number;
  newLine?: number;
}

interface DiffRow {
  type: 'line';
  line: DiffLine;
}
interface FoldRow {
  type: 'fold';
  count: number;
}
type Row = DiffRow | FoldRow;

interface FileDiff {
  entryId: string;
  path: string;
  filename: string;
  toolBadge: 'edit' | 'write' | 'shell';
  kind: 'diff' | 'shell';
  shellCommand?: string;
  shellResult?: string;
  lines: DiffLine[];
  rows: Row[];         // with context folding
  added: number;
  removed: number;
  status: ToolExecutionEntry['status'];
}

// ── Diff algorithm ───────────────────────────────────────────────────────────

/** Compute LCS index pairs for two string arrays. Falls back to empty for large inputs. */
function computeLCS(a: string[], b: string[]): [number, number][] {
  const m = a.length, n = b.length;
  if (m === 0 || n === 0) return [];
  // O(m*n) memory guard – for very large diffs just show all as changed
  if (m > 600 || n > 600) return [];

  const stride = n + 1;
  const dp = new Uint16Array((m + 1) * stride);

  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      if (a[i - 1] === b[j - 1]) {
        dp[i * stride + j] = dp[(i - 1) * stride + (j - 1)] + 1;
      } else {
        const top  = dp[(i - 1) * stride + j];
        const left = dp[i * stride + (j - 1)];
        dp[i * stride + j] = top >= left ? top : left;
      }
    }
  }

  const pairs: [number, number][] = [];
  let i = m, j = n;
  while (i > 0 && j > 0) {
    if (a[i - 1] === b[j - 1]) {
      pairs.unshift([i - 1, j - 1]);
      i--; j--;
    } else if (dp[(i - 1) * stride + j] >= dp[i * stride + (j - 1)]) {
      i--;
    } else {
      j--;
    }
  }
  return pairs;
}

/** Diff two text strings and return annotated lines with line numbers. */
function diffText(before: string, after: string): DiffLine[] {
  const aLines = before ? before.split('\n') : [];
  const bLines = after  ? after.split('\n')  : [];

  if (aLines.length === 0 && bLines.length === 0) return [];

  const lcs = computeLCS(aLines, bLines);
  const result: DiffLine[] = [];
  let ai = 0, bi = 0, oldNo = 1, newNo = 1;

  for (const [li, ri] of lcs) {
    while (ai < li) result.push({ kind: 'del', text: aLines[ai++], oldLine: oldNo++ });
    while (bi < ri) result.push({ kind: 'add', text: bLines[bi++], newLine: newNo++ });
    result.push({ kind: 'eq', text: aLines[ai++], oldLine: oldNo++, newLine: newNo++ });
    bi++;
  }
  while (ai < aLines.length) result.push({ kind: 'del', text: aLines[ai++], oldLine: oldNo++ });
  while (bi < bLines.length) result.push({ kind: 'add', text: bLines[bi++], newLine: newNo++ });

  return result;
}

/** Collapse long runs of unchanged context lines, keeping CONTEXT_SIZE lines around changes. */
const CONTEXT_SIZE = 3;

function buildRows(lines: DiffLine[]): Row[] {
  const n = lines.length;
  const rows: Row[] = [];

  // Mark which eq lines are "near" a change
  const keep = new Uint8Array(n);
  for (let i = 0; i < n; i++) {
    if (lines[i].kind !== 'eq') {
      const lo = Math.max(0, i - CONTEXT_SIZE);
      const hi = Math.min(n - 1, i + CONTEXT_SIZE);
      for (let k = lo; k <= hi; k++) keep[k] = 1;
    }
  }

  let i = 0;
  while (i < n) {
    if (lines[i].kind === 'eq' && !keep[i]) {
      let j = i;
      while (j < n && lines[j].kind === 'eq' && !keep[j]) j++;
      const count = j - i;
      if (count >= 5) {
        rows.push({ type: 'fold', count });
        i = j;
        continue;
      }
    }
    rows.push({ type: 'line', line: lines[i] });
    i++;
  }
  return rows;
}

// ── Parsing ──────────────────────────────────────────────────────────────────

const FILE_EDIT_TOOLS = new Set(['replace_string_in_file', 'write_file']);
const SHELL_TOOLS = new Set(['execute_powershell', 'bash', 'shell']);

function parseEntry(entry: ToolExecutionEntry): FileDiff | null {
  let input: Record<string, unknown>;
  try {
    input = JSON.parse(entry.input) as Record<string, unknown>;
  } catch {
    return null;
  }

  const rawPath = (input.path as string | undefined) ?? '';
  const filename = rawPath.split(/[\\/]/).pop() ?? rawPath;

  if (entry.toolName === 'replace_string_in_file') {
    const oldStr = (input.old_string as string | undefined) ?? '';
    const newStr = (input.new_string as string | undefined) ?? '';
    const lines = diffText(oldStr, newStr);
    return {
      entryId: entry.id,
      path: rawPath,
      filename,
      toolBadge: 'edit',
      kind: 'diff',
      lines,
      rows: buildRows(lines),
      added:   lines.filter((l) => l.kind === 'add').length,
      removed: lines.filter((l) => l.kind === 'del').length,
      status: entry.status,
    };
  }

  if (entry.toolName === 'write_file') {
    const content = (input.content as string | undefined) ?? '';
    const lines: DiffLine[] = content.split('\n').map((text, i) => ({
      kind: 'add' as LineKind,
      text,
      newLine: i + 1,
    }));
    return {
      entryId: entry.id,
      path: rawPath,
      filename,
      toolBadge: 'write',
      kind: 'diff',
      lines,
      rows: lines.map((l) => ({ type: 'line' as const, line: l })),
      added:   lines.length,
      removed: 0,
      status: entry.status,
    };
  }

  // Shell tools: display command + output, no line diff
  if (SHELL_TOOLS.has(entry.toolName)) {
    const command = (input.command as string | undefined) ?? entry.input;
    return {
      entryId: entry.id,
      path: command.trim().slice(0, 120),
      filename: entry.toolName,
      toolBadge: 'shell',
      kind: 'shell',
      shellCommand: command,
      shellResult: entry.result,
      lines: [],
      rows: [],
      added: 0,
      removed: 0,
      status: entry.status,
    };
  }

  return null;
}

// ── State ────────────────────────────────────────────────────────────────────

const fileDiffs = computed<FileDiff[]>(() =>
  props.entries
    .filter((e) => FILE_EDIT_TOOLS.has(e.toolName) || SHELL_TOOLS.has(e.toolName))
    .map(parseEntry)
    .filter((d): d is FileDiff => d !== null),
);

const totalAdded   = computed(() => fileDiffs.value.reduce((s, d) => s + d.added, 0));
const totalRemoved = computed(() => fileDiffs.value.reduce((s, d) => s + d.removed, 0));

const selectedId = ref<string | null>(null);
const toggleFile = (id: string) => {
  selectedId.value = selectedId.value === id ? null : id;
};

// ── Revert ───────────────────────────────────────────────────────────────────

const revertingId   = ref<string | null>(null);
const acceptingId   = ref<string | null>(null);
// 'reverted' | 'accepted' | null
const resolvedState = ref<Record<string, 'reverted' | 'accepted'>>({});
const actionErrors  = ref<Record<string, string>>({});

async function handleRevert(diff: FileDiff) {
  const entry = props.entries.find((e) => e.id === diff.entryId);
  if (!entry) return;
  revertingId.value = diff.entryId;
  delete actionErrors.value[diff.entryId];
  try {
    if (diff.toolBadge === 'edit') {
      let input: Record<string, unknown>;
      try { input = JSON.parse(entry.input) as Record<string, unknown>; }
      catch { throw new Error('无法解析工具输入'); }
      await invoke('revert_file_edit', {
        path:      input.path       as string,
        oldString: input.old_string as string,
        newString: input.new_string as string,
      });
    } else if (diff.toolBadge === 'write') {
      let input: Record<string, unknown>;
      try { input = JSON.parse(entry.input) as Record<string, unknown>; }
      catch { throw new Error('无法解析工具输入'); }
      await invoke('revert_write_file', { path: input.path as string });
    }
    resolvedState.value[diff.entryId] = 'reverted';
  } catch (e) {
    actionErrors.value[diff.entryId] = String(e);
  } finally {
    revertingId.value = null;
  }
}

async function handleAccept(diff: FileDiff) {
  if (diff.toolBadge !== 'write') {
    // replace_string_in_file has no cleanup needed; just mark accepted
    resolvedState.value[diff.entryId] = 'accepted';
    return;
  }
  const entry = props.entries.find((e) => e.id === diff.entryId);
  if (!entry) return;
  acceptingId.value = diff.entryId;
  delete actionErrors.value[diff.entryId];
  try {
    let input: Record<string, unknown>;
    try { input = JSON.parse(entry.input) as Record<string, unknown>; }
    catch { throw new Error('无法解析工具输入'); }
    await invoke('accept_write_file', { path: input.path as string });
    resolvedState.value[diff.entryId] = 'accepted';
  } catch (e) {
    actionErrors.value[diff.entryId] = String(e);
  } finally {
    acceptingId.value = null;
  }
}
</script>

<template>
  <div class="flex flex-col h-full overflow-hidden text-sm select-none">

    <!-- ── Empty state ──────────────────────────────────────────────────── -->
    <div
      v-if="fileDiffs.length === 0"
      class="flex flex-col items-center justify-center h-full gap-3 text-muted-foreground"
    >
      <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor"
        stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="opacity-30">
        <polyline points="16 18 22 12 16 6"/>
        <polyline points="8 6 2 12 8 18"/>
      </svg>
      <p class="text-sm">当前对话暂无文件改动</p>
      <p class="text-xs text-muted-foreground/60">支持工具：<code class="font-mono">replace_string_in_file</code> · <code class="font-mono">write_file</code> · <code class="font-mono">execute_powershell</code></p>
    </div>

    <template v-else>
      <!-- ── Stats bar ───────────────────────────────────────────────────── -->
      <div class="flex items-center gap-3 px-4 py-2 border-b border-[#e7e2d7] dark:border-[#333] shrink-0 text-xs text-muted-foreground bg-[#faf9f6] dark:bg-[#1a1a1a]">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
        </svg>
        <span>{{ fileDiffs.length }} 个文件改动</span>
        <span class="font-mono text-green-600 dark:text-green-400">+{{ totalAdded }}</span>
        <span class="font-mono text-red-500 dark:text-red-400">-{{ totalRemoved }}</span>
      </div>

      <!-- ── File list ───────────────────────────────────────────────────── -->
      <div class="flex-1 overflow-y-auto diff-scrollbar">
        <div
          v-for="diff in fileDiffs"
          :key="diff.entryId"
          class="border-b border-[#e7e2d7] dark:border-[#2a2a2a]"
        >
          <!-- File header row -->
          <button
            class="w-full flex items-center gap-2.5 px-4 py-2.5 text-left transition-colors hover:bg-black/[0.03] dark:hover:bg-white/[0.04]"
            @click="toggleFile(diff.entryId)"
          >
            <!-- Chevron -->
            <svg
              :class="['shrink-0 transition-transform duration-150 text-muted-foreground/60', selectedId === diff.entryId ? 'rotate-90' : '']"
              width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor"
              stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="9 18 15 12 9 6"/>
            </svg>

            <!-- Status indicator -->
            <span
              class="shrink-0 w-2 h-2 rounded-full"
              :class="{
                'bg-amber-400 animate-pulse': diff.status === 'running',
                'bg-green-400': diff.status === 'completed',
                'bg-red-400':   diff.status === 'error',
                'bg-gray-400':  diff.status === 'cancelled',
              }"
            />

            <!-- Filename -->
            <span class="font-medium text-[#1a1a1a] dark:text-[#ececec] truncate min-w-0">{{ diff.filename }}</span>

            <!-- Full path (secondary) -->
            <span class="text-[10px] text-muted-foreground/50 truncate flex-1 min-w-0">{{ diff.path }}</span>

            <!-- Tool badge -->
            <span
              class="shrink-0 text-[10px] px-1.5 py-0.5 rounded font-mono"
              :class="diff.toolBadge === 'edit'
                ? 'bg-blue-100 text-blue-600 dark:bg-blue-950/50 dark:text-blue-400'
                : diff.toolBadge === 'write'
                ? 'bg-purple-100 text-purple-600 dark:bg-purple-950/50 dark:text-purple-400'
                : 'bg-amber-100 text-amber-700 dark:bg-amber-950/50 dark:text-amber-400'"
            >{{ diff.toolBadge === 'edit' ? 'replace' : diff.toolBadge === 'write' ? 'write' : 'shell' }}</span>

            <!-- +/- stats -->
            <span v-if="diff.added > 0"   class="shrink-0 font-mono text-[11px] text-green-600 dark:text-green-400 tabular-nums">+{{ diff.added }}</span>
            <span v-if="diff.removed > 0" class="shrink-0 font-mono text-[11px] text-red-500 dark:text-red-400 tabular-nums">-{{ diff.removed }}</span>

            <!-- Accept / Revert buttons — edit & write tools, coding mode + workspace ready only -->
            <template v-if="props.codingMode && props.workspaceReady && (diff.toolBadge === 'edit' || diff.toolBadge === 'write') && diff.status === 'completed'">
              <!-- Already resolved -->
              <span
                v-if="resolvedState[diff.entryId] === 'accepted'"
                class="shrink-0 text-[11px] text-green-600 dark:text-green-400 font-medium"
              >已保留</span>
              <span
                v-else-if="resolvedState[diff.entryId] === 'reverted'"
                class="shrink-0 text-[11px] text-muted-foreground font-medium"
              >已撤回</span>

              <!-- Pending action buttons -->
              <template v-else>
                <!-- Accept -->
                <button
                  class="shrink-0 text-[11px] px-2 py-0.5 rounded border transition-colors border-[#e7e2d7] dark:border-[#333] text-muted-foreground hover:border-green-400 hover:text-green-600 dark:hover:border-green-700 dark:hover:text-green-400"
                  :class="{ 'opacity-50 pointer-events-none': acceptingId === diff.entryId }"
                  @click.stop="handleAccept(diff)"
                >{{ acceptingId === diff.entryId ? '保留中…' : '保留' }}</button>

                <!-- Revert -->
                <button
                  class="shrink-0 text-[11px] px-2 py-0.5 rounded border transition-colors"
                  :class="actionErrors[diff.entryId]
                    ? 'border-red-300 text-red-500 dark:border-red-700 dark:text-red-400'
                    : revertingId === diff.entryId
                    ? 'border-[#e7e2d7] dark:border-[#333] text-muted-foreground opacity-50 pointer-events-none'
                    : 'border-[#e7e2d7] dark:border-[#333] text-muted-foreground hover:border-red-300 hover:text-red-500 dark:hover:border-red-700 dark:hover:text-red-400'"
                  :title="actionErrors[diff.entryId]"
                  @click.stop="handleRevert(diff)"
                >{{ revertingId === diff.entryId ? '撤回中…' : actionErrors[diff.entryId] ? '失败' : '撤回' }}</button>
              </template>
            </template>
          </button>

          <!-- ── Shell command view ────────────────────────────────────── -->
          <div
            v-if="selectedId === diff.entryId && diff.kind === 'shell'"
            class="border-t border-[#e7e2d7] dark:border-[#2a2a2a] bg-[#0d1117] text-xs font-mono"
          >
            <!-- Command block -->
            <div class="px-4 py-2 border-b border-[#21262d]">
              <p class="text-[10px] text-[#8b949e] mb-1 select-none">$ 命令</p>
              <pre class="whitespace-pre-wrap break-all text-[#e6edf3] leading-relaxed max-h-40 overflow-y-auto diff-scrollbar">{{ diff.shellCommand }}</pre>
            </div>
            <!-- Output block -->
            <div class="px-4 py-2">
              <p class="text-[10px] text-[#8b949e] mb-1 select-none">输出</p>
              <pre class="whitespace-pre-wrap break-all text-[#aff5b4] leading-relaxed max-h-48 overflow-y-auto diff-scrollbar">{{ diff.shellResult || '（无输出）' }}</pre>
            </div>
          </div>

          <!-- ── Inline diff view ──────────────────────────────────────── -->
          <div
            v-if="selectedId === diff.entryId && diff.kind === 'diff'"
            class="overflow-x-auto border-t border-[#e7e2d7] dark:border-[#2a2a2a] bg-[#0d1117]"
          >
            <table class="min-w-full text-xs font-mono border-collapse">
              <tbody>
                <template v-for="(row, i) in diff.rows" :key="i">
                  <!-- Fold row -->
                  <tr v-if="row.type === 'fold'" class="bg-[#161b22]">
                    <td colspan="4" class="px-4 py-1 text-center text-[#8b949e] text-[11px] select-none cursor-default">
                      ···  {{ row.count }} 行未改动  ···
                    </td>
                  </tr>

                  <!-- Diff line row -->
                  <tr
                    v-else
                    :class="{
                      'bg-[#0e4020] hover:bg-[#0d4a1e]': row.line.kind === 'add',
                      'bg-[#3d0a0a] hover:bg-[#450c0c]': row.line.kind === 'del',
                      'hover:bg-[#161b22]':               row.line.kind === 'eq',
                    }"
                  >
                    <!-- Old line number -->
                    <td class="select-none text-right pr-3 pl-2 py-[1px] text-[#484f58] w-10 border-r border-[#21262d] tabular-nums">
                      {{ row.line.oldLine ?? '' }}
                    </td>
                    <!-- New line number -->
                    <td class="select-none text-right pr-3 pl-1 py-[1px] text-[#484f58] w-10 border-r border-[#21262d] tabular-nums">
                      {{ row.line.newLine ?? '' }}
                    </td>
                    <!-- Diff marker -->
                    <td
                      class="select-none px-2 py-[1px] w-5 text-center font-bold"
                      :class="{
                        'text-green-400': row.line.kind === 'add',
                        'text-red-400':   row.line.kind === 'del',
                        'text-[#484f58]': row.line.kind === 'eq',
                      }"
                    >{{ row.line.kind === 'add' ? '+' : row.line.kind === 'del' ? '−' : ' ' }}</td>
                    <!-- Code content -->
                    <td
                      class="py-[1px] pl-1 pr-4 whitespace-pre select-text"
                      :class="{
                        'text-[#aff5b4]': row.line.kind === 'add',
                        'text-[#ffa198]': row.line.kind === 'del',
                        'text-[#e6edf3]': row.line.kind === 'eq',
                      }"
                    >{{ row.line.text }}</td>
                  </tr>
                </template>
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.diff-scrollbar::-webkit-scrollbar { width: 5px; }
.diff-scrollbar::-webkit-scrollbar-track { background: transparent; }
.diff-scrollbar::-webkit-scrollbar-thumb { background-color: #333; border-radius: 10px; }
</style>
