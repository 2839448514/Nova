// 这是工具注册入口模块，定义了所有内置工具（Bash/PowerShell/File/Task/... 等）
// 以及工具发现、执行、权限检查的统一接口。
#[path = "BashTool/mod.rs"]
pub mod bash_tool;
#[path = "WriteFileTool/mod.rs"]
pub mod write_file_tool;
#[path = "GrepSearchTool/mod.rs"]
pub mod grep_search_tool;
pub mod shared;
#[path = "GlobTool/mod.rs"]
pub mod glob_tool;
#[path = "PowerShellTool/mod.rs"]
pub mod powershell_tool;
#[path = "WebFetchTool/mod.rs"]
pub mod web_fetch_tool;
#[path = "WebSearchTool/mod.rs"]
pub mod web_search_tool;
#[path = "TaskCreateTool/mod.rs"]
pub mod task_create_tool;
#[path = "TaskListTool/mod.rs"]
pub mod task_list_tool;
#[path = "TaskUpdateTool/mod.rs"]
pub mod task_update_tool;
#[path = "TaskGetTool/mod.rs"]
pub mod task_get_tool;
#[path = "TaskOutputTool/mod.rs"]
pub mod task_output_tool;
#[path = "TaskStopTool/mod.rs"]
pub mod task_stop_tool;
#[path = "TaskCreateCompatTool/mod.rs"]
pub mod task_create_compat_tool;
#[path = "TaskListCompatTool/mod.rs"]
pub mod task_list_compat_tool;
#[path = "TaskUpdateCompatTool/mod.rs"]
pub mod task_update_compat_tool;
#[path = "SkillTool/mod.rs"]
pub mod skill_tool;
#[path = "TodoWriteTool/mod.rs"]
pub mod todo_write_tool;
#[path = "ToolSearchTool/mod.rs"]
pub mod tool_search_tool;
#[path = "MCPTool/mod.rs"]
pub mod mcp_tool;
#[path = "ListMcpResourcesTool/mod.rs"]
pub mod list_mcp_resources_tool;
#[path = "ReadMcpResourceTool/mod.rs"]
pub mod read_mcp_resource_tool;
#[path = "McpAuthTool/mod.rs"]
pub mod mcp_auth_tool;
#[path = "LSPTool/mod.rs"]
pub mod lsp_tool;
#[path = "FileReadTool/mod.rs"]
pub mod file_read_tool;
#[path = "FileEditTool/mod.rs"]
pub mod file_edit_tool;
#[path = "AskUserQuestionTool/mod.rs"]
pub mod ask_user_question_tool;
#[path = "ConfigTool/mod.rs"]
pub mod config_tool;
#[path = "EnterPlanModeTool/mod.rs"]
pub mod enter_plan_mode_tool;
#[path = "ExitPlanModeTool/mod.rs"]
pub mod exit_plan_mode_tool;

// Placeholder migration modules stay out of `registered_tools()` until their
// runtime bridge is complete. This avoids exposing Claude-style folders as if
// they were fully migrated Nova tools.

use crate::llm::types::Tool;
use crate::llm::query_engine::ChatMessageEvent;
use std::collections::{BTreeMap, VecDeque};
use crate::llm::services::mcp_tools::parse_mcp_tool_name;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::task::JoinSet;

struct RegisteredTool {
    tool: fn() -> Tool,
    execute: fn(Value) -> String,
}

#[derive(Debug, Clone)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone)]
pub struct ToolCallResult {
    pub id: String,
    pub name: String,
    pub input: Value,
    pub output: String,
    pub is_error: bool,
    pub additional_messages: Vec<crate::llm::types::Message>,
    pub prevent_continuation: bool,
    pub stop_reason: Option<String>,
}

fn find_tool_definition(name: &str) -> Option<Tool> {
    registered_tools().into_iter().find_map(|entry| {
        let tool = (entry.tool)();
        if tool.name == name {
            Some(tool)
        } else {
            None
        }
    })
}

