use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::llm::commands::types::{CompactBoundary, HistoryMessage, ResumeContext};

pub async fn get_conversation_resume_context_by_pool(
    pool: &SqlitePool,
    conversation_id: &str,
    boundary: CompactBoundary,
) -> Result<ResumeContext, String> {
    let rows = sqlx::query(
        r#"
        SELECT role, content, token_usage, cost_json
        FROM conversation_messages
        WHERE conversation_id = ? AND created_at >= ?
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(conversation_id)
    .bind(boundary.created_at)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let messages_since_boundary = rows
        .into_iter()
        .map(|row| HistoryMessage {
            role: row.get::<String, _>("role"),
            content: row.get::<String, _>("content"),
            token_usage: row.get::<Option<i64>, _>("token_usage"),
            cost: row
                .get::<Option<String>, _>("cost_json")
                .and_then(|s| serde_json::from_str::<Value>(&s).ok()),
        })
        .collect::<Vec<_>>();

    Ok(ResumeContext {
        boundary,
        messages_since_boundary,
    })
}