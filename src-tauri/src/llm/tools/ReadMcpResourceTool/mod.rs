use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "read_mcp_resource".into(),
        description: "Read MCP resource content (scaffold placeholder).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "resource": { "type": "string" }
            },
            "required": ["resource"]
        }),
    }
}

pub fn execute(input: Value) -> String {
    let resource = input.get("resource").and_then(|v| v.as_str()).unwrap_or("");
    json!({
        "ok": false,
        "resource": resource,
        "message": "read_mcp_resource scaffold exists; runtime bridge to command::mcp is pending."
    })
    .to_string()
}