fn validate_type(value: &Value, expected: &str) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn validate_schema_fragment(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    if let Some(expected_type) = schema.get("type").and_then(|v| v.as_str()) {
        if !validate_type(value, expected_type) {
            return Err(format!(
                "Input validation failed for '{}': expected {}, got {}",
                path,
                expected_type,
                value
            ));
        }
    }

    if let Some(enum_values) = schema.get("enum").and_then(|v| v.as_array()) {
        if !enum_values.iter().any(|allowed| allowed == value) {
            return Err(format!(
                "Input validation failed for '{}': value not in enum",
                path
            ));
        }
    }

    if schema.get("type").and_then(|v| v.as_str()) == Some("object") {
        let object = value
            .as_object()
            .ok_or_else(|| format!("Input validation failed for '{}': expected object", path))?;

        if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
            for key in required.iter().filter_map(|k| k.as_str()) {
                if !object.contains_key(key) {
                    return Err(format!(
                        "Input validation failed for '{}': missing required field '{}'",
                        path, key
                    ));
                }
            }
        }

        if let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) {
            for (key, sub_schema) in properties {
                if let Some(sub_value) = object.get(key) {
                    let sub_path = format!("{}.{}", path, key);
                    validate_schema_fragment(sub_value, sub_schema, &sub_path)?;
                }
            }
        }
    }

    if schema.get("type").and_then(|v| v.as_str()) == Some("array") {
        let array = value
            .as_array()
            .ok_or_else(|| format!("Input validation failed for '{}': expected array", path))?;

        if let Some(item_schema) = schema.get("items") {
            for (index, item) in array.iter().enumerate() {
                let sub_path = format!("{}[{}]", path, index);
                validate_schema_fragment(item, item_schema, &sub_path)?;
            }
        }
    }

    Ok(())
}

fn validate_tool_input(name: &str, input: &Value) -> Result<(), String> {
    if let Some(tool) = find_tool_definition(name) {
        return validate_schema_fragment(input, &tool.input_schema, name);
    }

    Ok(())
}

fn validate_tool_output(name: &str, output: &str) -> Result<(), String> {
    let trimmed = output.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        serde_json::from_str::<Value>(trimmed).map_err(|e| {
            format!(
                "Output validation failed for '{}': invalid JSON payload ({})",
                name, e
            )
        })?;
    }
    Ok(())
}

fn infer_is_error(output: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(output) else {
        return false;
    };

    if v
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t == "needs_user_input")
        .unwrap_or(false)
    {
        return false;
    }

    if v.get("ok").and_then(|ok| ok.as_bool()) == Some(false) {
        return true;
    }

    v.get("error").is_some() && v.get("ok").is_none()
}

pub(crate) fn is_read_only_tool(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "read_file"
        | "grep_search"
        | "glob_search"
        | "web_fetch"
        | "web_search"
        | "task_list"
        | "taskget"
        | "taskoutput"
        | "task_get"
        | "task_output"
        | "tool_search"
        | "list_mcp_resources"
        | "read_mcp_resource"
        | "skill"
        | "lsp_tool" => true,
        _ => {
            if let Some((_server, tool_name)) = parse_mcp_tool_name(name) {
                let tool_lower = tool_name.to_ascii_lowercase();
                return ["read", "list", "search", "get", "fetch", "glob", "grep"]
                    .iter()
                    .any(|kw| tool_lower.contains(kw));
            }
            false
        }
    }
}

fn max_tool_use_concurrency() -> usize {
    std::env::var("NOVA_MAX_TOOL_USE_CONCURRENCY")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(10)
}

fn cancelled_result_from_call(call: ToolCallRequest, reason: &str) -> ToolCallResult {
    ToolCallResult {
        id: call.id,
        name: call.name,
        input: call.input,
        output: json!({ "ok": false, "error": reason }).to_string(),
        is_error: true,
        additional_messages: Vec::new(),
        prevent_continuation: false,
        stop_reason: None,
    }
}

