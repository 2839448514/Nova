use std::path::PathBuf;

use tauri::AppHandle;

// 系统提示文件名（相对工程目录 src/prompt）
const SYSTEM_PROMPT_FILE_NAME: &str = "system_prompt.md";

// 计划模式附加内容：当 plan_mode=true 时合并到系统提示中。
// 该段与正常模式分离方便 semantics 清晰、可测。
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

fn main_prompt_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("prompt")
        .join(SYSTEM_PROMPT_FILE_NAME)
}

pub fn load_system_prompt(_app: &AppHandle, plan_mode: bool) -> Result<String, String> {
    let path = main_prompt_path();
    let prompt = read_non_empty_file(&path).ok_or_else(|| {
        format!(
            "System prompt file is missing or empty: {}. Refusing to use fallback.",
            path.display()
        )
    })?;

    if plan_mode {
        Ok(format!("{}{}", prompt, PLAN_MODE_SECTION))
    } else {
        Ok(prompt)
    }
}
