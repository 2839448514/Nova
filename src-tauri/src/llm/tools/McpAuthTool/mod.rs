use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "mcp_auth".into(),
        description: "MCP auth management (scaffold placeholder).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": { "type": "string" }
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
        "message": "mcp_auth scaffold exists; runtime bridge is pending."
    })
    .to_string()
}
