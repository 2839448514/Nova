use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "mcp_auth".into(),
        description: "Manage MCP connection/auth lifecycle: status, reload, enable/disable server, list tools, and probe tool access.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["status", "reload_all", "enable", "disable", "list_tools", "probe_tool"]
                },
                "server": { "type": "string" },
                "tool": { "type": "string" },
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
        "message": "mcp_auth requires AppHandle-aware execution and should be routed via execute_tool_with_app."
    })
    .to_string()
}
