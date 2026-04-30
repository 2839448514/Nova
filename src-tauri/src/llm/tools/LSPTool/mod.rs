use crate::llm::tools::{app_tool, AppExecuteFuture, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};
use tauri::AppHandle;

// 把 LSP 工具的 async 执行逻辑包装成统一 future。
// `conversation_id` 会继续传给嵌套的 MCP 权限流程，`input` 里包含 action/server/tool/arguments。
fn execute_with_app_boxed(
    app: AppHandle,
    conversation_id: Option<String>,
    input: Value,
) -> AppExecuteFuture {
    Box::pin(async move { execute_with_app(&app, conversation_id.as_deref(), input).await })
}

// 返回 lsp_tool 的注册信息。
// 这是只读语义工具，主要做代码导航和诊断查询，不直接改写本地状态。
pub(crate) fn registration() -> ToolRegistration {
    app_tool(tool, execute, execute_with_app_boxed, true, None)
}

// 返回模型可见的 lsp_tool 元数据。
// `action` 决定本次是列 server、列工具，还是执行符号/引用/定义/诊断类查询。
pub fn tool() -> Tool {
    Tool {
        name: "lsp_tool".into(),
        description: "Run semantic code-navigation operations via MCP-backed LSP servers (list servers/tools, call, find symbol/references/definition/implementation, diagnostics).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "list_servers",
                        "list_server_tools",
                        "call",
                        "find_symbol",
                        "find_references",
                        "find_definition",
                        "find_implementation",
                        "diagnostics"
                    ]
                },
                "server": { "type": "string" },
                "tool": { "type": "string" },
                "symbol": { "type": "string" },
                "file": { "type": "string" },
                "lineContent": { "type": "string" },
                "arguments": { "type": "object" }
            },
            "required": ["action"]
        }),
    }
}

// 同步入口只返回提示，要求调用方改走带 AppHandle 的 LSP/MCP 执行路径。
pub fn execute(input: Value) -> String {
    let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("unknown");
    json!({
        "ok": false,
        "action": action,
        "message": "lsp_tool requires AppHandle-aware execution and should be routed via execute_tool_with_app."
    })
    .to_string()
}

// 根据高层 action 返回一组用于匹配 MCP 工具名的关键词。
// 这些关键词只用来“猜”哪个 MCP 工具最像 symbol/reference/definition 等操作。
fn lsp_keywords_for_action(action: &str) -> &'static [&'static str] {
    match action {
        "find_symbol" => &["symbol", "workspace", "document_symbol", "symbols"],
        "find_references" => &["reference", "references", "usage", "usages"],
        "find_definition" => &["definition", "goto_definition", "definitions"],
        "find_implementation" => &["implementation", "implementations"],
        "diagnostics" => &["diagnostic", "diagnostics", "problem", "problems", "error", "errors"],
        _ => &[],
    }
}

// 粗略判断一个 MCP 工具名是否看起来像 LSP 相关工具。
// `name` 会先转小写，再跟一组常见关键字做包含匹配。
fn is_lsp_candidate_tool_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "lsp",
        "symbol",
        "reference",
        "definition",
        "implementation",
        "diagnostic",
        "workspace",
        "document",
        "hover",
        "rename",
        "usage",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
}

// 从某个 server 暴露的工具列表里挑一个最适合当前 action 的工具名。
// `tools` 是 MCP 返回的工具清单，命中后返回原始工具名供后续真正调用。
fn choose_lsp_tool_name(action: &str, tools: &[crate::command::mcp::McpToolInfo]) -> Option<String> {
    let keywords = lsp_keywords_for_action(action);
    if keywords.is_empty() {
        return None;
    }

    tools
        .iter()
        .find(|tool| {
            let name = tool.name.to_ascii_lowercase();
            keywords.iter().any(|kw| name.contains(kw))
        })
        .map(|tool| tool.name.clone())
}

// 把高层字段并入底层 MCP 调用参数。
// `arguments` 是模型直接传来的原始参数对象，若没带 `symbol/file/lineContent`，这里会从顶层字段补进去。
fn merge_lsp_arguments(input: &Value) -> Value {
    // map: 最终要传给 MCP LSP 工具的参数对象，会在这里被补齐常见字段。
    let mut map = input
        .get("arguments")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    if !map.contains_key("symbol") {
        if let Some(symbol) = input
            .get("symbol")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            map.insert("symbol".to_string(), Value::String(symbol.to_string()));
        }
    }

    if !map.contains_key("file") {
        if let Some(file) = input
            .get("file")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            map.insert("file".to_string(), Value::String(file.to_string()));
        }
    }

    if !map.contains_key("lineContent") {
        if let Some(line_content) = input
            .get("lineContent")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            map.insert(
                "lineContent".to_string(),
                Value::String(line_content.to_string()),
            );
        }
    }

    Value::Object(map)
}

