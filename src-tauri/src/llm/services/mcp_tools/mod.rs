use tauri::AppHandle;

use crate::llm::types::Tool;

pub fn parse_mcp_tool_name(name: &str) -> Option<(String, String)> {
    let raw = name.strip_prefix("mcp/")?;
    let mut parts = raw.splitn(2, '/');
    let server = parts.next()?.trim();
    let tool = parts.next()?.trim();
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server.to_string(), tool.to_string()))
}

pub async fn collect_mcp_tools(app: &AppHandle) -> Vec<Tool> {
    let mut statuses = match crate::llm::services::mcp::get_mcp_server_statuses(app.clone()).await {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let has_enabled = statuses.iter().any(|s| s.enabled);
    let has_connected = statuses
        .iter()
        .any(|s| s.enabled && s.status == "connected");
    if has_enabled && !has_connected {
        let _ = crate::llm::services::mcp::reload_all_mcp_servers(app.clone()).await;
        statuses = match crate::llm::services::mcp::get_mcp_server_statuses(app.clone()).await {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
    }

    let mut tools_vec = Vec::new();
    for status in statuses
        .into_iter()
        .filter(|s| s.enabled && s.status == "connected")
    {
        let listed = match crate::llm::services::mcp::list_mcp_tools(app.clone(), status.name.clone()).await
        {
            Ok(v) => v,
            Err(_) => continue,
        };

        for t in listed {
            tools_vec.push(Tool {
                name: format!("mcp/{}/{}", status.name, t.name),
                description: t.description.unwrap_or_else(|| {
                    format!("MCP tool '{}' from server '{}'.", t.name, status.name)
                }),
                input_schema: t
                    .input_schema
                    .unwrap_or_else(|| serde_json::json!({ "type": "object", "properties": {} })),
            });
        }
    }

    tools_vec
}