pub(crate) async fn execute_single_tool_call(
    app: &AppHandle,
    conversation_id: Option<&str>,
    call: ToolCallRequest,
) -> ToolCallResult {
    let ToolCallRequest { id, name, input } = call;
    let mut additional_messages = Vec::new();
    let mut prevent_continuation = false;
    let mut stop_reason: Option<String> = None;

    let pre_hook = crate::llm::services::tools::run_pre_tool_use_hooks(
        &name,
        &input,
        conversation_id,
    );
    additional_messages.extend(pre_hook.additional_messages);
    if pre_hook.prevent_continuation {
        prevent_continuation = true;
        stop_reason = pre_hook.stop_reason.clone();
    }

    if let Some(err) = pre_hook.override_error {
        return ToolCallResult {
            id,
            name,
            input,
            output: json!({ "ok": false, "error": err }).to_string(),
            is_error: true,
            additional_messages,
            prevent_continuation,
            stop_reason,
        };
    }

    if let Err(e) = validate_tool_input(&name, &input) {
        let failure_hook = crate::llm::services::tools::run_post_tool_use_failure_hooks(
            &name,
            &input,
            &e,
            conversation_id,
        );
        additional_messages.extend(failure_hook.additional_messages);
        if failure_hook.prevent_continuation {
            prevent_continuation = true;
            if stop_reason.is_none() {
                stop_reason = failure_hook.stop_reason;
            }
        }

        let output = json!({ "ok": false, "error": e }).to_string();
        return ToolCallResult {
            id,
            name,
            input,
            output,
            is_error: true,
            additional_messages,
            prevent_continuation,
            stop_reason,
        };
    }

    let output = if let Some((server_name, tool_name)) = parse_mcp_tool_name(&name) {
        execute_tool_with_app(
            app,
            conversation_id,
            "mcp_tool",
            json!({
                "server": server_name,
                "tool": tool_name,
                "arguments": input.clone(),
            }),
        )
        .await
    } else {
        execute_tool_with_app(app, conversation_id, &name, input.clone()).await
    };

    let validated_output = match validate_tool_output(&name, &output) {
        Ok(()) => output,
        Err(e) => json!({ "ok": false, "error": e }).to_string(),
    };

    let mut is_error = infer_is_error(&validated_output);

    if is_error {
        let failure_hook = crate::llm::services::tools::run_post_tool_use_failure_hooks(
            &name,
            &input,
            &validated_output,
            conversation_id,
        );
        additional_messages.extend(failure_hook.additional_messages);
        if failure_hook.prevent_continuation {
            prevent_continuation = true;
            if stop_reason.is_none() {
                stop_reason = failure_hook.stop_reason;
            }
        }
    }

    let post_hook = crate::llm::services::tools::run_post_tool_use_hooks(
        &name,
        &input,
        &validated_output,
        is_error,
        conversation_id,
    );
    additional_messages.extend(post_hook.additional_messages);
    if post_hook.prevent_continuation {
        prevent_continuation = true;
        if stop_reason.is_none() {
            stop_reason = post_hook.stop_reason;
        }
    }

    let final_output = if let Some(err) = post_hook.override_error {
        is_error = true;
        json!({ "ok": false, "error": err }).to_string()
    } else {
        validated_output
    };

    ToolCallResult {
        id,
        name,
        input,
        is_error,
        output: final_output,
        additional_messages,
        prevent_continuation,
        stop_reason,
    }
}

