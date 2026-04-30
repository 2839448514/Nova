use crate::llm::tools::{sync_tool, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};
use std::process::Command;

// 返回 grep_search 的注册信息。
// 它只做内容检索，不修改文件，所以可以走只读并发队列。
pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, true, None)
}

// 返回模型可见的 grep_search 元数据。
// `pattern` 是要搜的文本，`path` 是递归搜索的目录。
pub fn tool() -> Tool {
    Tool {
        name: "grep_search".into(),
        description: "Search for a pattern in files within a directory.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "The pattern to search for" },
                "path": { "type": "string", "description": "Directory to search in" }
            },
            "required": ["pattern", "path"]
        }),
    }
}

// 在 `path` 目录里递归搜索 `pattern`。
// `pattern` 和 `path` 都直接来自模型输入，结果会尽量保留底层搜索命令的原始输出格式。
pub fn execute(input: Value) -> String {
    if let (Some(pattern), Some(path)) = (
        input.get("pattern").and_then(|v| v.as_str()),
        input.get("path").and_then(|v| v.as_str())
    ) {
        #[cfg(target_os = "windows")]
        let out = Command::new("powershell").args(["-Command", &format!("Select-String -Path '{}' -Pattern '{}' -Recurse", path, pattern)]).output();

        #[cfg(not(target_os = "windows"))]
        let out = Command::new("grep").args(["-rni", pattern, path]).output();

        match out {
            Ok(output) => {
                // result: 底层搜索命令的标准输出，里面通常包含 文件:行号:内容 这样的命中列表。
                let result = String::from_utf8_lossy(&output.stdout).to_string();
                if result.is_empty() {
                    "No matches found".into()
                } else {
                    if result.len() > 10000 {
                        format!("{}...\n(Result truncated)", &result[..10000])
                    } else {
                        result
                    }
                }
            }
            Err(e) => format!("Failed to search: {}", e),
        }
    } else {
        "Error: Missing 'pattern' or 'path' argument".into()
    }
}
