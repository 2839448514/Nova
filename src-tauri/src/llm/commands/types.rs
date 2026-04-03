use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
    pub token_usage: Option<i64>,
    pub cost: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMemory {
    pub summary: String,
    pub key_facts: Vec<String>,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConversationHandover {
    pub conversation_id: String,
    pub title: String,
    pub summary: String,
    pub key_facts: Vec<String>,
    pub recent_messages: Vec<HistoryMessage>,
    pub omitted_message_count: i64,
    pub total_message_count: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompactContext {
    pub conversation_id: String,
    pub context_text: String,
    pub recent_limit: i64,
    pub omitted_message_count: i64,
    pub total_message_count: i64,
    pub estimated_tokens: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompactBoundary {
    pub id: i64,
    pub conversation_id: String,
    pub context_text: String,
    pub summary: String,
    pub key_facts: Vec<String>,
    pub recent_limit: i64,
    pub omitted_message_count: i64,
    pub total_message_count: i64,
    pub estimated_tokens: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResumeContext {
    pub boundary: CompactBoundary,
    pub messages_since_boundary: Vec<HistoryMessage>,
}
