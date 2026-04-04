use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpRuntimeStatus {
    // 服务未连接或已主动断开。
    Disconnected,
    // 服务连接正常可请求。
    Connected,
    // 服务处于错误态。
    Error,
}

impl McpRuntimeStatus {
    pub fn as_str(self) -> &'static str {
        // 将内部状态映射为外部 API 的字符串状态。
        match self {
            // Disconnected -> disconnected。
            Self::Disconnected => "disconnected",
            // Connected -> connected。
            Self::Connected => "connected",
            // Error -> error。
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpServerConfig {
    // 本地 stdio 子进程方式启动 MCP 服务。
    Stdio {
        // 启动命令，例如 npx。
        command: String,
        // 启动参数列表。
        args: Vec<String>,
        #[serde(default)]
        // 进程环境变量覆盖表。
        env: HashMap<String, String>,
    },
    // 通过 SSE 地址连接 MCP 服务。
    Sse {
        // SSE 服务地址。
        url: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStatus {
    // 服务器名称。
    pub name: String,
    // 状态字符串（connected/disconnected/error）。
    pub status: String,
    // 是否启用。
    pub enabled: bool,
    // 服务器连接类型（stdio/sse）。
    pub r#type: String,
    // 已发现工具数量。
    pub tool_count: usize,
    // 最近一次错误信息。
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpToolInfo {
    // 工具名称。
    pub name: String,
    // 工具描述。
    pub description: Option<String>,
    // 工具输入 schema（JSON）。
    pub input_schema: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceInfo {
    // 资源唯一 URI。
    pub uri: String,
    // 资源展示名称。
    pub name: String,
    // 资源描述。
    pub description: Option<String>,
    // 资源 MIME 类型。
    pub mime_type: Option<String>,
}
