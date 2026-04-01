use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "ask_user_question".into(),
        description: "Ask the user a clarifying question when required information is missing before continuing.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The exact question to ask the user"
                },
                "context": {
                    "type": "string",
                    "description": "Short reason why this question is needed"
                },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional suggested answer choices"
                },
                "allow_freeform": {
                    "type": "boolean",
                    "description": "Whether user can answer outside provided options"
                }
            },
            "required": ["question"]
        }),
    }
}

pub fn execute(input: Value) -> String {
    let question = match input.get("question").and_then(|v| v.as_str()) {
        Some(q) if !q.trim().is_empty() => q.trim().to_string(),
        _ => return "Error: Missing or empty 'question' argument".into(),
    };

    let context = input
        .get("context")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let options: Vec<String> = input
        .get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let allow_freeform = input
        .get("allow_freeform")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    json!({
        "type": "needs_user_input",
        "question": question,
        "context": context,
        "options": options,
        "allow_freeform": allow_freeform,
        "instruction": "Stop tool execution and ask the user this question before continuing."
    })
    .to_string()
}
