# Nova

Nova is a local coding assistant desktop app built with Tauri, Vue 3, TypeScript, and Rust.

It is designed for an in-editor workflow where the model can:

- read and edit files in the current workspace
- call local tools from the Rust side (Bash, Git, File System, MCP)
- stream responses into the chat UI
- pause and ask the user for clarification through an interactive option dialog when key information is missing
- connect to multiple LLM providers (Anthropic, OpenAI, Ollama, etc.) with custom model support

## Key Features

- **Multi-Provider LLM Support:** Seamlessly switch between Anthropic, OpenAI-compatible APIs, and local Ollama interfaces.
- **Customizable Models:** No hardcoded models. Add any custom model name (e.g., `gpt-4o`, `claude-3-5-sonnet`, `qwen2.5:7b`) directly from the settings or chat interface, and they will be permanently saved and made available in a quick-select dropdown.
- **Human-in-the-Loop:** Supports a clarification flow through the `ask_user_question` tool. When the model lacks required information, it pauses and prompts the user via a bottom-pinned UI dialog (with options and freeform input) instead of guessing.
- **MCP Integration:** Extensible tool system supporting the Model Context Protocol (MCP) alongside powerful built-in system tools like Bash, Grep, File Edit, and more.

## Current Interaction Model

Nova supports a human-in-the-loop clarification flow through the `ask_user_question` tool.

When the model lacks required information, it should stop and return a `needs_user_input` payload instead of guessing. The frontend then shows a bottom-pinned dialog with:

- a question title
- optional context
- selectable options
- an optional freeform input
- skip support

After the user selects an option, types an answer, or skips, Nova resumes the conversation with that clarification injected back into the chat context.

## Project Structure

- `src/`: Vue frontend
- `src/components/chat/`: chat UI, message rendering, ask-user dialog
- `src/components/layout/`: layout, sidebar, input area, settings
- `src-tauri/src/`: Rust commands, LLM services, tools, prompt loading
- `src-tauri/src/prompt/system_prompt.md`: runtime system prompt actually loaded by the app
- `src-tauri/prompts/system_prompt.md`: prompt reference copy for editing and versioning

## Development

Recommended environment:

- Node.js
- Rust
- Tauri prerequisites for your OS
- VS Code + Vue Official + rust-analyzer + Tauri extension

Install dependencies:

```bash
npm install
```

Start the frontend:

```bash
npm run dev
```

Start the Tauri app:

```bash
npm run tauri
```

Build the frontend:

```bash
npm run build
```

## Notes

- PowerShell commands in this repo should use PowerShell 7 (`pwsh.exe`).
- The build may currently surface unrelated TypeScript unused-variable warnings in some settings and welcome-screen files.
