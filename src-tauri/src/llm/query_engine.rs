use serde::Serialize;
use tauri::AppHandle;

use crate::llm::types::Message;

#[derive(Debug, Serialize, Clone)]
pub struct ChatMessageEvent {
    pub r#type: String,
    pub text: Option<String>,
    pub tool_use_id: Option<String>,
    pub tool_use_name: Option<String>,
    pub tool_use_input: Option<String>,
    pub tool_result: Option<String>,
    pub token_usage: Option<u32>,
    pub stop_reason: Option<String>,
    pub turn_state: Option<String>,
}

// 兼容入口：保留原函数名，内部委托给 query 模块实现。
pub async fn send_chat_message(
    app: AppHandle,
    conversation_id: Option<String>,
    messages: Vec<Message>,
    plan_mode: bool,
) -> Result<(), String> {
    crate::llm::query::send_chat_message(app, conversation_id, messages, plan_mode).await
}
