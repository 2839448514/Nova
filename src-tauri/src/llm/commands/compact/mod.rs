use sqlx::{Row, SqlitePool};

use crate::llm::commands::types::{CompactBoundary, CompactContext, ConversationHandover};

pub fn estimate_tokens(text: &str) -> i64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        0
    } else {
        ((trimmed.chars().count() as i64) + 3) / 4
    }
}

pub fn build_compact_context(
    conversation_id: String,
    handover: ConversationHandover,
    token_budget: Option<i64>,
    recent_limit: Option<i64>,
) -> CompactContext {
    let recent_limit = recent_limit.unwrap_or(8).clamp(4, 24);
    let token_budget = token_budget.unwrap_or(1600).clamp(400, 6000);

    let recent_section = handover
        .recent_messages
        .iter()
        .rev()
        .take(recent_limit as usize)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|m| {
            let speaker = if m.role.eq_ignore_ascii_case("user") {
                "User"
            } else {
                "Nova"
            };
            format!("{}: {}", speaker, m.content.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let facts = if handover.key_facts.is_empty() {
        String::new()
    } else {
        handover
            .key_facts
            .iter()
            .map(|fact| format!("- {}", fact))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let mut context_text = format!(
        "[Compact context]\nConversation: {}\nSummary: {}\n{}{}",
        handover.title,
        handover.summary,
        if facts.is_empty() { "" } else { "Key facts:\n" },
        facts
    );

    if !recent_section.trim().is_empty() {
        context_text.push_str("\n\nRecent messages:\n");
        context_text.push_str(&recent_section);
    }

    let estimated_tokens = estimate_tokens(&context_text);
    let final_text = if estimated_tokens > token_budget {
        context_text
            .chars()
            .take((token_budget * 4) as usize)
            .collect::<String>()
    } else {
        context_text
    };

    CompactContext {
        conversation_id,
        context_text: final_text.clone(),
        recent_limit,
        omitted_message_count: handover.omitted_message_count,
        total_message_count: handover.total_message_count,
        estimated_tokens: estimate_tokens(&final_text),
        updated_at: handover.updated_at,
    }
}

pub async fn record_compact_boundary_by_pool(
    pool: &SqlitePool,
    compact: &CompactContext,
    summary: &str,
    key_facts: &[String],
) -> Result<CompactBoundary, String> {
    let created_at = chrono::Utc::now().timestamp();
    let key_facts_json = serde_json::to_string(key_facts).map_err(|e| e.to_string())?;

    let result = sqlx::query(
        r#"
        INSERT INTO conversation_compact_boundaries (
            conversation_id, context_text, summary, key_facts_json, recent_limit,
            omitted_message_count, total_message_count, estimated_tokens, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&compact.conversation_id)
    .bind(&compact.context_text)
    .bind(summary)
    .bind(&key_facts_json)
    .bind(compact.recent_limit)
    .bind(compact.omitted_message_count)
    .bind(compact.total_message_count)
    .bind(compact.estimated_tokens)
    .bind(created_at)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(CompactBoundary {
        id: result.last_insert_rowid(),
        conversation_id: compact.conversation_id.clone(),
        context_text: compact.context_text.clone(),
        summary: summary.to_string(),
        key_facts: key_facts.to_vec(),
        recent_limit: compact.recent_limit,
        omitted_message_count: compact.omitted_message_count,
        total_message_count: compact.total_message_count,
        estimated_tokens: compact.estimated_tokens,
        created_at,
    })
}

pub async fn get_latest_compact_boundary_by_pool(
    pool: &SqlitePool,
    conversation_id: &str,
) -> Result<Option<CompactBoundary>, String> {
    let row = sqlx::query(
        r#"
        SELECT id, conversation_id, context_text, summary, key_facts_json,
               recent_limit, omitted_message_count, total_message_count,
               estimated_tokens, created_at
        FROM conversation_compact_boundaries
        WHERE conversation_id = ?
        ORDER BY created_at DESC, id DESC
        LIMIT 1
        "#,
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let Some(row) = row else {
        return Ok(None);
    };

    let key_facts_raw = row.get::<String, _>("key_facts_json");
    let key_facts = serde_json::from_str::<Vec<String>>(&key_facts_raw).unwrap_or_default();

    Ok(Some(CompactBoundary {
        id: row.get::<i64, _>("id"),
        conversation_id: row.get::<String, _>("conversation_id"),
        context_text: row.get::<String, _>("context_text"),
        summary: row.get::<String, _>("summary"),
        key_facts,
        recent_limit: row.get::<i64, _>("recent_limit"),
        omitted_message_count: row.get::<i64, _>("omitted_message_count"),
        total_message_count: row.get::<i64, _>("total_message_count"),
        estimated_tokens: row.get::<i64, _>("estimated_tokens"),
        created_at: row.get::<i64, _>("created_at"),
    }))
}