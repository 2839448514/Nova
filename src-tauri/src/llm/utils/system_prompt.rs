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
    // 读取文件文本，读取失败返回 None。
    let text = std::fs::read_to_string(path).ok()?;
    // 去掉首尾空白后判断是否为空。
    let trimmed = text.trim();
    // 空文件或全空白文件视为无效。
    if trimmed.is_empty() {
        return None;
    }
    // 返回裁剪后的新字符串。
    Some(trimmed.to_string())
}

fn main_prompt_path() -> PathBuf {
    // 从编译时清单目录开始构造绝对路径。
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        // 进入 src 目录。
        .join("src")
        // 进入 prompt 子目录。
        .join("prompt")
        // 拼接系统提示文件名。
        .join(SYSTEM_PROMPT_FILE_NAME)
}

pub fn load_system_prompt(_app: &AppHandle, plan_mode: bool) -> Result<String, String> {
    // 计算系统提示文件路径。
    let path = main_prompt_path();
    // 读取并校验主提示词文件，失败时拒绝 fallback。
    let prompt = read_non_empty_file(&path).ok_or_else(|| {
        format!(
            "System prompt file is missing or empty: {}. Refusing to use fallback.",
            path.display()
        )
    })?;

    // 计划模式下在主提示词后拼接计划附加段。
    if plan_mode {
        Ok(format!("{}{}", prompt, PLAN_MODE_SECTION))
    } else {
        // 普通模式直接返回主提示词。
        Ok(prompt)
    }
}
