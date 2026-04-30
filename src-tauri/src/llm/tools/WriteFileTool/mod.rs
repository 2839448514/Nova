use crate::llm::tools::{sync_tool, ToolPermissionDescriptor, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};
use std::fs;

pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, false, Some(permission))
}

fn permission(input: &Value) -> Option<ToolPermissionDescriptor> {
    // 写文件属于高风险操作，由工具自己声明“写到哪儿”。
    crate::llm::utils::permissions::describe_file_write_permission(
        "write_file",
        "文件写入",
        "path",
        input,
    )
}

pub fn tool() -> Tool {
    Tool {
        name: "write_file".into(),
        description: "Write content to a file on the host machine. This completely overwrites the file.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute path to the file" },
                "content": { "type": "string", "description": "The content to write" }
            },
            "required": ["path", "content"]
        }),
    }
}

pub fn execute(input: Value) -> String {
    if let (Some(path), Some(content)) = (
        input.get("path").and_then(|v| v.as_str()),
        input.get("content").and_then(|v| v.as_str())
    ) {
        // path: 目标文件路径；content: 要完整写入的新文件内容。
        match fs::write(path, content) {
            Ok(_) => "Successfully wrote to file".into(),
            Err(e) => format!("Error writing file: {}", e),
        }
    } else {
        "Error: Missing 'path' or 'content' argument".into()
    }
}
