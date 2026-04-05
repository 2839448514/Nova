# Nova

Nova is a local coding assistant focused on real project execution, controllable automation, and human-in-the-loop workflows.

## Core Features

- Workspace-aware coding: reads, searches, edits, and organizes files in the current project.
- Tool-driven execution: runs shell commands, inspects output, and continues tasks end-to-end.
- Multi-provider model access: switch providers and models with isolated per-provider profiles.
- Custom model management: add and persist custom model names for quick selection.
- MCP connectivity: register MCP servers, inspect tools/resources, and invoke MCP tools during tasks.
- Human clarification flow: pauses and asks focused questions when key details are missing.
- Approval controls: supports allow once, allow for session, and deny decisions for sensitive operations.
- Conversation continuity: includes conversation history, resume context, compact context, and memory updates.
- Streaming responses: sends intermediate progress and final completion states clearly.
- Multi-conversation stream routing: stream events are scoped by conversation so concurrent turns do not mix outputs.
- Skill integration: discover and run reusable skills from local skill definitions.
- Scheduled task automation: create/list/delete cron-based tasks with session or durable persistence.

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
