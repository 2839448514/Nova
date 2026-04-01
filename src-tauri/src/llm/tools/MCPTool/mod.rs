use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "mcp_tool".into(),
        description: "Placeholder MCP bridge tool. Use dedicated MCP command integration path.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": { "type": "string" },
                "payload": { "type": "object" }
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
        "message": "mcp_tool is scaffolded with Claude-style folder structure but not yet wired to AppHandle-based MCP runtime."
    })
    .to_string()
}
