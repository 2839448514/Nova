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
use tauri::AppHandle;
use serde_json::Value;

struct RegisteredTool {
    tool: fn() -> Tool,
    execute: fn(Value) -> String,
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

pub fn get_available_tools() -> Vec<Tool> {
    registered_tools()
        .into_iter()
        .map(|entry| (entry.tool)())
        .collect()
}

pub fn execute_tool(name: &str, input: Value) -> String {
    for entry in registered_tools() {
        let tool = (entry.tool)();
        if tool.name == name {
            return (entry.execute)(input);
        }
    }

    format!("Unknown tool: {}", name)
}

pub async fn execute_tool_with_app(app: &AppHandle, name: &str, input: Value) -> String {
    match crate::llm::utils::permissions::enforce_tool_permission(app, name, &input) {
        crate::llm::utils::permissions::PermissionEnforcement::Allow => {}
        crate::llm::utils::permissions::PermissionEnforcement::Deny(e) => {
            return serde_json::json!({ "ok": false, "error": e }).to_string();
        }
        crate::llm::utils::permissions::PermissionEnforcement::AskUser(payload) => {
            return payload;
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
        _ => execute_tool(name, input),
    }
}
