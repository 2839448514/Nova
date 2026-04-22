use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::llm::types::AgentMode;

// 系统提示文件名（相对工程目录 src/prompt）
const SYSTEM_PROMPT_FILE_NAME: &str = "system_prompt.md";

// 计划模式附加内容：当 agent_mode=plan 时合并到系统提示中。
// 该段与正常模式分离方便 semantics 清晰、可测。
const PLAN_MODE_SECTION: &str = r#"

## Plan Mode
- You are currently in plan mode.
- In this mode, prioritize understanding the problem, exploring the codebase, identifying constraints, and proposing a concrete implementation strategy.
- Do not edit files or run implementation tools before explicit user approval.
- When your plan is ready, call `plan_for_approval` with summary, concrete steps, and key risks so the user can review and decide.
- If the user asks for adjustments, revise the plan and call `plan_for_approval` again.
- Only after the user explicitly approves implementation should you use `exit_plan_mode` and proceed to implementation.
- Use `ask_user_question` for extra clarifications only when needed to unblock planning decisions.
"#;

// 自动迭代模式附加内容：鼓励在单轮中自主推进，只有被真实阻塞时再请求用户输入。
const AUTO_MODE_SECTION: &str = r#"

## Auto Iteration Mode
- You are currently in auto iteration mode.
- Drive the task forward proactively with focused tool usage and iterative verification.
- Keep iterating until the task is meaningfully complete, then present a concise outcome.
- Ask for user input only when blocked by missing requirements, permissions, or irreversible decisions.
"#;

// 代码编写模式附加内容：AI 聚焦文件改动，用户通过 Code Diff 面板审阅并决定保留/撤回。
const CODING_MODE_SECTION: &str = r#"

## Coding Mode
- You are currently in coding mode.
- Focus on precise, minimal file edits. Use `replace_string_in_file` for targeted changes and `write_file` only for new files.
- The user has a Code Diff panel open to review every file modification. They will manually accept or revert each change.
- Do not ask for confirmation before writing; just make the change and explain briefly what you did and why.
- Prefer atomic, reviewable edits: one logical change per tool call where possible.
"#;

const GLOBAL_MEMORY_SECTION: &str = r#"

## Global Memory
- You may store stable cross-session memory by calling `remember_global_memory`.
- Use it for durable user preferences, long-lived project rules, or persistent facts that improve future turns.
- Do not store secrets, credentials, private tokens, or one-off ephemeral details.
- Keep memory entries concise, specific, and reusable.
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

pub fn workspace_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|d| d.join("workspace"))
        .unwrap_or_else(|_| PathBuf::from("workspace"))
}

pub fn load_system_prompt(app: &AppHandle, agent_mode: AgentMode) -> Result<String, String> {
    // 计算系统提示文件路径。
    let path = main_prompt_path();
    // 读取并校验主提示词文件，失败时拒绝 fallback。
    let prompt = read_non_empty_file(&path).ok_or_else(|| {
        format!(
            "System prompt file is missing or empty: {}. Refusing to use fallback.",
            path.display()
        )
    })?;

    // 将 workspace 路径注入提示词。
    // Coding 模式下优先使用用户选定的工作区目录。
    let workspace = if agent_mode == AgentMode::Coding {
        // 尝试从设置中读取 coding_workspace。
        let settings_path = app
            .path()
            .app_data_dir()
            .map(|d| d.join("settings.json"))
            .ok();
        let coding_ws = settings_path
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v.get("codingWorkspace").and_then(|w| w.as_str()).map(|s| s.to_string()));
        coding_ws.unwrap_or_else(|| workspace_dir(app).display().to_string())
    } else {
        workspace_dir(app).display().to_string()
    };

    let prompt = prompt.replace("{{NOVA_WORKSPACE}}", &workspace);

    let prompt_with_memory = format!("{}{}", prompt, GLOBAL_MEMORY_SECTION);

    // 按执行模式拼接附加段。
    match agent_mode {
        AgentMode::Plan   => Ok(format!("{}{}", prompt_with_memory, PLAN_MODE_SECTION)),
        AgentMode::Auto   => Ok(format!("{}{}", prompt_with_memory, AUTO_MODE_SECTION)),
        AgentMode::Coding => Ok(format!("{}{}", prompt_with_memory, CODING_MODE_SECTION)),
        AgentMode::Agent  => Ok(prompt_with_memory),
    }
}
