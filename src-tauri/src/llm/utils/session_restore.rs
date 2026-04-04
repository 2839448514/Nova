use tauri::AppHandle;

use crate::llm::types::{Content, Message, Role};

fn truncate_chars(input: &str, max_chars: usize) -> String {
    // input: 要截断的原始字符串。
    // max_chars: 最大字符数限制。
    let mut out = String::new();
    // out: 用于累积截断后的字符。
    for ch in input.chars().take(max_chars) {
        // ch: 当前迭代到的字符。
        out.push(ch);
    }
    // 返回截断后的字符串。
    out
}

pub async fn build_resume_context_message(
    app: &AppHandle,
    conversation_id: &str,
) -> Option<Message> {
    // conversation_id 为空或仅有空白时不生成恢复上下文。
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
    // app.clone(): 克隆 AppHandle 以便传入异步上下文。
    // conversation_id.trim().to_string(): 去掉空白后转换为 String。
    // .await: 等待异步命令完成。
    // .ok(): 将 Result 转换为 Option，错误时返回 None。
    // .flatten()?: 展开嵌套 Option<...>，如果内部 None 则返回 None。
    // resume: 从历史命令中获取的会话恢复上下文。

    let summary = truncate_chars(&resume.boundary.summary, 480);
    // summary: 对话摘要文本，最多 480 个字符。

    let key_facts = resume
        .boundary
        .key_facts
        .iter()
        .take(6)
        .map(|s| format!("- {}", truncate_chars(s, 120)))
        .collect::<Vec<_>>()
        .join("\n");
    // resume.boundary.key_facts.iter(): 遍历关键事实列表。
    // take(6): 只处理前 6 条。
    // map(|s| ...): 每条 key fact 前加 "- " 并截断。
    // collect::<Vec<_>>(): 收集为字符串向量。
    // join("\n"): 以换行符连接为单个字符串。
    // key_facts: 最多 6 条关键事实文本。

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
            // speaker: 如果 role 等于 "user"（不区分大小写），则显示 "User"，否则显示 "Nova"。
            format!("{}: {}", speaker, truncate_chars(m.content.trim(), 140))
        })
        .collect::<Vec<_>>()
        .join("\n");
    // messages_since_boundary.iter(): 遍历 boundary 之后的消息列表。
    // rev(): 将迭代顺序反转，取最近的消息。
    // take(4): 只保留最近 4 条消息。
    // collect::<Vec<_>>(): 将取到的消息先收集到 Vec 中。
    // into_iter().rev(): 再次反转，恢复消息的原始时间顺序。
    // map(|m| ...): 为每条消息生成 "说话者: 内容" 文本。
    // collect::<Vec<_>>().join("\n"): 连接为多行字符串。
    // recent: 最近 4 条消息摘要。

    let mut content_parts = Vec::new();
    // content_parts: 构建最终 Message 内容的段落数组。
    // 固定头部标记，供上游识别恢复上下文。
    content_parts.push("[Session Restore Context]".to_string());
    // 插入摘要段。
    content_parts.push(format!("Summary: {}", summary));
    if !key_facts.is_empty() {
        // 关键事实标题段。
        content_parts.push("Key facts:".to_string());
        // 关键事实正文段。
        content_parts.push(key_facts);
    }
    if !recent.is_empty() {
        // 最近消息标题段。
        content_parts.push("Recent messages since compact boundary:".to_string());
        // 最近消息正文段。
        content_parts.push(recent);
    }

    Some(Message {
        role: Role::User,
        // role: 将恢复上下文作为用户消息发送给模型。
        content: Content::Text(content_parts.join("\n")),
        // content_parts.join("\n"): 把各段落按换行符连接成全文本。
    })
}
