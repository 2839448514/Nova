use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "read_mcp_resource".into(),
        description: "Read a resource exposed by a configured MCP server.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "server": { "type": "string" },
                "resource": { "type": "string" },
                "uri": { "type": "string" }
            },
            "required": ["server", "resource"]
        }),
    }
}

pub fn execute(input: Value) -> String {
    let server = input.get("server").and_then(|v| v.as_str()).unwrap_or("");
    let resource = input
        .get("resource")
        .or_else(|| input.get("uri"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    json!({
        "ok": false,
        "server": server,
        "resource": resource,
        "message": "read_mcp_resource requires AppHandle-aware execution and should be routed via execute_tool_with_app."
    })
    .to_string()
}