async fn execute_read_only_batch(
    app: &AppHandle,
    conversation_id: Option<&str>,
    calls: Vec<ToolCallRequest>,
) -> Vec<ToolCallResult> {
    let total = calls.len();
    if total == 0 {
        return Vec::new();
    }

    let mut queue: VecDeque<(usize, ToolCallRequest)> =
        calls.into_iter().enumerate().collect();
    let mut in_flight: BTreeMap<usize, ToolCallRequest> = BTreeMap::new();
    let mut results_by_index: BTreeMap<usize, ToolCallResult> = BTreeMap::new();
    let mut tasks: JoinSet<(usize, ToolCallResult)> = JoinSet::new();
    let mut cascade_reason: Option<String> = None;
    let max_concurrency = max_tool_use_concurrency();
    let conversation_owned = conversation_id.map(|v| v.to_string());

    while !queue.is_empty() || !tasks.is_empty() {
        while cascade_reason.is_none() && tasks.len() < max_concurrency && !queue.is_empty() {
            let Some((index, call)) = queue.pop_front() else {
                break;
            };

            let app_clone = app.clone();
            let conversation_for_task = conversation_owned.clone();
            in_flight.insert(index, call.clone());
            tasks.spawn(async move {
                let result = execute_single_tool_call(
                    &app_clone,
                    conversation_for_task.as_deref(),
                    call,
                )
                .await;
                (index, result)
            });
        }

        let Some(joined) = tasks.join_next().await else {
            break;
        };

        if let Ok((index, result)) = joined {
            in_flight.remove(&index);
            let is_error = result.is_error;
            let error_tool_name = result.name.clone();
            results_by_index.insert(index, result);

            if cascade_reason.is_none() && is_error {
                cascade_reason = Some(format!(
                    "Cancelled: parallel tool call '{}' errored",
                    error_tool_name
                ));
                tasks.abort_all();

                while let Some(_drained) = tasks.join_next().await {
                    // Ignore aborted task outputs; unresolved calls are converted
                    // into deterministic synthetic cancellations below.
                }
                break;
            }
        }
    }

    if let Some(reason) = cascade_reason {
        for (index, call) in in_flight.into_iter() {
            results_by_index
                .entry(index)
                .or_insert_with(|| cancelled_result_from_call(call, &reason));
        }
        while let Some((index, call)) = queue.pop_front() {
            results_by_index
                .entry(index)
                .or_insert_with(|| cancelled_result_from_call(call, &reason));
        }
    } else {
        for (index, call) in in_flight.into_iter() {
            results_by_index.entry(index).or_insert_with(|| {
                cancelled_result_from_call(call, "cancelled: read-only task aborted")
            });
        }
        while let Some((index, call)) = queue.pop_front() {
            results_by_index.entry(index).or_insert_with(|| {
                cancelled_result_from_call(call, "cancelled: read-only task not executed")
            });
        }
    }

    let mut ordered_results = Vec::with_capacity(total);
    for index in 0..total {
        if let Some(result) = results_by_index.remove(&index) {
            ordered_results.push(result);
        }
    }
    ordered_results
}

async fn flush_read_only_batch(
    app: &AppHandle,
    conversation_id: Option<&str>,
    batch: &mut Vec<ToolCallRequest>,
    out: &mut Vec<ToolCallResult>,
) {
    if batch.is_empty() {
        return;
    }

    let drained = std::mem::take(batch);
    let mut batch_results = execute_read_only_batch(app, conversation_id, drained).await;
    out.append(&mut batch_results);
}

pub async fn execute_tool_calls_with_app(
    app: &AppHandle,
    conversation_id: Option<&str>,
    calls: Vec<ToolCallRequest>,
) -> Vec<ToolCallResult> {
    let mut results: Vec<ToolCallResult> = Vec::with_capacity(calls.len());
    let mut read_only_batch: Vec<ToolCallRequest> = Vec::new();

    for call in calls {
        if crate::llm::cancellation::is_cancelled(conversation_id) {
            results.push(cancelled_result_from_call(call, "cancelled"));
            continue;
        }

        if is_read_only_tool(&call.name) {
            read_only_batch.push(call);
            continue;
        }

        flush_read_only_batch(app, conversation_id, &mut read_only_batch, &mut results).await;
        results.push(execute_single_tool_call(app, conversation_id, call).await);
    }

    flush_read_only_batch(app, conversation_id, &mut read_only_batch, &mut results).await;
    results
}

