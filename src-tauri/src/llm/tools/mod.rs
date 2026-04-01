pub mod execute_bash;
pub mod write_file;
pub mod grep_search;
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
        file_read_tool::tool(),
        write_file::tool(),
        file_edit_tool::tool(),
        grep_search::tool(),
        ask_user_question_tool::tool(),
        config_tool::tool(),
    ]
}

pub fn execute_tool(name: &str, input: Value) -> String {
    match name {
        "execute_bash" => execute_bash::execute(input),
        "read_file" => file_read_tool::execute(input),
        "write_file" => write_file::execute(input),
        "replace_string_in_file" => file_edit_tool::execute(input),
        "grep_search" => grep_search::execute(input),
        "ask_user_question" => ask_user_question_tool::execute(input),
        "config_tool" => config_tool::execute(input),
        _ => format!("Unknown tool: {}", name)
    }
}
