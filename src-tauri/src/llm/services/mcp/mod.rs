use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tauri::{AppHandle, Manager};

mod types;

pub use types::{McpResourceInfo, McpRuntimeStatus, McpServerConfig, McpServerStatus, McpToolInfo};

struct StdioMcpConnection {
    child: Child,
    reader: BufReader<ChildStdout>,
    writer: ChildStdin,
    next_id: u64,
}

impl StdioMcpConnection {
    async fn send_message(&mut self, value: &Value) -> Result<(), String> {
        // value: 要通过 stdio 发送的 JSON 值
        // 将 Value 序列化为字节数组
        let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        // 在 stdio 行协议上以换行符分隔消息
        bytes.push(b'\n');
        // 将字节写入子进程 stdin
        self.writer.write_all(&bytes).await.map_err(|e| e.to_string())?;
        // 确保数据被刷新到子进程
        self.writer.flush().await.map_err(|e| e.to_string())
    }

    async fn read_message(&mut self) -> Result<Value, String> {
        // 持续读取直到解析到合法的 JSON 行或遇到流关闭
        loop {
            let mut line = String::new();
            // read_line 将读取到的字节数返回到 n
            let n = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(|e| e.to_string())?;
            // n == 0 表示流已关闭
            if n == 0 {
                return Err("MCP stdio stream closed".into());
            }

            // trim 并检查是否为空行，跳过空白或日志行
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // 尝试解析该行为 JSON 值；某些服务器在启动时会在 stdout 打印日志，解析失败则跳过
            match serde_json::from_str::<Value>(line) {
                Ok(v) => return Ok(v),
                Err(_) => continue,
            }
        }
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        // 使用自增 id 来关联请求与响应
        let id = self.next_id;
        self.next_id += 1;

        // 构造 JSON-RPC 请求体
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        // 发送请求
        self.send_message(&req).await?;

        // 持续读取直到遇到匹配 id 的响应
        loop {
            let msg = self.read_message().await?;
            // 尝试从响应中解析 id（支持数字或字符串类型）
            let msg_id = msg
                .get("id")
                .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok())));
            // 如果不是我们请求的 id，则跳过该消息
            if msg_id != Some(id) {
                continue;
            }
            // 如果存在 error 字段，返回错误
            if let Some(err) = msg.get("error") {
                return Err(format!("MCP error: {}", err));
            }
            // 返回 result 字段或空对象作为默认
            return Ok(msg.get("result").cloned().unwrap_or_else(|| json!({})));
        }
    }

    async fn send_notification(&mut self, method: &str, params: Value) -> Result<(), String> {
        // 通知无 id 字段，按 JSON-RPC 通知规范构造
        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        self.send_message(&req).await
    }

    async fn list_tools(&mut self) -> Result<Vec<McpToolInfo>, String> {
        // 请求工具列表并从返回值中提取 "tools" 数组
        let result = self.send_request("tools/list", json!({})).await?;
        let tools = result
            .get("tools")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // 将 JSON 值数组映射为 McpToolInfo 结构体向量
        Ok(tools
            .into_iter()
            .filter_map(|t| {
                // name: 必需字段，缺失则跳过该条目
                let name = t.get("name").and_then(|v| v.as_str())?.to_string();
                // description: 可选字段
                let description = t
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                // input_schema: 可能是嵌套 JSON
                let input_schema = t.get("inputSchema").cloned();
                Some(McpToolInfo {
                    name,
                    description,
                    input_schema,
                })
            })
            .collect())
    }

    async fn call_tool(&mut self, tool_name: &str, arguments: Value) -> Result<Value, String> {
        // 直接调用 tools/call 接口并返回结果
        self.send_request(
            "tools/call",
            json!({
                "name": tool_name,
                "arguments": arguments
            }),
        )
        .await
    }

    async fn list_resources(&mut self) -> Result<Vec<McpResourceInfo>, String> {
        // 请求资源列表并解析 "resources" 字段
        let result = self.send_request("resources/list", json!({})).await?;
        let resources = result
            .get("resources")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // 将每个资源 JSON 对象映射为 McpResourceInfo
        Ok(resources
            .into_iter()
            .filter_map(|r| {
                let uri = r.get("uri").and_then(|v| v.as_str())?.to_string();
                let name = r
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&uri)
                    .to_string();
                let description = r
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let mime_type = r
                    .get("mimeType")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                Some(McpResourceInfo {
                    uri,
                    name,
                    description,
                    mime_type,
                })
            })
            .collect())
    }

    async fn read_resource(&mut self, uri: &str) -> Result<Value, String> {
        // 调用 resources/read 并返回原始 JSON 值
        self.send_request("resources/read", json!({ "uri": uri })).await
    }

    async fn shutdown(&mut self) {
        // 终止子进程（忽略错误）
        let _ = self.child.kill().await;
    }
}

