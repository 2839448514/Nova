# Tools Migration Structure

This folder follows a two-layer migration strategy:

1. Active tools (already wired in `mod.rs`)
- execute_bash.rs
- FileReadTool/mod.rs
- write_file.rs
- FileEditTool/mod.rs
- grep_search.rs
- AskUserQuestionTool/mod.rs
- ConfigTool/mod.rs

2. Planned tool folders (reserved from claude-code migration)
- AgentTool/

## Migration Rules

1. Keep one tool per module with two functions:
- `tool() -> Tool`
- `execute(input: Value) -> String`

2. After creating a tool module, wire it in `mod.rs` in both places:
- `get_available_tools()`
- `execute_tool()`

3. If a reserved folder is not implemented yet, keep `.gitkeep` so the structure is not lost in Git.

4. Prefer non-destructive tools first (`read`, `search`) before enabling destructive tools (`write`, `replace`).

## Current Status

- Wired and usable: 7
- Reserved but not implemented: 1

This keeps the migration structure complete and prevents missing-module drift.
