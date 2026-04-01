pub mod execute_bash;
pub mod write_file;
pub mod grep_search;
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

// Reserved migration targets (folder placeholders kept in tools/):
// - AgentTool
// - AskUserQuestionTool (migrated)
// - ConfigTool (migrated)
// - FileEditTool (migrated)
// - FileReadTool (migrated)
// Add them here after each module is implemented.

use crate::llm::types::Tool;
use serde_json::Value;

pub fn get_available_tools() -> Vec<Tool> {
    vec![
        execute_bash::tool(),
        powershell_tool::tool(),
        file_read_tool::tool(),
        write_file::tool(),
        file_edit_tool::tool(),
        grep_search::tool(),
        glob_tool::tool(),
        web_fetch_tool::tool(),
        web_search_tool::tool(),
        task_create_tool::tool(),
        task_list_tool::tool(),
        task_update_tool::tool(),
        todo_write_tool::tool(),
        tool_search_tool::tool(),
        ask_user_question_tool::tool(),
        config_tool::tool(),
    ]
}

pub fn execute_tool(name: &str, input: Value) -> String {
    match name {
        "execute_bash" => execute_bash::execute(input),
        "execute_powershell" => powershell_tool::execute(input),
        "read_file" => file_read_tool::execute(input),
        "write_file" => write_file::execute(input),
        "replace_string_in_file" => file_edit_tool::execute(input),
        "grep_search" => grep_search::execute(input),
        "glob_search" => glob_tool::execute(input),
        "web_fetch" => web_fetch_tool::execute(input),
        "web_search" => web_search_tool::execute(input),
        "task_create" => task_create_tool::execute(input),
        "task_list" => task_list_tool::execute(input),
        "task_update" => task_update_tool::execute(input),
        "todo_write" => todo_write_tool::execute(input),
        "tool_search" => tool_search_tool::execute(input),
        "mcp_tool" => mcp_tool::execute(input),
        "list_mcp_resources" => list_mcp_resources_tool::execute(input),
        "read_mcp_resource" => read_mcp_resource_tool::execute(input),
        "mcp_auth" => mcp_auth_tool::execute(input),
        "lsp_tool" => lsp_tool::execute(input),
        "ask_user_question" => ask_user_question_tool::execute(input),
        "config_tool" => config_tool::execute(input),
        _ => format!("Unknown tool: {}", name)
    }
}