enum ServerConnection {
    Stdio(StdioMcpConnection),
}

struct RegisteredServer {
    config: McpServerConfig,
    enabled: bool,
    status: McpRuntimeStatus,
    tool_count: usize,
    error: Option<String>,
    connection: Option<ServerConnection>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PersistedServer {
    name: String,
    enabled: bool,
    config: McpServerConfig,
}

static MCP_RUNTIME: OnceLock<Mutex<HashMap<String, RegisteredServer>>> = OnceLock::new();
static MCP_LOADED: OnceLock<Mutex<bool>> = OnceLock::new();
const MCP_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

fn runtime() -> &'static Mutex<HashMap<String, RegisteredServer>> {
    MCP_RUNTIME.get_or_init(|| Mutex::new(HashMap::new()))
}

fn loaded_flag() -> &'static Mutex<bool> {
    MCP_LOADED.get_or_init(|| Mutex::new(false))
}

fn server_type(config: &McpServerConfig) -> String {
    match config {
        McpServerConfig::Stdio { .. } => "stdio".to_string(),
        McpServerConfig::Sse { .. } => "sse".to_string(),
    }
}

async fn connect_stdio(command: &str, args: &[String], env_map: &HashMap<String, String>) -> Result<StdioMcpConnection, String> {
    // 解析可能的复合命令：如果 args 为空而 command 中含空格，则把 command 拆分为命令名与参数
    let mut parsed_command = command.trim().to_string();
    let mut parsed_args = args.to_vec();
    if parsed_args.is_empty() && parsed_command.contains(' ') {
        // parts: 将 command 按空白拆分为 token 列表
        let mut parts = parsed_command
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        if !parts.is_empty() {
            // 将首个 token 作为命令名，其余作为参数
            parsed_command = parts.remove(0);
            parsed_args = parts;
        }
    }

    // spawn_once: 封装一次性 spawn 调用，便于在后续做 fallback
    let spawn_once = |cmd_name: &str, cmd_args: &[String]| {
        let mut cmd = Command::new(cmd_name);
        // 将参数应用到 Command
        cmd.args(cmd_args);
        // 管道化 stdin/stdout，以便与子进程通信
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        // 将 stderr 重定向到 null，避免污染 stdout
        cmd.stderr(std::process::Stdio::null());

        // 将 env_map 中的环境变量注入子进程
        for (k, v) in env_map {
            cmd.env(k, v);
        }

        // 返回 spawn 调用的结果
        cmd.spawn()
    };

    // 尝试启动子进程，如失败则在 Windows 上尝试通过 cmd /C 回退启动
    let mut child = match spawn_once(&parsed_command, &parsed_args) {
        Ok(child) => child,
        Err(primary_err) => {
            #[cfg(windows)]
            {
                // 如果找不到可执行文件，尝试使用 cmd 回退（支持传入复杂命令行）
                if primary_err.kind() == std::io::ErrorKind::NotFound {
                    let mut shell_args = vec!["/C".to_string(), parsed_command.clone()];
                    shell_args.extend(parsed_args.clone());
                    match spawn_once("cmd", &shell_args) {
                        Ok(child) => child,
                        Err(shell_err) => {
                            return Err(format!(
                                "Failed to spawn MCP server: {} (and cmd fallback failed: {}). Hint: for Playwright MCP use command 'npx' with args '-y @playwright/mcp@latest'.",
                                primary_err,
                                shell_err
                            ));
                        }
                    }
                } else {
                    return Err(format!(
                        "Failed to spawn MCP server: {}. Hint: for Playwright MCP use command 'npx' with args '-y @playwright/mcp@latest'.",
                        primary_err
                    ));
                }
            }
            #[cfg(not(windows))]
            {
                // 非 Windows 平台直接返回错误
                return Err(format!(
                    "Failed to spawn MCP server: {}. Hint: for Playwright MCP use command 'npx' with args '-y @playwright/mcp@latest'.",
                    primary_err
                ));
            }
        }
    };

    // 提取子进程的 stdin/stdout 管道句柄，若不存在则认为启动异常
    let stdin = child.stdin.take().ok_or_else(|| "Missing MCP stdin pipe".to_string())?;
    let stdout = child.stdout.take().ok_or_else(|| "Missing MCP stdout pipe".to_string())?;

    // 构造连接对象，reader 用于按行读取 stdout，writer 写入 stdin
    let mut conn = StdioMcpConnection {
        child,
        reader: BufReader::new(stdout),
        writer: stdin,
        next_id: 1,
    };

    // 发送 initialize 请求并带超时保护，防止首次 npx 安装等长时阻塞
    let init_result = timeout(
        MCP_CONNECT_TIMEOUT,
        conn.send_request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "nova",
                    "version": "0.1.0"
                }
            }),
        ),
    )
    .await;

    match init_result {
        // 初始化成功
        Ok(Ok(_)) => {}
        // MCP 返回错误信息
        Ok(Err(e)) => return Err(e),
        // 超时则关闭连接并返回超时错误
        Err(_) => {
            let _ = conn.shutdown().await;
            return Err("MCP server initialize timeout (30s). First-time npx install may be slow; please retry or pre-run `npx -y @playwright/mcp@latest --help` in terminal.".to_string());
        }
    }

    // 通知 MCP 已初始化（忽略返回值）
    let _ = conn
        .send_notification("notifications/initialized", json!({}))
        .await;

    Ok(conn)
}

