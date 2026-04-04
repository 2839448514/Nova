use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "rag_tool".into(),
        description: "Search and read documents from Nova local RAG database.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["stats", "search", "read"]
                },
                "query": { "type": "string" },
                "documentId": { "type": "string" },
                "id": { "type": "string" },
                "limit": { "type": "integer" }
            },
            "required": ["action"]
        }),
    }
}

pub fn execute(_input: Value) -> String {
    json!({
        "ok": false,
        "message": "rag_tool requires AppHandle-aware execution and should be routed via execute_tool_with_app."
    })
    .to_string()
}
