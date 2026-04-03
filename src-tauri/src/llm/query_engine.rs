use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::llm::providers::LlmProvider;
use crate::llm::services::compact;
use crate::llm::types::{Content, ContentBlock, Message};
use crate::llm::utils::error_event::emit_backend_error;

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

fn has_session_restore_marker(messages: &[Message]) -> bool {
    messages.iter().any(|m| match &m.content {
        Content::Text(t) => t.contains("[Session Restore Context]"),
        Content::Blocks(blocks) => blocks.iter().any(|b| {
            if let ContentBlock::Text { text } = b {
                text.contains("[Session Restore Context]")
            } else {
                false
            }
        }),
    })
}

pub async fn send_chat_message(
    app: AppHandle,
    conversation_id: Option<String>,
    messages: Vec<Message>,
    plan_mode: bool,
) -> Result<(), String> {
    let mut current_messages =
        compact::prepare_messages_for_turn(&app, conversation_id.as_deref(), &messages).await;

    if let Some(conversation_id) = conversation_id.as_deref() {
        if !has_session_restore_marker(&current_messages) {
            if let Some(restore_msg) =
                crate::llm::utils::session_restore::build_resume_context_message(
                    &app,
                    conversation_id,
                )
                .await
            {
                current_messages.insert(0, restore_msg);
            }
        }
    }

    let provider = LlmProvider::new(&app);

    let (final_stop_reason, final_turn_state) = loop {
        let consumed =
            crate::llm::utils::permissions::consume_user_permission_decisions(
                conversation_id.as_deref(),
                &current_messages,
            );
        if consumed > 0 {
            eprintln!("[permissions] applied user approval decisions={}", consumed);
        }

        let provider_result = match provider
            .send_request(&app, &current_messages, plan_mode, conversation_id.as_deref())
            .await
        {
            Ok(v) => v,
            Err(e) => {
                emit_backend_error(
                    &app,
                    "llm.query_engine",
                    e.clone(),
                    Some("provider.send_request"),
                );
                app.emit(
                    "chat-stream",
                    ChatMessageEvent {
                        r#type: "stop".into(),
                        text: Some(e.clone()),
                        tool_use_id: None,
                        tool_use_name: None,
                        tool_use_input: None,
                        tool_result: None,
                        token_usage: None,
                        stop_reason: Some("provider_error".into()),
                        turn_state: Some("error".into()),
                    },
                )
                .ok();
                return Err(e);
            }
        };

        let new_messages = provider_result.messages;
        current_messages.extend(new_messages.clone());

        eprintln!("[loop] new_messages count={},the new messages are: {:?}", new_messages.len(), new_messages);

        let has_tool_result = new_messages.iter().any(|m| {
            if let Content::Blocks(blocks) = &m.content {
                blocks
                    .iter()
                    .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
            } else {
                false
            }
        });

        eprintln!("[loop] has_tool_result={}", has_tool_result);

        if compact::has_needs_user_input(&new_messages) {
            break (
                "needs_user_input".to_string(),
                "needs_user_input".to_string(),
            );
        }

        if !has_tool_result {
            break (
                provider_result
                    .stop_reason
                    .unwrap_or_else(|| "end_turn".to_string()),
                "completed".to_string(),
            );
        }
    };

    app.emit(
        "chat-stream",
        ChatMessageEvent {
            r#type: "stop".into(),
            text: None,
            tool_use_id: None,
            tool_use_name: None,
            tool_use_input: None,
            tool_result: None,
            token_usage: None,
            stop_reason: Some(final_stop_reason),
            turn_state: Some(final_turn_state),
        },
    )
    .ok();

    Ok(())
}
