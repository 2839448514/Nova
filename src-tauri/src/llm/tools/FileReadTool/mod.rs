use crate::llm::tools::{sync_tool, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};
use std::fs;

// 注册 read_file，声明它是只读同步工具，不参与权限审批。
pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, true, None)
}

// 返回暴露给模型的工具元数据，告诉模型需要传入 path 读取文件。
pub fn tool() -> Tool {
    Tool {
        name: "read_file".into(),
        description: "Read the content of a file from the host machine.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute path to the file" }
            },
            "required": ["path"]
        }),
    }
}

// 从输入里读取 path，并把目标文件完整内容作为字符串返回。
pub fn execute(input: Value) -> String {
    if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
        fs::read_to_string(path).unwrap_or_else(|e| format!("Error reading file: {}", e))
    } else {
        "Error: Missing 'path' argument".into()
    }
}
