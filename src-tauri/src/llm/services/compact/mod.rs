use tauri::AppHandle;

use crate::llm::types::{Content, ContentBlock, Message, Role};

fn estimate_message_tokens(messages: &[Message]) -> i64 {
    messages
        .iter()
        .map(|m| match &m.content {
            Content::Text(text) => text.trim().chars().count() as i64,
            Content::Blocks(blocks) => blocks
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => text.trim().chars().count() as i64,
                    ContentBlock::ToolUse { input, .. } => input.to_string().chars().count() as i64,
                    ContentBlock::ToolResult { content, .. } => content
                        .iter()
                        .map(|inner| match inner {
                            ContentBlock::Text { text } => text.trim().chars().count() as i64,
                            _ => 0,
                        })
                        .sum::<i64>(),
                })
                .sum::<i64>(),
        })
        .sum::<i64>()
        / 4
}

pub async fn prepare_messages_for_turn(
    app: &AppHandle,
    conversation_id: Option<&str>,
    messages: &[Message],
) -> Vec<Message> {
    let Some(conversation_id) = conversation_id.filter(|id| !id.trim().is_empty()) else {
        return messages.to_vec();
    };

    let estimated_tokens = estimate_message_tokens(messages);
    let should_compact = messages.len() > 16 || estimated_tokens > 1800;
    if !should_compact {
        return messages.to_vec();
    }

    let compact = match crate::command::history::get_conversation_compact_context(
        app.clone(),
        conversation_id.to_string(),
        Some(1600),
        Some(8),
    )
    .await
    {
        Ok(v) => v,
        Err(_) => return messages.to_vec(),
    };

    let handover = crate::command::history::get_conversation_handover(
        app.clone(),
        conversation_id.to_string(),
        Some(compact.recent_limit),
    )
    .await
    .ok();

    if let Some(handover) = handover {
        let _ = crate::command::history::record_compact_boundary(
            app.clone(),
            &compact,
            &handover.summary,
            &handover.key_facts,
        )
        .await;
    }

    let keep_count = compact.recent_limit.clamp(4, 24) as usize;
    let recent_messages = if messages.len() > keep_count {
        messages[messages.len() - keep_count..].to_vec()
    } else {
        messages.to_vec()
    };

    let compact_message = Message {
        role: Role::User,
        content: Content::Text(compact.context_text),
    };

    let mut prepared = Vec::with_capacity(recent_messages.len() + 1);
    prepared.push(compact_message);
    prepared.extend(recent_messages);
    prepared
}

pub fn has_needs_user_input(messages: &[Message]) -> bool {
    messages.iter().any(|m| {
        let Content::Blocks(blocks) = &m.content else {
            return false;
        };

        blocks.iter().any(|b| {
            let ContentBlock::ToolResult { content, .. } = b else {
                return false;
            };

            content.iter().any(|inner| {
                let ContentBlock::Text { text } = inner else {
                    return false;
                };

                serde_json::from_str::<serde_json::Value>(text)
                    .ok()
                    .and_then(|v| {
                        v.get("type")
                            .and_then(|t| t.as_str())
                            .map(|s| s == "needs_user_input")
                    })
                    .unwrap_or(false)
            })
        })
    })
}
