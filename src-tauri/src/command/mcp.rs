use serde_json::Value;
use tauri::AppHandle;

// 复用 services 层的 MCP 对外类型。
pub use crate::llm::services::mcp::{
    McpResourceInfo,
    McpServerConfig,
    McpServerEntry,
    McpServerStatus,
    McpToolInfo,
};

#[tauri::command]
pub async fn add_mcp_server(app: AppHandle, name: String, config: McpServerConfig) -> Result<(), String> {
    // 将 tauri 命令直接转发到 MCP service。
    crate::llm::services::mcp::add_mcp_server(app, name, config).await
}

#[tauri::command]
pub async fn get_mcp_server(app: AppHandle, name: String) -> Result<McpServerEntry, String> {
    // 读取指定 MCP server 的完整配置。
    crate::llm::services::mcp::get_mcp_server(app, name).await
}

#[tauri::command]
pub async fn update_mcp_server(
    app: AppHandle,
    old_name: String,
    new_name: String,
    config: McpServerConfig,
) -> Result<(), String> {
    // 更新指定 MCP server，必要时允许改名并重连。
    crate::llm::services::mcp::update_mcp_server(app, old_name, new_name, config).await
}

#[tauri::command]
pub async fn remove_mcp_server(app: AppHandle, name: String) -> Result<(), String> {
    // 删除指定 MCP server 配置与运行态。
    crate::llm::services::mcp::remove_mcp_server(app, name).await
}

#[tauri::command]
pub async fn get_mcp_server_statuses(app: AppHandle) -> Result<Vec<McpServerStatus>, String> {
    // 获取所有 MCP server 当前状态。
    crate::llm::services::mcp::get_mcp_server_statuses(app).await
}

#[tauri::command]
pub async fn reload_all_mcp_servers(app: AppHandle) -> Result<(), String> {
    // 触发所有 MCP server 重载。
    crate::llm::services::mcp::reload_all_mcp_servers(app).await
}

#[tauri::command]
pub async fn set_mcp_server_enabled(app: AppHandle, name: String, enabled: bool) -> Result<(), String> {
    // 启用或禁用指定 MCP server。
    crate::llm::services::mcp::set_mcp_server_enabled(app, name, enabled).await
}

#[tauri::command]
pub async fn list_mcp_tools(app: AppHandle, server_name: String) -> Result<Vec<McpToolInfo>, String> {
    // 列出指定 server 的工具。
    crate::llm::services::mcp::list_mcp_tools(app, server_name).await
}

#[tauri::command]
pub async fn list_mcp_resources(app: AppHandle, server_name: String) -> Result<Vec<McpResourceInfo>, String> {
    // 列出指定 server 的资源。
    crate::llm::services::mcp::list_mcp_resources(app, server_name).await
}

#[tauri::command]
pub async fn read_mcp_resource(app: AppHandle, server_name: String, uri: String) -> Result<Value, String> {
    // 读取指定资源 URI 的内容。
    crate::llm::services::mcp::read_mcp_resource(app, server_name, uri).await
}

#[tauri::command]
pub async fn call_mcp_tool(
    app: AppHandle,
    server_name: String,
    tool_name: String,
    arguments: Value,
) -> Result<Value, String> {
    // 调用指定 MCP 工具并返回原始结果 JSON。
    crate::llm::services::mcp::call_mcp_tool(app, server_name, tool_name, arguments).await
}

pub async fn warmup_runtime(app: AppHandle) -> Result<(), String> {
    // 应用启动时预热 MCP runtime（非 tauri command）。
    crate::llm::services::mcp::warmup_runtime(app).await
}