fn get_mcp_settings_path(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap().join("mcp_servers.json")
}

fn load_persisted_servers(app: &AppHandle) -> Vec<PersistedServer> {
    let path = get_mcp_settings_path(app);
    if !path.exists() {
        return Vec::new();
    }

    let content = match std::fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    serde_json::from_str::<Vec<PersistedServer>>(&content).unwrap_or_default()
}

fn save_persisted_servers(app: &AppHandle, servers: &[PersistedServer]) -> Result<(), String> {
    let path = get_mcp_settings_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(servers).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}

async fn persist_runtime(app: &AppHandle) -> Result<(), String> {
    let map = runtime().lock().await;
    let mut servers = Vec::with_capacity(map.len());
    for (name, item) in map.iter() {
        servers.push(PersistedServer {
            name: name.clone(),
            enabled: item.enabled,
            config: item.config.clone(),
        });
    }
    drop(map);
    save_persisted_servers(app, &servers)
}

async fn connect_server(
    config: &McpServerConfig,
) -> (
    McpRuntimeStatus,
    usize,
    Option<String>,
    Option<ServerConnection>,
) {
    match config {
        McpServerConfig::Stdio { command, args, env } => match connect_stdio(command, args, env).await {
            Ok(mut conn) => {
                let tool_count = match timeout(MCP_CONNECT_TIMEOUT, conn.list_tools()).await {
                    Err(_) => {
                        let _ = conn.shutdown().await;
                        return (
                            McpRuntimeStatus::Error,
                            0,
                            Some("MCP tools/list timeout (30s). Server may still be downloading dependencies or stuck during startup.".to_string()),
                            None,
                        )
                    }
                    Ok(result) => match result {
                    Ok(tools) => tools.len(),
                    Err(e) => {
                        return (
                            McpRuntimeStatus::Connected,
                            0,
                            Some(e),
                            Some(ServerConnection::Stdio(conn)),
                        )
                    }
                    },
                };
                (
                    McpRuntimeStatus::Connected,
                    tool_count,
                    None,
                    Some(ServerConnection::Stdio(conn)),
                )
            }
            Err(e) => (McpRuntimeStatus::Error, 0, Some(e), None),
        },
        McpServerConfig::Sse { .. } => (
            McpRuntimeStatus::Error,
            0,
            Some("SSE MCP runtime not implemented yet. Use stdio for now.".to_string()),
            None,
        ),
    }
}

