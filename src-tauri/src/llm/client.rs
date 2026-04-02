use tauri::AppHandle;

use crate::llm::types::Message;

pub use crate::llm::query_engine::{send_request, ChatMessageEvent};

#[tauri::command]
pub async fn send_chat_message(
    app: AppHandle,
    conversation_id: Option<String>,
    messages: Vec<Message>,
) -> Result<(), String> {
    crate::llm::query_engine::send_chat_message(app, conversation_id, messages).await
}
