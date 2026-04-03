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
- Skill integration: discover and run reusable skills from local skill definitions.

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