async fn reconnect_server(app: &AppHandle, name: &str) -> Result<(), String> {
    ensure_runtime_loaded(app).await;

    let cfg = {
        let map = runtime().lock().await;
        let server = map
            .get(name)
            .ok_or_else(|| format!("MCP server '{}' not found", name))?;
        if !server.enabled {
            return Err(format!("MCP server '{}' is disabled", name));
        }
        server.config.clone()
    };

    let (status, tool_count, error, connection) = connect_server(&cfg).await;
    let mut map = runtime().lock().await;
    let server = map
        .get_mut(name)
        .ok_or_else(|| format!("MCP server '{}' not found", name))?;

    if let Some(ServerConnection::Stdio(conn)) = server.connection.as_mut() {
        conn.shutdown().await;
    }

    server.status = status.clone();
    server.tool_count = tool_count;
    server.error = error.clone();
    server.connection = connection;

    if status == McpRuntimeStatus::Connected {
        Ok(())
    } else {
        Err(error.unwrap_or_else(|| format!("MCP server '{}' failed to reconnect", name)))
    }
}

async fn mark_server_runtime_error(server_name: &str, error: String) {
    let mut map = runtime().lock().await;
    if let Some(server) = map.get_mut(server_name) {
        server.status = McpRuntimeStatus::Error;
        server.error = Some(error);
        server.tool_count = 0;
        server.connection = None;
    }
}

async fn ensure_runtime_loaded(app: &AppHandle) {
    let mut loaded = loaded_flag().lock().await;
    if *loaded {
        return;
    }

    let persisted = load_persisted_servers(app);
    {
        let mut map = runtime().lock().await;
        for item in persisted {
            map.insert(
                item.name,
                RegisteredServer {
                    config: item.config,
                    enabled: item.enabled,
                    status: McpRuntimeStatus::Disconnected,
                    tool_count: 0,
                    error: None,
                    connection: None,
                },
            );
        }
    }

    let names: Vec<String> = {
        let map = runtime().lock().await;
        map.iter()
            .filter_map(|(name, s)| if s.enabled { Some(name.clone()) } else { None })
            .collect()
    };

    for name in names {
        let config = {
            let map = runtime().lock().await;
            map.get(&name).map(|s| s.config.clone())
        };

        if let Some(cfg) = config {
            let (status, tool_count, error, connection) = connect_server(&cfg).await;
            let mut map = runtime().lock().await;
            if let Some(server) = map.get_mut(&name) {
                server.status = status;
                server.tool_count = tool_count;
                server.error = error;
                server.connection = connection;
            }
        }
    }

    *loaded = true;
}

pub async fn add_mcp_server(app: AppHandle, name: String, config: McpServerConfig) -> Result<(), String> {
    ensure_runtime_loaded(&app).await;

    if name.trim().is_empty() {
        return Err("Server name cannot be empty".into());
    }

    let (status, connected_tools, error, connection) = connect_server(&config).await;

    let mut map = runtime().lock().await;
    if let Some(mut old) = map.remove(&name) {
        if let Some(ServerConnection::Stdio(conn)) = old.connection.as_mut() {
            conn.shutdown().await;
        }
    }

    map.insert(
        name,
        RegisteredServer {
            config,
            enabled: true,
            status,
            tool_count: connected_tools,
            error,
            connection,
        },
    );
    drop(map);

    persist_runtime(&app).await?;

    Ok(())
}

pub async fn remove_mcp_server(app: AppHandle, name: String) -> Result<(), String> {
    ensure_runtime_loaded(&app).await;

    let mut map = runtime().lock().await;
    if let Some(mut item) = map.remove(&name) {
        if let Some(ServerConnection::Stdio(conn)) = item.connection.as_mut() {
            conn.shutdown().await;
        }
    }
    drop(map);

    persist_runtime(&app).await?;

    Ok(())
}

pub async fn get_mcp_server_statuses(app: AppHandle) -> Result<Vec<McpServerStatus>, String> {
    ensure_runtime_loaded(&app).await;

    let map = runtime().lock().await;
    let mut result = Vec::new();
    for (name, item) in map.iter() {
        result.push(McpServerStatus {
            name: name.clone(),
            status: item.status.as_str().to_string(),
            enabled: item.enabled,
            r#type: server_type(&item.config),
            tool_count: item.tool_count,
            error: item.error.clone(),
        });
    }
    Ok(result)
}