fn registered_tools() -> Vec<RegisteredTool> {
    vec![
        RegisteredTool {
            tool: bash_tool::tool,
            execute: bash_tool::execute,
        },
        RegisteredTool {
            tool: powershell_tool::tool,
            execute: powershell_tool::execute,
        },
        RegisteredTool {
            tool: file_read_tool::tool,
            execute: file_read_tool::execute,
        },
        RegisteredTool {
            tool: write_file_tool::tool,
            execute: write_file_tool::execute,
        },
        RegisteredTool {
            tool: file_edit_tool::tool,
            execute: file_edit_tool::execute,
        },
        RegisteredTool {
            tool: grep_search_tool::tool,
            execute: grep_search_tool::execute,
        },
        RegisteredTool {
            tool: glob_tool::tool,
            execute: glob_tool::execute,
        },
        RegisteredTool {
            tool: web_fetch_tool::tool,
            execute: web_fetch_tool::execute,
        },
        RegisteredTool {
            tool: web_search_tool::tool,
            execute: web_search_tool::execute,
        },
        RegisteredTool {
            tool: task_create_tool::tool,
            execute: task_create_tool::execute,
        },
        RegisteredTool {
            tool: task_create_compat_tool::tool,
            execute: task_create_compat_tool::execute,
        },
        RegisteredTool {
            tool: task_list_tool::tool,
            execute: task_list_tool::execute,
        },
        RegisteredTool {
            tool: task_list_compat_tool::tool,
            execute: task_list_compat_tool::execute,
        },
        RegisteredTool {
            tool: task_update_tool::tool,
            execute: task_update_tool::execute,
        },
        RegisteredTool {
            tool: task_update_compat_tool::tool,
            execute: task_update_compat_tool::execute,
        },
        RegisteredTool {
            tool: task_get_tool::tool,
            execute: task_get_tool::execute,
        },
        RegisteredTool {
            tool: task_output_tool::tool,
            execute: task_output_tool::execute,
        },
        RegisteredTool {
            tool: task_stop_tool::tool,
            execute: task_stop_tool::execute,
        },
        RegisteredTool {
            tool: skill_tool::tool,
            execute: skill_tool::execute,
        },
        RegisteredTool {
            tool: todo_write_tool::tool,
            execute: todo_write_tool::execute,
        },
        RegisteredTool {
            tool: tool_search_tool::tool,
            execute: tool_search_tool::execute,
        },
        RegisteredTool {
            tool: mcp_tool::tool,
            execute: mcp_tool::execute,
        },
        RegisteredTool {
            tool: list_mcp_resources_tool::tool,
            execute: list_mcp_resources_tool::execute,
        },
        RegisteredTool {
            tool: read_mcp_resource_tool::tool,
            execute: read_mcp_resource_tool::execute,
        },
        RegisteredTool {
            tool: mcp_auth_tool::tool,
            execute: mcp_auth_tool::execute,
        },
        RegisteredTool {
            tool: lsp_tool::tool,
            execute: lsp_tool::execute,
        },
        RegisteredTool {
            tool: ask_user_question_tool::tool,
            execute: ask_user_question_tool::execute,
        },
        RegisteredTool {
            tool: enter_plan_mode_tool::tool,
            execute: enter_plan_mode_tool::execute,
        },
        RegisteredTool {
            tool: exit_plan_mode_tool::tool,
            execute: exit_plan_mode_tool::execute,
        },
        RegisteredTool {
            tool: config_tool::tool,
            execute: config_tool::execute,
        },
    ]
}

// 取当前注册工具列表，用于在 LLM 提示里传给模型，告诉模型可调用哪些功能。
pub fn get_available_tools() -> Vec<Tool> {
    registered_tools()
        .into_iter()
        .map(|entry| (entry.tool)())
        .collect()
}

// 在后端直接执行工具，输入来自模型返回的 tool call 名称和参数，只在同步模式下使用。
pub fn execute_tool(name: &str, input: Value) -> String {
    for entry in registered_tools() {
        let tool = (entry.tool)();
        if tool.name == name {
            return (entry.execute)(input);
        }
    }

    format!("Unknown tool: {}", name)
}

fn is_needs_user_input_payload(raw: &str) -> bool {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| {
            v.get("type")
                .and_then(|t| t.as_str())
                .map(|s| s == "needs_user_input")
        })
        .unwrap_or(false)
}

