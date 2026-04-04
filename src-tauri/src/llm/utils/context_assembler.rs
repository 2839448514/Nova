use tauri::AppHandle;

use crate::llm::types::{Content, ContentBlock, Message, Role};

const SESSION_RESTORE_MARKER: &str = "[Session Restore Context]";

#[derive(Debug, Clone, Copy)]
pub struct AssembleOptions {
    // 是否尝试插入会话恢复上下文。
    pub include_session_restore: bool,
    // 是否读取组装器自定义环境上下文（默认关闭）。
    pub include_env_contexts: bool,
}

impl Default for AssembleOptions {
    fn default() -> Self {
        Self {
            include_session_restore: true,
            include_env_contexts: false,
        }
    }
}

fn env_context_message() -> Option<Message> {
    let extra = std::env::var("NOVA_ASSEMBLER_CONTEXT").ok()?;
    let trimmed = extra.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(Message {
        role: Role::User,
        content: Content::Text(format!("[AssemblerContext] {}", trimmed)),
    })
}

// 检查消息序列里是否已经包含会话恢复标记，避免重复插入。
pub fn has_session_restore_marker(messages: &[Message]) -> bool {
    messages.iter().any(|m| match &m.content {
        Content::Text(t) => t.contains(SESSION_RESTORE_MARKER),
        Content::Blocks(blocks) => blocks.iter().any(|b| {
            if let ContentBlock::Text { text } = b {
                text.contains(SESSION_RESTORE_MARKER)
            } else {
                false
            }
        }),
    })
}

// 组装本轮上下文：
// 1) 以传入消息为基础
// 2) 视配置插入会话恢复消息（幂等）
// 3) 视配置附加组装器自定义上下文
pub async fn assemble_messages_for_turn(
    app: &AppHandle,
    conversation_id: Option<&str>,
    incoming: &[Message],
    options: AssembleOptions,
) -> Vec<Message> {
    let mut assembled = incoming.to_vec();

    if options.include_session_restore
        && !has_session_restore_marker(&assembled)
        && conversation_id.is_some()
    {
        if let Some(restore_msg) =
            crate::llm::utils::session_restore::build_resume_context_message(
                app,
                conversation_id.unwrap_or_default(),
            )
            .await
        {
            assembled.insert(0, restore_msg);
        }
    }

    if options.include_env_contexts {
        if let Some(msg) = env_context_message() {
            assembled.push(msg);
        }
    }

    assembled
}
