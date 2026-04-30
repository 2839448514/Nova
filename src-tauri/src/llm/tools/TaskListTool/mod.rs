use crate::llm::tools::shared::task_store;
use crate::llm::tools::{sync_tool, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};

// 注册 task_list，声明它是只读同步工具，用于列出当前会话全部任务。
pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, true, None)
}

// 返回暴露给模型的工具元数据；这个工具不需要额外输入字段。
pub fn tool() -> Tool {
    Tool {
        name: "task_list".into(),
        description: "List all session tasks.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

// 直接读取当前内存任务表，并把所有任务打包成 JSON 数组返回。
pub fn execute(_input: Value) -> String {
    let tasks = task_store::list();
    json!({ "ok": true, "tasks": tasks }).to_string()
}