fn permission_wait_timeout_ms() -> u64 {
    std::env::var("NOVA_PERMISSION_WAIT_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(15 * 60 * 1000)
}

async fn await_permission_and_recheck(
    app: &AppHandle,
    conversation_id: Option<&str>,
    tool_name: &str,
    permission_input: &Value,
    request_id: String,
    payload: String,
) -> Result<(), String> {
    app.emit(
        "chat-stream",
        ChatMessageEvent {
            r#type: "permission-request".into(),
            text: Some(payload),
            tool_use_id: Some(request_id.clone()),
            tool_use_name: Some(tool_name.to_string()),
            tool_use_input: None,
            tool_result: None,
            token_usage: None,
            stop_reason: None,
            turn_state: Some("awaiting_permission".into()),
        },
    )
    .map_err(|e| {
        format!(
            "Permission request failed for '{}': unable to notify frontend ({})",
            tool_name, e
        )
    })?;

    let decision = crate::llm::utils::permissions::await_permission_decision(
        conversation_id,
        &request_id,
        permission_wait_timeout_ms(),
    )
    .await
    .map_err(|e| format!("Permission request failed for '{}': {}", tool_name, e))?;

    if matches!(decision, crate::llm::utils::permissions::PermissionAction::DenySession) {
        return Err(format!("Permission denied by user for '{}'", tool_name));
    }

    match crate::llm::utils::permissions::enforce_tool_permission(
        app,
        conversation_id,
        tool_name,
        permission_input,
    ) {
        crate::llm::utils::permissions::PermissionEnforcement::Allow => Ok(()),
        crate::llm::utils::permissions::PermissionEnforcement::Deny(e) => Err(e),
        crate::llm::utils::permissions::PermissionEnforcement::AskUser { .. } => Err(format!(
            "Permission decision for '{}' is still pending",
            tool_name
        )),
    }
}

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

fn merge_lsp_arguments(input: &Value) -> Value {
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

async fn call_mcp_tool_with_nested_permission(
    app: &AppHandle,
    conversation_id: Option<&str>,
    server_name: String,
    tool_name: String,
    arguments: Value,
) -> String {
    let nested_payload = serde_json::json!({
        "server": server_name,
        "tool": tool_name,
        "arguments": arguments,
    });

    match crate::llm::utils::permissions::enforce_tool_permission(
        app,
        conversation_id,
        "mcp_tool",
        &nested_payload,
    ) {
        crate::llm::utils::permissions::PermissionEnforcement::Allow => {}
        crate::llm::utils::permissions::PermissionEnforcement::Deny(e) => {
            return serde_json::json!({ "ok": false, "error": e }).to_string();
        }
        crate::llm::utils::permissions::PermissionEnforcement::AskUser {
            request_id,
            payload,
        } => {
            if let Err(e) = await_permission_and_recheck(
                app,
                conversation_id,
                "mcp_tool",
                &nested_payload,
                request_id,
                payload,
            )
            .await
            {
                return serde_json::json!({ "ok": false, "error": e }).to_string();
            }
        }
    }

    let server = nested_payload
        .get("server")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let tool = nested_payload
        .get("tool")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let args = nested_payload
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    match crate::command::mcp::call_mcp_tool(app.clone(), server, tool, args).await {
        Ok(v) => v.to_string(),
        Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
    }
}

// 在带 AppHandle 的环境中执行工具，附带权限校验和 MCP 代理能力。
// 若权限拒绝返回特殊 JSON payload；允许则执行工具。
pub async fn execute_tool_with_app(
    app: &AppHandle,
    conversation_id: Option<&str>,
    name: &str,
    input: Value,
) -> String {
    match crate::llm::utils::permissions::enforce_tool_permission(
        app,
        conversation_id,
        name,
        &input,
    ) {
        crate::llm::utils::permissions::PermissionEnforcement::Allow => {}
        crate::llm::utils::permissions::PermissionEnforcement::Deny(e) => {
            return serde_json::json!({ "ok": false, "error": e }).to_string();
        }
        crate::llm::utils::permissions::PermissionEnforcement::AskUser {
            request_id,
            payload,
        } => {
            if let Err(e) = await_permission_and_recheck(
                app,
                conversation_id,
                name,
                &input,
                request_id,
                payload,
            )
            .await
            {
                return serde_json::json!({ "ok": false, "error": e }).to_string();
            }
        }
    }

    match name {
        "mcp_tool" => {
            let server_name = input
                .get("server")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let tool_name = input
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let arguments = input
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            if server_name.is_empty() || tool_name.is_empty() {
                return serde_json::json!({
                    "ok": false,
                    "error": "mcp_tool requires non-empty 'server' and 'tool' fields"
                })
                .to_string();
            }

            match crate::command::mcp::call_mcp_tool(
                app.clone(),
                server_name,
                tool_name,
                arguments,
            )
            .await
            {
                Ok(v) => v.to_string(),
                Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
            }
        }
        "list_mcp_resources" => {
            let server_name = input
                .get("server")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();

            if server_name.is_empty() {
                return serde_json::json!({
                    "ok": false,
                    "error": "list_mcp_resources requires non-empty 'server'"
                })
                .to_string();
            }

            match crate::command::mcp::list_mcp_resources(app.clone(), server_name).await {
                Ok(v) => serde_json::json!({ "ok": true, "resources": v }).to_string(),
                Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
            }
        }
        "read_mcp_resource" => {
            let server_name = input
                .get("server")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let uri = input
                .get("resource")
                .or_else(|| input.get("uri"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();

            if server_name.is_empty() || uri.is_empty() {
                return serde_json::json!({
                    "ok": false,
                    "error": "read_mcp_resource requires non-empty 'server' and 'resource'/'uri'"
                })
                .to_string();
            }

            match crate::command::mcp::read_mcp_resource(app.clone(), server_name, uri).await {
                Ok(v) => v.to_string(),
                Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
            }
        }
        "mcp_auth" => {
            let action = input
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();

            match action.as_str() {
                "status" => match crate::command::mcp::get_mcp_server_statuses(app.clone()).await {
                    Ok(statuses) => serde_json::json!({
                        "ok": true,
                        "action": "status",
                        "servers": statuses
                    })
                    .to_string(),
                    Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
                },
                "reload_all" => {
                    if let Err(e) = crate::command::mcp::reload_all_mcp_servers(app.clone()).await {
                        return serde_json::json!({ "ok": false, "error": e }).to_string();
                    }
                    match crate::command::mcp::get_mcp_server_statuses(app.clone()).await {
                        Ok(statuses) => serde_json::json!({
                            "ok": true,
                            "action": "reload_all",
                            "servers": statuses
                        })
                        .to_string(),
                        Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                "enable" | "disable" => {
                    let Some(server_name) = input
                        .get("server")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    else {
                        return serde_json::json!({
                            "ok": false,
                            "error": "mcp_auth action requires non-empty 'server'"
                        })
                        .to_string();
                    };

                    let enabled = action == "enable";
                    match crate::command::mcp::set_mcp_server_enabled(
                        app.clone(),
                        server_name.to_string(),
                        enabled,
                    )
                    .await
                    {
                        Ok(()) => serde_json::json!({
                            "ok": true,
                            "action": action,
                            "server": server_name,
                            "enabled": enabled
                        })
                        .to_string(),
                        Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                "list_tools" => {
                    let Some(server_name) = input
                        .get("server")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    else {
                        return serde_json::json!({
                            "ok": false,
                            "error": "mcp_auth list_tools requires non-empty 'server'"
                        })
                        .to_string();
                    };

                    match crate::command::mcp::list_mcp_tools(app.clone(), server_name.to_string()).await {
                        Ok(tools) => serde_json::json!({
                            "ok": true,
                            "action": "list_tools",
                            "server": server_name,
                            "tools": tools
                        })
                        .to_string(),
                        Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                "probe_tool" => {
                    let server_name = input
                        .get("server")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .unwrap_or_default()
                        .to_string();
                    let tool_name = input
                        .get("tool")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .unwrap_or_default()
                        .to_string();
                    let arguments = input
                        .get("arguments")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));

                    if server_name.is_empty() || tool_name.is_empty() {
                        return serde_json::json!({
                            "ok": false,
                            "error": "mcp_auth probe_tool requires non-empty 'server' and 'tool'"
                        })
                        .to_string();
                    }

                    call_mcp_tool_with_nested_permission(
                        app,
                        conversation_id,
                        server_name,
                        tool_name,
                        arguments,
                    )
                    .await
                }
                _ => serde_json::json!({
                    "ok": false,
                    "error": "mcp_auth action must be one of: status, reload_all, enable, disable, list_tools, probe_tool"
                })
                .to_string(),
            }
        }
        "lsp_tool" => {
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
                            return serde_json::json!({ "ok": false, "error": e }).to_string();
                        }
                    };

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

                        rows.push(serde_json::json!({
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

                    serde_json::json!({
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
                        return serde_json::json!({
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

                            serde_json::json!({
                                "ok": true,
                                "action": "list_server_tools",
                                "server": server_name,
                                "tools": tools,
                                "lspTools": lsp_tools,
                            })
                            .to_string()
                        }
                        Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                "call" | "find_symbol" | "find_references" | "find_definition" | "find_implementation" | "diagnostics" => {
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
                                return serde_json::json!({ "ok": false, "error": e }).to_string();
                            }
                        };

                        let mut chosen = None;
                        for status in statuses.into_iter().filter(|s| s.enabled && s.status == "connected") {
                            if let Ok(tools) = crate::command::mcp::list_mcp_tools(app.clone(), status.name.clone()).await {
                                if tools.iter().any(|t| is_lsp_candidate_tool_name(&t.name)) {
                                    chosen = Some(status.name);
                                    break;
                                }
                            }
                        }

                        let Some(server) = chosen else {
                            return serde_json::json!({
                                "ok": false,
                                "error": "No connected MCP server exposing LSP-like tools; set 'server' explicitly or connect an LSP MCP server"
                            })
                            .to_string();
                        };
                        server
                    };

                    let available_tools = match crate::command::mcp::list_mcp_tools(app.clone(), target_server.clone()).await {
                        Ok(v) => v,
                        Err(e) => {
                            return serde_json::json!({ "ok": false, "error": e }).to_string();
                        }
                    };

                    let explicit_tool = input
                        .get("tool")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string());

                    let target_tool = if let Some(tool_name) = explicit_tool {
                        tool_name
                    } else if action == "call" {
                        return serde_json::json!({
                            "ok": false,
                            "error": "lsp_tool call requires non-empty 'tool'"
                        })
                        .to_string();
                    } else {
                        let Some(tool_name) = choose_lsp_tool_name(&action, &available_tools) else {
                            let names = available_tools.into_iter().map(|t| t.name).collect::<Vec<_>>();
                            return serde_json::json!({
                                "ok": false,
                                "error": format!("No suitable LSP tool found for action '{}' on server '{}'", action, target_server),
                                "availableTools": names
                            })
                            .to_string();
                        };
                        tool_name
                    };

                    let call_output = call_mcp_tool_with_nested_permission(
                        app,
                        conversation_id,
                        target_server,
                        target_tool,
                        merge_lsp_arguments(&input),
                    )
                    .await;

                    if is_needs_user_input_payload(&call_output) {
                        call_output
                    } else {
                        let parsed = serde_json::from_str::<Value>(&call_output)
                            .unwrap_or_else(|_| Value::String(call_output.clone()));
                        serde_json::json!({
                            "ok": true,
                            "action": action,
                            "result": parsed
                        })
                        .to_string()
                    }
                }
                _ => serde_json::json!({
                    "ok": false,
                    "error": "lsp_tool action must be one of: list_servers, list_server_tools, call, find_symbol, find_references, find_definition, find_implementation, diagnostics"
                })
                .to_string(),
            }
        }
        _ => execute_tool(name, input),
    }
}
