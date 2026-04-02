use std::path::PathBuf;

use tauri::AppHandle;

const SYSTEM_PROMPT_FILE_NAME: &str = "system_prompt.md";
const FALLBACK_SYSTEM_PROMPT: &str = "You are a helpful coding assistant running in a local Tauri desktop app. You will answer questions briefly and write accurate code.";
const PLAN_MODE_SECTION: &str = r#"

## Plan Mode
- You are currently in plan mode.
- In this mode, prioritize understanding the problem, exploring the codebase, identifying constraints, and proposing a concrete implementation strategy.
- Avoid editing files unless the user explicitly asks to skip planning or approves implementation.
- Use `ask_user_question` if a design decision still depends on user intent.
- When the plan is ready and aligned, use `exit_plan_mode` before proceeding with implementation.
"#;

fn read_non_empty_file(path: &PathBuf) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

pub fn load_system_prompt(_app: &AppHandle, plan_mode: bool) -> String {
    if let Ok(cwd) = std::env::current_dir() {
        let path = cwd
            .join("src-tauri")
            .join("src")
            .join("prompt")
            .join(SYSTEM_PROMPT_FILE_NAME);
        if let Some(prompt) = read_non_empty_file(&path) {
            return if plan_mode {
                format!("{}{}", prompt, PLAN_MODE_SECTION)
            } else {
                prompt
            };
        }
    }

    if plan_mode {
        format!("{}{}", FALLBACK_SYSTEM_PROMPT, PLAN_MODE_SECTION)
    } else {
        FALLBACK_SYSTEM_PROMPT.to_string()
    }
}