// 根据 `action` 执行 LSP 相关操作。
// `target_server` 是最终选中的 MCP server，`target_tool` 是最终要调用的具体 MCP 工具名。
pub async fn execute_with_app(
    app: &AppHandle,
    conversation_id: Option<&str>,
    input: Value,
) -> String {
    // action: 统一转成小写后的操作类型，避免大小写不同导致分支判断失败。
    let action = input
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    match action.as_str() {
        "list_servers" => {
            let statuses = match crate::command::mcp::get_mcp_server_statuses(app.clone()).await {
                Ok(v) => v,
                Err(e) => {
                    return json!({ "ok": false, "error": e }).to_string();
                }
            };

            // rows: 返回给模型的 server 摘要列表，每项会附带该 server 暴露的 LSP 风格工具。
            let mut rows = Vec::new();
            for status in statuses {
                let lsp_tools = if status.status == "connected" {
                    match crate::command::mcp::list_mcp_tools(app.clone(), status.name.clone()).await {
                        Ok(tools) => tools
                            .into_iter()
                            .map(|t| t.name)
                            .filter(|name| is_lsp_candidate_tool_name(name))
                            .collect::<Vec<_>>(),
                        Err(_) => Vec::new(),
                    }
                } else {
                    Vec::new()
                };

                rows.push(json!({
                    "name": status.name,
                    "status": status.status,
                    "enabled": status.enabled,
                    "type": status.r#type,
                    "toolCount": status.tool_count,
                    "error": status.error,
                    "lspToolCount": lsp_tools.len(),
                    "lspTools": lsp_tools,
                }));
            }

            json!({
                "ok": true,
                "action": "list_servers",
                "servers": rows
            })
            .to_string()
        }
        "list_server_tools" => {
            let Some(server_name) = input
                .get("server")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return json!({
                    "ok": false,
                    "error": "lsp_tool list_server_tools requires non-empty 'server'"
                })
                .to_string();
            };

            match crate::command::mcp::list_mcp_tools(app.clone(), server_name.to_string()).await {
                Ok(tools) => {
                    let lsp_tools = tools
                        .iter()
                        .map(|t| t.name.clone())
                        .filter(|name| is_lsp_candidate_tool_name(name))
                        .collect::<Vec<_>>();

                    json!({
                        "ok": true,
                        "action": "list_server_tools",
                        "server": server_name,
                        "tools": tools,
                        "lspTools": lsp_tools,
                    })
                    .to_string()
                }
                Err(e) => json!({ "ok": false, "error": e }).to_string(),
            }
        }
        "call" | "find_symbol" | "find_references" | "find_definition" | "find_implementation" | "diagnostics" => {
            // explicit_server: 模型如果明确指定了 server，就直接使用；否则后面会自动挑一个可用的。
            let explicit_server = input
                .get("server")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());

            let target_server = if let Some(server) = explicit_server {
                server
            } else {
                let statuses = match crate::command::mcp::get_mcp_server_statuses(app.clone()).await {
                    Ok(v) => v,
                    Err(e) => {
                        return json!({ "ok": false, "error": e }).to_string();
                    }
                };

                let mut chosen = None;
                for status in statuses
                    .into_iter()
                    .filter(|s| s.enabled && s.status == "connected")
                {
                    if let Ok(tools) =
                        crate::command::mcp::list_mcp_tools(app.clone(), status.name.clone()).await
                    {
                        if tools.iter().any(|t| is_lsp_candidate_tool_name(&t.name)) {
                            chosen = Some(status.name);
                            break;
                        }
                    }
                }

                let Some(server) = chosen else {
                    return json!({
                        "ok": false,
                        "error": "No connected MCP server exposing LSP-like tools; set 'server' explicitly or connect an LSP MCP server"
                    })
                    .to_string();
                };
                server
            };

            let available_tools =
                match crate::command::mcp::list_mcp_tools(app.clone(), target_server.clone()).await {
                    Ok(v) => v,
                    Err(e) => {
                        return json!({ "ok": false, "error": e }).to_string();
                    }
                };

            // explicit_tool: `action=call` 时允许模型手动指定底层 MCP 工具名。
            let explicit_tool = input
                .get("tool")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());

            let target_tool = if let Some(tool_name) = explicit_tool {
                tool_name
            } else if action == "call" {
                return json!({
                    "ok": false,
                    "error": "lsp_tool call requires non-empty 'tool'"
                })
                .to_string();
            } else {
                let Some(tool_name) = choose_lsp_tool_name(&action, &available_tools) else {
                    let names = available_tools.into_iter().map(|t| t.name).collect::<Vec<_>>();
                    return json!({
                        "ok": false,
                        "error": format!("No suitable LSP tool found for action '{}' on server '{}'", action, target_server),
                        "availableTools": names
                    })
                    .to_string();
                };
                tool_name
            };

            // call_output: 调 MCP 工具后的原始字符串输出，可能是普通 JSON，也可能是权限等待 payload。
            let call_output =
                crate::llm::tools::shared::permission_runtime::call_mcp_tool_with_nested_permission(
                    app,
                    conversation_id,
                    target_server,
                    target_tool,
                    merge_lsp_arguments(&input),
                )
                .await;

            if crate::llm::tools::shared::permission_runtime::is_needs_user_input_payload(&call_output)
            {
                call_output
            } else {
                let parsed = serde_json::from_str::<Value>(&call_output)
                    .unwrap_or_else(|_| Value::String(call_output.clone()));
                json!({
                    "ok": true,
                    "action": action,
                    "result": parsed
                })
                .to_string()
            }
        }
        _ => json!({
            "ok": false,
            "error": "lsp_tool action must be one of: list_servers, list_server_tools, call, find_symbol, find_references, find_definition, find_implementation, diagnostics"
        })
        .to_string(),
    }
}
