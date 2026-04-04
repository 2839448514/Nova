use tauri::AppHandle;

use crate::llm::types::Message;

pub use crate::llm::query_engine::ChatMessageEvent;

#[tauri::command]
pub async fn send_chat_message(
    app: AppHandle,
    conversation_id: Option<String>,
    messages: Vec<Message>,
    plan_mode: Option<bool>,
) -> Result<(), String> {
    let conversation_scope = conversation_id.clone();
    crate::llm::cancellation::begin_turn(conversation_scope.as_deref());

    let result = crate::llm::query_engine::send_chat_message(
        app,
        conversation_id,
        messages,
        plan_mode.unwrap_or(false),
    )
    .await;

    crate::llm::cancellation::finish_turn(conversation_scope.as_deref());
    result
}

#[tauri::command]
pub async fn cancel_chat_message(conversation_id: Option<String>) -> Result<bool, String> {
    Ok(crate::llm::cancellation::request_cancel(
        conversation_id.as_deref(),
    ))
}
