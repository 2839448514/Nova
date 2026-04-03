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

// 检查一轮消息里是否已经包含过会话恢复标记，避免重复叠加恢复上下文。
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

// 入口函数：发送用户聊天消息，驱动 LLM 请求和工具调用流程。
// 这个函数负责准备消息、循环调度 provider、处理 tool-result 掉回、最后发送 stop 事件。
pub async fn send_chat_message(
    app: AppHandle,
    conversation_id: Option<String>,
    messages: Vec<Message>,
    plan_mode: bool,
) -> Result<(), String> {
    // 1. 预处理消息：把用户本轮输入和历史消息压缩为本次模型请求的 current_messages。
    let mut current_messages =
        compact::prepare_messages_for_turn(&app, conversation_id.as_deref(), &messages).await;

    // 2. 如果有会话 ID，尝试插入会话恢复上下文（仅当当前内容里未标记时）。
    //    这块会返回类似: "[Session Restore Context] ..." 的 system/user 信息。
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

    // 3. 根据设置选择模型提供方（Anthropic/OpenAI）。
    let provider = LlmProvider::new(&app);

    // 4. 主循环：调用 provider.send_request（流式），并根据 tool 执行情况决定是否继续下一步。
    //    - 如果发生工具调用，结果会被“注入”到 current_messages 继续下一轮。
    //    - 如果 provider 返回 needs_user_input / 无工具结果，则结束。
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
                // 出错直接通知前端 stop(error) 并返回错误。
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

        // 本轮 provider 输出合并到 current_messages 以支持工具环回。
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

        // 若返回需要用户输入，终止当前回合并告诉前端。
        if compact::has_needs_user_input(&new_messages) {
            break (
                "needs_user_input".to_string(),
                "needs_user_input".to_string(),
            );
        }

        // 若本轮没有工具结果，说明回合结束。
        if !has_tool_result {
            break (
                provider_result
                    .stop_reason
                    .unwrap_or_else(|| "end_turn".to_string()),
                "completed".to_string(),
            );
        }
    };

    // 5. 业务终止：告知前端本轮结束，并携带 stop_reason/turn_state 以区分 completed/needs_user_input/error。
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
