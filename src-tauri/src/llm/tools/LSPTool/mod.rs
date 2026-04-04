use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "lsp_tool".into(),
        description: "Run semantic code-navigation operations via MCP-backed LSP servers (list servers/tools, call, find symbol/references/definition/implementation, diagnostics).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "list_servers",
                        "list_server_tools",
                        "call",
                        "find_symbol",
                        "find_references",
                        "find_definition",
                        "find_implementation",
                        "diagnostics"
                    ]
                },
                "server": { "type": "string" },
                "tool": { "type": "string" },
                "symbol": { "type": "string" },
                "file": { "type": "string" },
                "lineContent": { "type": "string" },
                "arguments": { "type": "object" }
            },
            "required": ["action"]
        }),
    }
}

pub fn execute(input: Value) -> String {
    let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("unknown");
    json!({
        "ok": false,
        "action": action,
        "message": "lsp_tool requires AppHandle-aware execution and should be routed via execute_tool_with_app."
    })
    .to_string()
}
