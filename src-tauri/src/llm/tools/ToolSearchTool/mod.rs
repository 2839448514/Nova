use crate::llm::services::mcp_tools;
use crate::llm::tools::{app_tool, get_available_tools, AppExecuteFuture, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};
use std::collections::HashSet;
use tauri::AppHandle;

pub(crate) fn registration() -> ToolRegistration {
    app_tool(tool, execute, execute_with_app_boxed, true, None)
}

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
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => return "Error: Missing 'query' argument".into(),
    };

    search_tools(query, get_available_tools())
}

pub async fn execute_with_app(app: &AppHandle, input: Value) -> String {
    let query = match input.get("query").and_then(|v| v.as_str()) {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => return "Error: Missing 'query' argument".into(),
    };

    let mut tools = get_available_tools();
    tools.extend(mcp_tools::collect_mcp_tools(app).await);
    search_tools(query, tools)
}

fn execute_with_app_boxed(
    app: AppHandle,
    _conversation_id: Option<String>,
    input: Value,
) -> AppExecuteFuture {
    Box::pin(async move { execute_with_app(&app, input).await })
}

fn search_tools(query: &str, tools: Vec<Tool>) -> String {
    let query = query.trim();
    let match_all = query == "*";
    let normalized_query = query.to_ascii_lowercase();
    let mut seen = HashSet::new();
    let mut matched: Vec<String> = tools
        .into_iter()
        .filter(|tool| {
            if match_all {
                return true;
            }
            let searchable = format!(
                "{} {}",
                tool.name.to_ascii_lowercase(),
                tool.description.to_ascii_lowercase()
            );
            searchable.contains(&normalized_query)
        })
        .filter(|tool| seen.insert(tool.name.clone()))
        .map(|tool| format!("{}: {}", tool.name, tool.description))
        .collect();
    matched.sort();

    if matched.is_empty() {
        "No matching tools found".into()
    } else {
        matched.join("\n")
    }
}
