use crate::llm::tools::shared::task_store;
use crate::llm::tools::{sync_tool, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};

// 注册 TaskStop，声明它是写类同步工具，用于兼容 Claude 风格的停止任务调用。
pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, false, None)
}

// 返回暴露给模型的工具元数据，允许通过 task_id / shell_id / id 指定要停止的任务。
pub fn tool() -> Tool {
    Tool {
        name: "TaskStop".into(),
        description: "Stop a running task by id (Claude-compatible name).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "task_id": { "type": ["string", "integer"] },
                "shell_id": { "type": ["string", "integer"] },
                "id": { "type": ["string", "integer"] }
            }
        }),
    }
}

// 依次尝试 task_id、shell_id、id，把不同命名的输入统一解析成内部 task id。
fn parse_task_id(input: &Value) -> Option<u64> {
    for key in ["task_id", "shell_id", "id"] {
        if let Some(v) = input.get(key) {
            if let Some(id) = v.as_u64() {
                return Some(id);
            }
            if let Some(s) = v.as_str() {
                if let Ok(id) = s.trim().parse::<u64>() {
                    return Some(id);
                }
            }
        }
    }
    None
}

// 把目标任务状态更新为 stopped，并返回 Claude 风格的停止结果。
pub fn execute(input: Value) -> String {
    let Some(task_id) = parse_task_id(&input) else {
        return "Error: Missing task id (task_id/shell_id/id)".into();
    };

    match task_store::update(task_id, None, Some("stopped".into()), None) {
        Some(task) => json!({
            "ok": true,
            "message": format!("Successfully stopped task: {}", task.id),
            "task_id": task.id.to_string(),
            "task_type": "todo",
            "command": task.title
        })
        .to_string(),
        None => format!("Error: Task id {} not found", task_id),
    }
}
