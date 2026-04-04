use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMeta {
    // 会话唯一 ID。
    pub id: String,
    // 会话标题。
    pub title: String,
    // 最近更新时间（unix 秒）。
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HistoryMessage {
    // 消息角色（user/assistant）。
    pub role: String,
    // 消息文本内容。
    pub content: String,
    // 可选 token 使用量。
    pub token_usage: Option<i64>,
    // 可选成本结构（JSON）。
    pub cost: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMemory {
    // 会话摘要。
    pub summary: String,
    // 关键事实列表。
    pub key_facts: Vec<String>,
    // 记忆更新时间（unix 秒）。
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConversationHandover {
    // 会话 ID。
    pub conversation_id: String,
    // 会话标题。
    pub title: String,
    // 摘要文本。
    pub summary: String,
    // 关键事实。
    pub key_facts: Vec<String>,
    // 最近消息列表。
    pub recent_messages: Vec<HistoryMessage>,
    // 被省略消息数。
    pub omitted_message_count: i64,
    // 总消息数。
    pub total_message_count: i64,
    // 更新时间（unix 秒）。
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompactContext {
    // 会话 ID。
    pub conversation_id: String,
    // 压缩上下文文本。
    pub context_text: String,
    // 采用的 recent limit。
    pub recent_limit: i64,
    // 被省略消息数。
    pub omitted_message_count: i64,
    // 总消息数。
    pub total_message_count: i64,
    // 估算 token 数。
    pub estimated_tokens: i64,
    // 更新时间（unix 秒）。
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompactBoundary {
    // 边界记录 ID。
    pub id: i64,
    // 会话 ID。
    pub conversation_id: String,
    // 该次 compact 的文本上下文。
    pub context_text: String,
    // compact 摘要。
    pub summary: String,
    // compact 关键事实。
    pub key_facts: Vec<String>,
    // recent limit。
    pub recent_limit: i64,
    // 被省略消息数。
    pub omitted_message_count: i64,
    // 总消息数。
    pub total_message_count: i64,
    // 估算 token 数。
    pub estimated_tokens: i64,
    // 创建时间（unix 秒）。
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResumeContext {
    // 恢复基线边界。
    pub boundary: CompactBoundary,
    // 边界之后的消息列表。
    pub messages_since_boundary: Vec<HistoryMessage>,
}
