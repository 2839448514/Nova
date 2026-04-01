use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "lsp_tool".into(),
        description: "LSP semantic operations scaffold (find symbols/usages).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": { "type": "string" },
                "symbol": { "type": "string" },
                "file": { "type": "string" }
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
        "message": "lsp_tool scaffold exists; Rust-side LSP integration is pending."
    })
    .to_string()
}
