use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::llm::commands::types::{
    ConversationHandover, ConversationMemory, HistoryMessage,
};

pub fn derive_title_from_message(content: &str) -> String {
    let first_line = content.lines().next().unwrap_or("").trim();
    let source = if first_line.is_empty() { content.trim() } else { first_line };
    let max_chars = 24usize;
    let mut out = String::new();
    for ch in source.chars().take(max_chars) {
        out.push(ch);
    }
    if source.chars().count() > max_chars {
        format!("{}...", out)
    } else if out.is_empty() {
        "New chat".to_string()
    } else {
        out
    }
}

fn normalize_inline(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn build_memory_from_history(
    messages: &[HistoryMessage],
    updated_at: i64,
) -> Option<ConversationMemory> {
    if messages.is_empty() {
        return None;
    }

    let summary_parts = messages
        .iter()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|m| {
            let speaker = if m.role.eq_ignore_ascii_case("user") {
                "User"
            } else {
                "Nova"
            };
            let content = normalize_inline(&m.content);
            format!("{}: {}", speaker, content.chars().take(120).collect::<String>())
        })
        .collect::<Vec<_>>();

    let summary = summary_parts.join(" | ").chars().take(800).collect::<String>();
    if summary.trim().is_empty() {
        return None;
    }

    let mut key_facts = Vec::new();
    for msg in messages.iter().rev().take(12).rev() {
        for line in msg.content.split('\n') {
            let normalized = normalize_inline(line);
            if normalized.len() < 12 || normalized.len() > 120 {
                continue;
            }
            if key_facts
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&normalized))
            {
                continue;
            }
            key_facts.push(normalized);
            if key_facts.len() >= 8 {
                break;
            }
        }
        if key_facts.len() >= 8 {
            break;
        }
    }

    Some(ConversationMemory {
        summary,
        key_facts,
        updated_at,
    })
}

pub async fn refresh_conversation_memory(
    pool: &SqlitePool,
    conversation_id: &str,
    updated_at: i64,
) -> Result<(), String> {
    let rows = sqlx::query(
        "SELECT role, content, token_usage, cost_json FROM conversation_messages WHERE conversation_id = ? ORDER BY created_at DESC, id DESC LIMIT 24",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut messages = rows
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

    messages.reverse();

    if let Some(memory) = build_memory_from_history(&messages, updated_at) {
        let key_facts_json = serde_json::to_string(&memory.key_facts).map_err(|e| e.to_string())?;
        sqlx::query(
            r#"
            INSERT INTO conversation_memory (conversation_id, summary, key_facts_json, updated_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(conversation_id)
            DO UPDATE SET summary=excluded.summary, key_facts_json=excluded.key_facts_json, updated_at=excluded.updated_at
            "#,
        )
        .bind(conversation_id)
        .bind(memory.summary)
        .bind(key_facts_json)
        .bind(memory.updated_at)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub async fn get_conversation_memory_by_pool(
    pool: &SqlitePool,
    conversation_id: &str,
) -> Result<Option<ConversationMemory>, String> {
    let row = sqlx::query(
        "SELECT summary, key_facts_json, updated_at FROM conversation_memory WHERE conversation_id = ?",
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

    Ok(Some(ConversationMemory {
        summary: row.get::<String, _>("summary"),
        key_facts,
        updated_at: row.get::<i64, _>("updated_at"),
    }))
}

pub async fn get_conversation_handover_by_pool(
    pool: &SqlitePool,
    conversation_id: &str,
    recent_limit: Option<i64>,
) -> Result<ConversationHandover, String> {
    let limit = recent_limit.unwrap_or(12).clamp(1, 50);

    let meta_row = sqlx::query("SELECT title, updated_at FROM conversations WHERE id = ?")
        .bind(conversation_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Conversation '{}' not found", conversation_id))?;

    let total_message_count: i64 =
        sqlx::query_scalar("SELECT COUNT(1) FROM conversation_messages WHERE conversation_id = ?")
            .bind(conversation_id)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;

    let rows = sqlx::query(
        "SELECT role, content, token_usage, cost_json FROM conversation_messages WHERE conversation_id = ? ORDER BY created_at DESC, id DESC LIMIT ?",
    )
    .bind(conversation_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut recent_messages = rows
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
    recent_messages.reverse();

    let updated_at = meta_row.get::<i64, _>("updated_at");
    let memory = match get_conversation_memory_by_pool(pool, conversation_id).await? {
        Some(memory) => memory,
        None => build_memory_from_history(&recent_messages, updated_at).unwrap_or(ConversationMemory {
            summary: String::new(),
            key_facts: Vec::new(),
            updated_at,
        }),
    };

    Ok(ConversationHandover {
        conversation_id: conversation_id.to_string(),
        title: meta_row.get::<String, _>("title"),
        summary: memory.summary,
        key_facts: memory.key_facts,
        recent_messages,
        omitted_message_count: (total_message_count - limit).max(0),
        total_message_count,
        updated_at,
    })
}

pub async fn upsert_conversation_memory_by_pool(
    pool: &SqlitePool,
    conversation_id: &str,
    summary: &str,
    key_facts: &[String],
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    let key_facts_json = serde_json::to_string(key_facts).map_err(|e| e.to_string())?;

    sqlx::query(
        r#"
        INSERT INTO conversation_memory (conversation_id, summary, key_facts_json, updated_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(conversation_id)
        DO UPDATE SET summary=excluded.summary, key_facts_json=excluded.key_facts_json, updated_at=excluded.updated_at
        "#,
    )
    .bind(conversation_id)
    .bind(summary)
    .bind(key_facts_json)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}