pub async fn reload_all_mcp_servers(app: AppHandle) -> Result<(), String> {
    ensure_runtime_loaded(&app).await;

    let configs: Vec<(String, McpServerConfig, bool)> = {
        let map = runtime().lock().await;
        map.iter()
            .map(|(k, v)| (k.clone(), v.config.clone(), v.enabled))
            .collect()
    };

    for (name, cfg, enabled) in configs {
        if !enabled {
            let mut map = runtime().lock().await;
            if let Some(server) = map.get_mut(&name) {
                server.status = McpRuntimeStatus::Disconnected;
                server.tool_count = 0;
                server.error = None;
                if let Some(ServerConnection::Stdio(conn)) = server.connection.as_mut() {
                    conn.shutdown().await;
                }
                server.connection = None;
            }
            continue;
        }

        let (status, tool_count, error, connection) = connect_server(&cfg).await;
        let mut map = runtime().lock().await;
        if let Some(server) = map.get_mut(&name) {
            if let Some(ServerConnection::Stdio(conn)) = server.connection.as_mut() {
                conn.shutdown().await;
            }
            server.status = status;
            server.tool_count = tool_count;
            server.error = error;
            server.connection = connection;
        }
    }

    persist_runtime(&app).await?;

    Ok(())
}

pub async fn set_mcp_server_enabled(app: AppHandle, name: String, enabled: bool) -> Result<(), String> {
    ensure_runtime_loaded(&app).await;

    let cfg = {
        let map = runtime().lock().await;
        let server = map
            .get(&name)
            .ok_or_else(|| format!("MCP server '{}' not found", name))?;
        server.config.clone()
    };

    if enabled {
        let (status, tool_count, error, connection) = connect_server(&cfg).await;
        let mut map = runtime().lock().await;
        if let Some(server) = map.get_mut(&name) {
            if let Some(ServerConnection::Stdio(conn)) = server.connection.as_mut() {
                conn.shutdown().await;
            }
            server.enabled = true;
            server.status = status;
            server.tool_count = tool_count;
            server.error = error;
            server.connection = connection;
        }
    } else {
        let mut map = runtime().lock().await;
        if let Some(server) = map.get_mut(&name) {
            if let Some(ServerConnection::Stdio(conn)) = server.connection.as_mut() {
                conn.shutdown().await;
            }
            server.enabled = false;
            server.status = McpRuntimeStatus::Disconnected;
            server.tool_count = 0;
            server.error = None;
            server.connection = None;
        }
    }

    persist_runtime(&app).await?;
    Ok(())
}

pub async fn list_mcp_tools(app: AppHandle, server_name: String) -> Result<Vec<McpToolInfo>, String> {
    ensure_runtime_loaded(&app).await;

    let needs_reconnect = {
        let map = runtime().lock().await;
        let server = map
            .get(&server_name)
            .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;
        server.enabled && server.connection.is_none()
    };

    if needs_reconnect {
        reconnect_server(&app, &server_name).await?;
    }

    let first_attempt = {
        let mut map = runtime().lock().await;
        let server = map
            .get_mut(&server_name)
            .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;

        match server.connection.as_mut() {
            Some(ServerConnection::Stdio(conn)) => conn.list_tools().await,
            None => Err("Server is not connected".into()),
        }
    };

    let tools = match first_attempt {
        Ok(v) => v,
        Err(e) => {
            mark_server_runtime_error(&server_name, e.clone()).await;
            reconnect_server(&app, &server_name).await?;

            let mut map = runtime().lock().await;
            let server = map
                .get_mut(&server_name)
                .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;

            match server.connection.as_mut() {
                Some(ServerConnection::Stdio(conn)) => conn.list_tools().await?,
                None => return Err("Server is not connected".into()),
            }
        }
    };

    let mut map = runtime().lock().await;
    let server = map
        .get_mut(&server_name)
        .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;
    server.tool_count = tools.len();
    server.status = McpRuntimeStatus::Connected;
    server.error = None;
    Ok(tools)
}

