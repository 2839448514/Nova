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

// Placeholder migration modules stay out of `registered_tools()` until their
// runtime bridge is complete. This avoids exposing Claude-style folders as if
// they were fully migrated Nova tools.

use crate::llm::types::Tool;
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
            tool: task_list_tool::tool,
            execute: task_list_tool::execute,
        },
        RegisteredTool {
            tool: task_update_tool::tool,
            execute: task_update_tool::execute,
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
            tool: ask_user_question_tool::tool,
            execute: ask_user_question_tool::execute,
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
