# Nova

Nova is a local coding assistant focused on real project execution, controllable automation, and human-in-the-loop workflows.

## Core Features

- Workspace-aware coding: reads, searches, edits, and organizes files in the current project.
- Tool-driven execution: runs shell commands, inspects output, and continues tasks end-to-end.
- Modular tool registry: built-in tools self-describe their registration, permissions, app execution, and post-processing so new tools can be mounted with one entry in `src-tauri/src/llm/tools/mod.rs`.
- Multi-provider model access: switch providers and models with isolated per-provider profiles.
- Custom model management: add and persist custom model names for quick selection.
- MCP connectivity: register MCP servers, inspect tools/resources, and invoke MCP tools during tasks through explicit tool names such as `mcp__server__tool`.
- Human clarification flow: pauses and asks focused questions when key details are missing.
- Approval controls: supports allow once, allow for session, and deny decisions for sensitive operations.
- Conversation continuity: includes conversation history, resume context, compact context, and memory updates.
- File-backed cross-session memory: long-term memory is stored under the app data `memory/` directory, retrieved per request, and auto-maintained from stable user preferences and rules.
- Streaming responses: sends intermediate progress and final completion states clearly.
- Multi-conversation stream routing: stream events are scoped by conversation so concurrent turns do not mix outputs.
- Skill integration: discover and run reusable skills from local skill definitions.
- Scheduled task automation: create/list/delete cron-based tasks with session or durable persistence.

## Tool System

- Built-in tools are mounted through the central registry in `src-tauri/src/llm/tools/mod.rs`.
- Each tool module owns its own registration metadata instead of spreading behavior across global `match` branches.
- New tools can be scaffolded from `src-tauri/src/llm/tools/NewToolTemplate/`.
- MCP tools are exposed as explicit names like `mcp__playwright__browser_navigate` instead of a generic dispatcher tool.

## Memory System

- Session memory still supports handover, compact context, and conversation restore.
- Cross-session memory now uses a file-backed memory directory instead of a database table.
- Memory records are grouped by kind: `preference`, `rule`, and `fact`.
- Retrieval is query-aware: Nova injects persistent rules/preferences plus relevant facts for the current request.
- New user preferences and rules can be auto-remembered from chat messages.
- Memory writes perform inline dedupe and conflict cleanup so newer rules replace stale duplicates instead of accumulating parallel variants.

## Conversation Titles

- New conversations are created with an empty title, not a fixed placeholder.
- The first user message becomes the conversation title automatically.
- Existing placeholder-titled conversations are resolved from their first user message when listed in the sidebar.

## Scheduled Tasks

Nova includes a built-in scheduled task system for recurring or one-shot prompt execution.

- Cron format: 5 fields (minute hour day-of-month month day-of-week).
- Storage modes:
	- session: in-memory, cleared on app restart.
	- durable: persisted under app_data_dir in scheduled_tasks.json.
- Task conversation binding:
	- Every newly created task automatically creates and binds a dedicated conversation.
	- Triggered task content is written into the bound conversation.
	- The scheduler also launches an automatic model turn for the bound conversation.
- UI behavior:
	- Schedule screen shows task metadata including bound conversation id.
	- Each task has a "View Task Details" action that opens its bound conversation directly.
	- Task-bound scheduled conversations are hidden from the normal Recents list to reduce noise.
- One-shot reliability:
	- One-shot tasks attempt deletion after trigger.
	- If deletion fails, the per-minute trigger guard is retained so the same task does not retrigger repeatedly in that minute.

## Interaction Flow

1. You give a task in chat.
2. Nova plans and executes using available tools.
3. If required information is missing, Nova asks a targeted question instead of guessing.
4. Nova resumes execution and returns a complete result.

## What Nova Is Optimized For

- Long, multi-step coding tasks that must finish reliably.
- Local project changes that need traceable tool logs.
- Mixed workflows with built-in tools plus MCP-provided capabilities.
- Safe automation where user approval and control are required.

## Development Commands

- Start web UI only: npm run dev
- Start Tauri desktop app: npm run tauri
- Build frontend bundle: npm run build
- Build desktop app: npm run tauri:build