pub async fn list_mcp_resources(app: AppHandle, server_name: String) -> Result<Vec<McpResourceInfo>, String> {
    ensure_runtime_loaded(&app).await;

    let needs_reconnect = {
        let map = runtime().lock().await;
        let server = map
            .get(&server_name)
            .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;
        server.enabled && server.connection.is_none()
    };

    if needs_reconnect {
        reconnect_server(&app, &server_name).await?;
    }

    let first_attempt = {
        let mut map = runtime().lock().await;
        let server = map
            .get_mut(&server_name)
            .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;

        match server.connection.as_mut() {
            Some(ServerConnection::Stdio(conn)) => conn.list_resources().await,
            None => Err("Server is not connected".into()),
        }
    };

    let resources = match first_attempt {
        Ok(v) => v,
        Err(e) => {
            mark_server_runtime_error(&server_name, e.clone()).await;
            reconnect_server(&app, &server_name).await?;

            let mut map = runtime().lock().await;
            let server = map
                .get_mut(&server_name)
                .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;

            match server.connection.as_mut() {
                Some(ServerConnection::Stdio(conn)) => conn.list_resources().await?,
                None => return Err("Server is not connected".into()),
            }
        }
    };

    let mut map = runtime().lock().await;
    let server = map
        .get_mut(&server_name)
        .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;
    server.status = McpRuntimeStatus::Connected;
    server.error = None;
    Ok(resources)
}

pub async fn read_mcp_resource(
    app: AppHandle,
    server_name: String,
    uri: String,
) -> Result<Value, String> {
    ensure_runtime_loaded(&app).await;

    let needs_reconnect = {
        let map = runtime().lock().await;
        let server = map
            .get(&server_name)
            .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;
        server.enabled && server.connection.is_none()
    };

    if needs_reconnect {
        reconnect_server(&app, &server_name).await?;
    }

    let first_attempt = {
        let mut map = runtime().lock().await;
        let server = map
            .get_mut(&server_name)
            .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;

        match server.connection.as_mut() {
            Some(ServerConnection::Stdio(conn)) => conn.read_resource(&uri).await,
            None => Err("Server is not connected".into()),
        }
    };

    let value = match first_attempt {
        Ok(v) => v,
        Err(e) => {
            mark_server_runtime_error(&server_name, e.clone()).await;
            reconnect_server(&app, &server_name).await?;

            let mut map = runtime().lock().await;
            let server = map
                .get_mut(&server_name)
                .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;

            match server.connection.as_mut() {
                Some(ServerConnection::Stdio(conn)) => conn.read_resource(&uri).await?,
                None => return Err("Server is not connected".into()),
            }
        }
    };

    let mut map = runtime().lock().await;
    let server = map
        .get_mut(&server_name)
        .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;
    server.status = McpRuntimeStatus::Connected;
    server.error = None;
    Ok(value)
}

pub async fn call_mcp_tool(
    app: AppHandle,
    server_name: String,
    tool_name: String,
    arguments: Value,
) -> Result<Value, String> {
    ensure_runtime_loaded(&app).await;

    let needs_reconnect = {
        let map = runtime().lock().await;
        let server = map
            .get(&server_name)
            .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;
        server.enabled && server.connection.is_none()
    };

    if needs_reconnect {
        reconnect_server(&app, &server_name).await?;
    }

    let first_attempt = {
        let mut map = runtime().lock().await;
        let server = map
            .get_mut(&server_name)
            .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;

        match server.connection.as_mut() {
            Some(ServerConnection::Stdio(conn)) => conn.call_tool(&tool_name, arguments.clone()).await,
            None => Err("Server is not connected".into()),
        }
    };

    let result = match first_attempt {
        Ok(v) => v,
        Err(e) => {
            mark_server_runtime_error(&server_name, e.clone()).await;
            reconnect_server(&app, &server_name).await?;

            let mut map = runtime().lock().await;
            let server = map
                .get_mut(&server_name)
                .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;

            match server.connection.as_mut() {
                Some(ServerConnection::Stdio(conn)) => conn.call_tool(&tool_name, arguments).await?,
                None => return Err("Server is not connected".into()),
            }
        }
    };

    let mut map = runtime().lock().await;
    let server = map
        .get_mut(&server_name)
        .ok_or_else(|| format!("MCP server '{}' not found", server_name))?;
    server.status = McpRuntimeStatus::Connected;
    server.error = None;
    Ok(result)
}

pub async fn warmup_runtime(app: AppHandle) -> Result<(), String> {
    ensure_runtime_loaded(&app).await;

    let has_enabled = {
        let map = runtime().lock().await;
        map.values().any(|s| s.enabled)
    };

    if has_enabled {
        let _ = reload_all_mcp_servers(app.clone()).await;
    }

    Ok(())
}
