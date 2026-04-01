use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "list_mcp_resources".into(),
        description: "List MCP resources (scaffold placeholder).".into(),
        input_schema: json!({ "type": "object", "properties": {} }),
    }
}

pub fn execute(_input: Value) -> String {
    json!({
        "ok": false,
        "message": "list_mcp_resources scaffold exists; runtime bridge to command::mcp is pending."
    })
    .to_string()
}
