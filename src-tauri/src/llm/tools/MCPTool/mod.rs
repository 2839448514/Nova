use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "mcp_tool".into(),
        description: "Call a tool exposed by a configured MCP server.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "server": { "type": "string" },
                "tool": { "type": "string" },
                "arguments": { "type": "object" }
            },
            "required": ["server", "tool"]
        }),
    }
}

pub fn execute(input: Value) -> String {
    let server = input.get("server").and_then(|v| v.as_str()).unwrap_or("");
    let tool = input.get("tool").and_then(|v| v.as_str()).unwrap_or("");
    json!({
        "ok": false,
        "server": server,
        "tool": tool,
        "message": "mcp_tool requires AppHandle-aware execution and should be routed via execute_tool_with_app."
    })
    .to_string()
}
