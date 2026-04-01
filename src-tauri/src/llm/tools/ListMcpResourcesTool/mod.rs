use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "list_mcp_resources".into(),
        description: "List resources exposed by a configured MCP server.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "server": { "type": "string" }
            },
            "required": ["server"]
        }),
    }
}

pub fn execute(input: Value) -> String {
    let server = input.get("server").and_then(|v| v.as_str()).unwrap_or("");
    json!({
        "ok": false,
        "server": server,
        "message": "list_mcp_resources requires AppHandle-aware execution and should be routed via execute_tool_with_app."
    })
    .to_string()
}
