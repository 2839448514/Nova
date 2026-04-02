use crate::llm::tools::shared::task_store;
use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "TaskList".into(),
        description: "List all tasks (Claude-compatible name).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

pub fn execute(_input: Value) -> String {
    let tasks = task_store::list();
    json!({ "ok": true, "tasks": tasks }).to_string()
}
