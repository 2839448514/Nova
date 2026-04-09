# Copilot Project Instructions

## Backend modularization rules

- Keep hook runtime code in dedicated modules under src-tauri/src/llm/services/hooks/.
- Do not add new hook logic directly into src-tauri/src/llm/services/tools/mod.rs.
- services/tools/mod.rs is a compatibility shim only; new code must target services/hooks.
- Split hook features by concern:
  - lifecycle hooks in lifecycle.rs
  - tool pre/post hooks in tool_flow.rs
  - stop hooks in stop.rs
  - shared parsing/config helpers in config.rs and shared.rs
- Keep query flow orchestration in src-tauri/src/llm/query.rs and tool execution orchestration in src-tauri/src/llm/tools/mod.rs; avoid embedding detailed hook parsing there.
- Prefer file-backed settings (settings.json hook_env) over process env state for hook behavior.
- When adding a new hook key, update both:
  - src/components/hooks/HooksConfigScreen.vue
  - src-tauri/src/command/settings.rs validation/parsing

## Refactor quality bar

- New backend behavior should be introduced in small modules, not monolithic files.
- Keep public function signatures stable when possible; if changed, update all call sites in the same commit.
- Run diagnostics after refactor and resolve compile or type errors before finishing.
Get-ChildItem -Path src/, src-tauri/src/ -Recurse -Include *.rs,*.vue,*.ts | ForEach-Object { Get-Content $_.FullName } | Out-File -Encoding utf8 all_code.txt