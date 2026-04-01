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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpServerConfig {
    Stdio {
        command: String,
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
    Sse {
        url: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStatus {
    pub name: String,
    pub status: String,
    pub enabled: bool,
    pub r#type: String,
    pub tool_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceInfo {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

struct StdioMcpConnection {
    child: Child,
    reader: BufReader<ChildStdout>,
    writer: ChildStdin,
    next_id: u64,
}

impl StdioMcpConnection {
    async fn send_message(&mut self, value: &Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.writer.write_all(&bytes).await.map_err(|e| e.to_string())?;
        self.writer.flush().await.map_err(|e| e.to_string())
    }

    async fn read_message(&mut self) -> Result<Value, String> {
        loop {
            let mut line = String::new();
            let n = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("MCP stdio stream closed".into());
            }

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<Value>(line) {
                Ok(v) => return Ok(v),
                Err(_) => {
                    // Some servers may print startup logs on stdout before JSON-RPC messages.
                    continue;
                }
            }
        }
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        self.send_message(&req).await?;

        loop {
            let msg = self.read_message().await?;
            let msg_id = msg
                .get("id")
                .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok())));
            if msg_id != Some(id) {
                continue;
            }
            if let Some(err) = msg.get("error") {
                return Err(format!("MCP error: {}", err));
            }
            return Ok(msg.get("result").cloned().unwrap_or_else(|| json!({})));
        }
    }

    async fn send_notification(&mut self, method: &str, params: Value) -> Result<(), String> {
        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        self.send_message(&req).await
    }

    async fn list_tools(&mut self) -> Result<Vec<McpToolInfo>, String> {
        let result = self.send_request("tools/list", json!({})).await?;
        let tools = result
            .get("tools")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(tools
            .into_iter()
            .filter_map(|t| {
                let name = t.get("name").and_then(|v| v.as_str())?.to_string();
                let description = t
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
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
        let result = self.send_request("resources/list", json!({})).await?;
        let resources = result
            .get("resources")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

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
        self.send_request("resources/read", json!({ "uri": uri })).await
    }

    async fn shutdown(&mut self) {
        let _ = self.child.kill().await;
    }
}

enum ServerConnection {
    Stdio(StdioMcpConnection),
}

struct RegisteredServer {
    config: McpServerConfig,
    enabled: bool,
    status: String,
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
    let mut parsed_command = command.trim().to_string();
    let mut parsed_args = args.to_vec();
    if parsed_args.is_empty() && parsed_command.contains(' ') {
        let mut parts = parsed_command
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        if !parts.is_empty() {
            parsed_command = parts.remove(0);
            parsed_args = parts;
        }
    }

    let spawn_once = |cmd_name: &str, cmd_args: &[String]| {
        let mut cmd = Command::new(cmd_name);
        cmd.args(cmd_args);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());

        for (k, v) in env_map {
            cmd.env(k, v);
        }

        cmd.spawn()
    };

    let mut child = match spawn_once(&parsed_command, &parsed_args) {
        Ok(child) => child,
        Err(primary_err) => {
            #[cfg(windows)]
            {
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
                return Err(format!(
                    "Failed to spawn MCP server: {}. Hint: for Playwright MCP use command 'npx' with args '-y @playwright/mcp@latest'.",
                    primary_err
                ));
            }
        }
    };
    let stdin = child.stdin.take().ok_or_else(|| "Missing MCP stdin pipe".to_string())?;
    let stdout = child.stdout.take().ok_or_else(|| "Missing MCP stdout pipe".to_string())?;

    let mut conn = StdioMcpConnection {
        child,
        reader: BufReader::new(stdout),
        writer: stdin,
        next_id: 1,
    };

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
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            let _ = conn.shutdown().await;
            return Err("MCP server initialize timeout (30s). First-time npx install may be slow; please retry or pre-run `npx -y @playwright/mcp@latest --help` in terminal.".to_string());
        }
    }

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

async fn connect_server(config: &McpServerConfig) -> (String, usize, Option<String>, Option<ServerConnection>) {
    match config {
        McpServerConfig::Stdio { command, args, env } => match connect_stdio(command, args, env).await {
            Ok(mut conn) => {
                let tool_count = match timeout(MCP_CONNECT_TIMEOUT, conn.list_tools()).await {
                    Err(_) => {
                        let _ = conn.shutdown().await;
                        return (
                            "error".to_string(),
                            0,
                            Some("MCP tools/list timeout (30s). Server may still be downloading dependencies or stuck during startup.".to_string()),
                            None,
                        )
                    }
                    Ok(result) => match result {
                    Ok(tools) => tools.len(),
                    Err(e) => {
                        return (
                            "connected".to_string(),
                            0,
                            Some(e),
                            Some(ServerConnection::Stdio(conn)),
                        )
                    }
                    },
                };
                (
                    "connected".to_string(),
                    tool_count,
                    None,
                    Some(ServerConnection::Stdio(conn)),
                )
            }
            Err(e) => ("error".to_string(), 0, Some(e), None),
        },
        McpServerConfig::Sse { .. } => (
            "error".to_string(),
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

    if status == "connected" {
        Ok(())
    } else {
        Err(error.unwrap_or_else(|| format!("MCP server '{}' failed to reconnect", name)))
    }
}

async fn mark_server_runtime_error(server_name: &str, error: String) {
    let mut map = runtime().lock().await;
    if let Some(server) = map.get_mut(server_name) {
        server.status = "error".to_string();
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
                    status: "disconnected".to_string(),
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

#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
pub async fn get_mcp_server_statuses(app: AppHandle) -> Result<Vec<McpServerStatus>, String> {
    ensure_runtime_loaded(&app).await;

    let map = runtime().lock().await;
    let mut result = Vec::new();
    for (name, item) in map.iter() {
        result.push(McpServerStatus {
            name: name.clone(),
            status: item.status.clone(),
            enabled: item.enabled,
            r#type: server_type(&item.config),
            tool_count: item.tool_count,
            error: item.error.clone(),
        });
    }
    Ok(result)
}

#[tauri::command]
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
                server.status = "disconnected".to_string();
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

#[tauri::command]
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
            server.status = "disconnected".to_string();
            server.tool_count = 0;
            server.error = None;
            server.connection = None;
        }
    }

    persist_runtime(&app).await?;
    Ok(())
}

#[tauri::command]
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
    server.status = "connected".into();
    server.error = None;
    Ok(tools)
}

#[tauri::command]
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
    server.status = "connected".into();
    server.error = None;
    Ok(resources)
}

#[tauri::command]
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
    server.status = "connected".into();
    server.error = None;
    Ok(value)
}

#[tauri::command]
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
    server.status = "connected".into();
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
