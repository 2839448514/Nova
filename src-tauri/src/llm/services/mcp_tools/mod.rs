use tauri::AppHandle;

use crate::llm::types::Tool;

// 解析 mcp 形式工具名："mcp/<server>/<tool>" -> (server, tool)
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

// 查询已启用并已连接的 MCP 服务器，收集每个 server 的 tool 列表并转成本地 Tool 格式。
// 这将使模型可调用 "mcp/{server}/{tool}"。
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
