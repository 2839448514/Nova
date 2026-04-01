use crate::llm::types::Tool;
use serde_json::{json, Value};

pub fn tool() -> Tool {
    Tool {
        name: "tool_search".into(),
        description: "Search available tool names by keyword.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        }),
    }
}

pub fn execute(input: Value) -> String {
    let query = match input.get("query").and_then(|v| v.as_str()) {
        Some(v) if !v.trim().is_empty() => v.trim().to_lowercase(),
        _ => return "Error: Missing 'query' argument".into(),
    };

    let candidates = vec![
        "execute_bash",
        "execute_powershell",
        "read_file",
        "write_file",
        "replace_string_in_file",
        "grep_search",
        "glob_search",
        "web_fetch",
        "web_search",
        "tool_search",
        "task_create",
        "task_list",
        "task_update",
        "todo_write",
        "ask_user_question",
        "config_tool",
    ];

    let matched: Vec<&str> = candidates
        .into_iter()
        .filter(|name| name.contains(&query))
        .collect();

    if matched.is_empty() {
        "No matching tools found".into()
    } else {
        matched.join("\n")
    }
}
