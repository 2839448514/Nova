use serde_json::Value;
use tauri::AppHandle;

pub use crate::llm::services::mcp::{McpResourceInfo, McpServerConfig, McpServerStatus, McpToolInfo};

#[tauri::command]
pub async fn add_mcp_server(app: AppHandle, name: String, config: McpServerConfig) -> Result<(), String> {
    crate::llm::services::mcp::add_mcp_server(app, name, config).await
}

#[tauri::command]
pub async fn remove_mcp_server(app: AppHandle, name: String) -> Result<(), String> {
    crate::llm::services::mcp::remove_mcp_server(app, name).await
}

#[tauri::command]
pub async fn get_mcp_server_statuses(app: AppHandle) -> Result<Vec<McpServerStatus>, String> {
    crate::llm::services::mcp::get_mcp_server_statuses(app).await
}

#[tauri::command]
pub async fn reload_all_mcp_servers(app: AppHandle) -> Result<(), String> {
    crate::llm::services::mcp::reload_all_mcp_servers(app).await
}

#[tauri::command]
pub async fn set_mcp_server_enabled(app: AppHandle, name: String, enabled: bool) -> Result<(), String> {
    crate::llm::services::mcp::set_mcp_server_enabled(app, name, enabled).await
}

#[tauri::command]
pub async fn list_mcp_tools(app: AppHandle, server_name: String) -> Result<Vec<McpToolInfo>, String> {
    crate::llm::services::mcp::list_mcp_tools(app, server_name).await
}

#[tauri::command]
pub async fn list_mcp_resources(app: AppHandle, server_name: String) -> Result<Vec<McpResourceInfo>, String> {
    crate::llm::services::mcp::list_mcp_resources(app, server_name).await
}

#[tauri::command]
pub async fn read_mcp_resource(app: AppHandle, server_name: String, uri: String) -> Result<Value, String> {
    crate::llm::services::mcp::read_mcp_resource(app, server_name, uri).await
}

#[tauri::command]
pub async fn call_mcp_tool(
    app: AppHandle,
    server_name: String,
    tool_name: String,
    arguments: Value,
) -> Result<Value, String> {
    crate::llm::services::mcp::call_mcp_tool(app, server_name, tool_name, arguments).await
}

pub async fn warmup_runtime(app: AppHandle) -> Result<(), String> {
    crate::llm::services::mcp::warmup_runtime(app).await
}
