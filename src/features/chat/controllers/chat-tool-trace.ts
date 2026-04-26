import type { ToolExecutionEntry } from "../../../lib/chat-types";

export function latestRunningToolExecutionIdByName(
  entries: ToolExecutionEntry[],
  toolName: string,
): string | null {
  for (let i = entries.length - 1; i >= 0; i -= 1) {
    const entry = entries[i];
    if (entry.toolName === toolName && entry.status === "running") {
      return entry.id;
    }
  }
  return null;
}

export function startToolExecutionTrace(
  entries: ToolExecutionEntry[],
  toolId: string,
  toolName: string,
) {
  const idx = entries.findIndex((entry) => entry.id === toolId);
  if (idx >= 0) {
    entries[idx] = {
      ...entries[idx],
      toolName,
      status: "running",
      startedAt: Date.now(),
      finishedAt: undefined,
    };
    return;
  }

  entries.push({
    id: toolId,
    toolName,
    input: "",
    result: "",
    status: "running",
    startedAt: Date.now(),
    finishedAt: undefined,
  });
}

export function appendToolExecutionInput(
  entries: ToolExecutionEntry[],
  toolId: string,
  inputDelta: string,
) {
  const idx = entries.findIndex((entry) => entry.id === toolId);
  if (idx < 0) {
    return;
  }

  const entry = entries[idx];
  entries[idx] = {
    ...entry,
    input: `${entry.input}${inputDelta}`,
  };
}

export function completeToolExecutionTrace(
  entries: ToolExecutionEntry[],
  toolId: string | null,
  toolName: string,
  result: string,
  status: ToolExecutionEntry["status"],
  inputFallback?: string,
): ToolExecutionEntry | null {
  const resolvedId = toolId || latestRunningToolExecutionIdByName(entries, toolName);
  if (!resolvedId) {
    return null;
  }

  const idx = entries.findIndex((entry) => entry.id === resolvedId);
  if (idx < 0) {
    return null;
  }

  const entry = entries[idx];
  const normalizedFallback = (inputFallback ?? "").trim();
  const resolvedInput = entry.input.trim().length > 0 ? entry.input : normalizedFallback;
  const updatedEntry: ToolExecutionEntry = {
    ...entry,
    toolName,
    input: resolvedInput,
    result,
    status,
    finishedAt: Date.now(),
  };
  entries[idx] = updatedEntry;
  return updatedEntry;
}

export function markRunningToolExecutions(
  entries: ToolExecutionEntry[],
  status: "completed" | "error" | "cancelled",
): ToolExecutionEntry[] {
  const now = Date.now();
  const finalizedEntries: ToolExecutionEntry[] = [];

  for (let i = 0; i < entries.length; i += 1) {
    const entry = entries[i];
    if (entry.status !== "running") {
      continue;
    }

    const updatedEntry: ToolExecutionEntry = {
      ...entry,
      status,
      finishedAt: now,
    };
    entries[i] = updatedEntry;
    finalizedEntries.push(updatedEntry);
  }

  return finalizedEntries;
}
