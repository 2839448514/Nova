use crate::llm::query_engine::ChatMessageEvent;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

pub fn is_needs_user_input_payload(raw: &str) -> bool {
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

pub async fn await_permission_and_recheck(
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
            conversation_id: conversation_id.map(str::to_string),
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

    if matches!(
        decision,
        crate::llm::utils::permissions::PermissionAction::DenySession
    ) {
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
        crate::llm::utils::permissions::PermissionEnforcement::AskUser { .. } => {
            Err(format!("Permission decision for '{}' is still pending", tool_name))
        }
    }
}

pub async fn call_mcp_tool_with_nested_permission(
    app: &AppHandle,
    conversation_id: Option<&str>,
    server_name: String,
    tool_name: String,
    arguments: Value,
) -> String {
    let nested_payload = json!({
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
            return json!({ "ok": false, "error": e }).to_string();
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
                return json!({ "ok": false, "error": e }).to_string();
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
        .unwrap_or_else(|| json!({}));

    match crate::command::mcp::call_mcp_tool(app.clone(), server, tool, args).await {
        Ok(v) => v.to_string(),
        Err(e) => json!({ "ok": false, "error": e }).to_string(),
    }
}
