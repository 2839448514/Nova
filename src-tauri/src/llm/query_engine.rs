use serde::Serialize;
use tauri::AppHandle;

use crate::llm::services::compact;
use crate::llm::types::{Content, ContentBlock, Message};
use crate::llm::providers::LlmProvider;

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

pub async fn send_chat_message(
    app: AppHandle,
    conversation_id: Option<String>,
    messages: Vec<Message>,
    plan_mode: bool,
) -> Result<(), String> {
    let mut current_messages =
        compact::prepare_messages_for_turn(&app, conversation_id.as_deref(), &messages).await;

    let provider = LlmProvider::new(&app);

    loop {
        let new_messages = provider.send_request(&app, &current_messages, plan_mode).await?;
        current_messages.extend(new_messages.clone());

        eprintln!("[loop] new_messages count={}", new_messages.len());

        let has_tool_result = new_messages.iter().any(|m| {
            if let Content::Blocks(blocks) = &m.content {
                blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }))
            } else {
                false
            }
        });

        eprintln!("[loop] has_tool_result={}", has_tool_result);

        if compact::has_needs_user_input(&new_messages) {
            break;
        }

        if !has_tool_result {
            break;
        }
    }

    Ok(())
}
