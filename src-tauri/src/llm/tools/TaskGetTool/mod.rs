use crate::llm::tools::shared::task_store;
use crate::llm::tools::{sync_tool, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};

// 注册 TaskGet，声明它是只读同步工具，用于兼容 Claude 风格的按 id 取任务。
pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, true, None)
}

// 返回暴露给模型的工具元数据，支持 taskId 或 id 两种任务标识字段。
pub fn tool() -> Tool {
    Tool {
        name: "TaskGet".into(),
        description: "Retrieve a task by ID (Claude-compatible name).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "taskId": { "type": "string" },
                "id": { "type": ["integer", "string"] }
            }
        }),
    }
}

// 先尝试从 id 读取整数，再退到 taskId 字符串，统一解析成内部 task id。
fn parse_task_id(input: &Value) -> Option<u64> {
    if let Some(id) = input.get("id") {
        if let Some(v) = id.as_u64() {
            return Some(v);
        }
        if let Some(s) = id.as_str() {
            return s.trim().parse::<u64>().ok();
        }
    }

    input
        .get("taskId")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

// 根据解析出的 task_id 读取任务，并把任务对象原样放进 JSON 结果里。
pub fn execute(input: Value) -> String {
    let Some(task_id) = parse_task_id(&input) else {
        return "Error: Missing 'taskId' or numeric 'id'".into();
    };

    let task = task_store::get(task_id);
    json!({ "ok": true, "task": task }).to_string()
}
