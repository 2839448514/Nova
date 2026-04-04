use crate::llm::types::{Content, ContentBlock, Message, Role};

pub(crate) fn context_message(prefix: &str, text: &str) -> Message {
    Message {
        role: Role::User,
        content: Content::Text(format!("{} {}", prefix, text.trim())),
    }
}

pub(crate) fn latest_assistant_text(messages: &[Message]) -> String {
    messages
        .iter()
        .rev()
        .find_map(|m| {
            if m.role != Role::Assistant {
                return None;
            }
            match &m.content {
                Content::Text(t) => Some(t.clone()),
                Content::Blocks(blocks) => Some(
                    blocks
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::Text { text } = b {
                                Some(text.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
            }
        })
        .unwrap_or_default()
}

pub(crate) fn has_exact_user_message(messages: &[Message], expected: &str) -> bool {
    messages.iter().any(|m| {
        if m.role != Role::User {
            return false;
        }
        matches!(&m.content, Content::Text(text) if text == expected)
    })
}
