use crate::llm::tools::shared::task_store;
use crate::llm::tools::{sync_tool, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};

// 注册 task_update，声明它是写类同步工具，用于更新已有任务。
pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, false, None)
}

// 返回暴露给模型的工具元数据，要求提供 id，其他字段按需更新。
pub fn tool() -> Tool {
    Tool {
        name: "task_update".into(),
        description: "Update a task item by id.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer" },
                "title": { "type": "string" },
                "status": { "type": "string", "enum": ["not-started", "in-progress", "completed"] },
                "notes": { "type": ["string", "null"] }
            },
            "required": ["id"]
        }),
    }
}

// 读取要更新的 id，以及可选的 title/status/notes，再写回内存任务表。
pub fn execute(input: Value) -> String {
    let id = match input.get("id").and_then(|v| v.as_u64()) {
        Some(v) => v,
        None => return "Error: Missing 'id' argument".into(),
    };

    let title = input
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // status: 新状态；未提供时保持原值不变。
    let status = input
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // notes: Some(None) 表示显式清空备注；None 表示本次不改备注字段。
    let notes = if input.get("notes").is_some() {
        Some(input.get("notes").and_then(|v| v.as_str()).map(|s| s.to_string()))
    } else {
        None
    };

    match task_store::update(id, title, status, notes) {
        Some(task) => json!({ "ok": true, "task": task }).to_string(),
        None => format!("Error: Task id {} not found", id),
    }
}
