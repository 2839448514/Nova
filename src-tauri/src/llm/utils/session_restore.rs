use tauri::AppHandle;

use crate::llm::types::{Content, Message, Role};

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in input.chars().take(max_chars) {
        out.push(ch);
    }
    out
}

pub async fn build_resume_context_message(
    app: &AppHandle,
    conversation_id: &str,
) -> Option<Message> {
    if conversation_id.trim().is_empty() {
        return None;
    }

    let resume = crate::command::history::get_conversation_resume_context(
        app.clone(),
        conversation_id.trim().to_string(),
    )
    .await
    .ok()
    .flatten()?;

    let summary = truncate_chars(&resume.boundary.summary, 480);
    let key_facts = resume
        .boundary
        .key_facts
        .iter()
        .take(6)
        .map(|s| format!("- {}", truncate_chars(s, 120)))
        .collect::<Vec<_>>()
        .join("\n");

    let recent = resume
        .messages_since_boundary
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|m| {
            let speaker = if m.role.eq_ignore_ascii_case("user") {
                "User"
            } else {
                "Nova"
            };
            format!("{}: {}", speaker, truncate_chars(m.content.trim(), 140))
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut content_parts = Vec::new();
    content_parts.push("[Session Restore Context]".to_string());
    content_parts.push(format!("Summary: {}", summary));
    if !key_facts.is_empty() {
        content_parts.push("Key facts:".to_string());
        content_parts.push(key_facts);
    }
    if !recent.is_empty() {
        content_parts.push("Recent messages since compact boundary:".to_string());
        content_parts.push(recent);
    }

    Some(Message {
        role: Role::User,
        content: Content::Text(content_parts.join("\n")),
    })
}
