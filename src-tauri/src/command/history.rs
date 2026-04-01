use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

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

fn derive_title_from_message(content: &str) -> String {
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

fn estimate_tokens(text: &str) -> i64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        0
    } else {
        ((trimmed.chars().count() as i64) + 3) / 4
    }
}

fn build_memory_from_history(messages: &[HistoryMessage], updated_at: i64) -> Option<ConversationMemory> {
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
            if key_facts.iter().any(|existing: &String| existing.eq_ignore_ascii_case(&normalized)) {
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

async fn refresh_conversation_memory(
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

fn get_db_url(app: &AppHandle) -> Result<String, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("history.db");

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    Ok(format!("sqlite:{}?mode=rwc", db_path.display()))
}

async fn get_pool(app: &AppHandle) -> Result<SqlitePool, String> {
    let db_url = get_db_url(app)?;
    SqlitePool::connect(&db_url)
        .await
        .map_err(|e| e.to_string())
}

async fn ensure_schema(pool: &SqlitePool) -> Result<(), String> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS conversation_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            token_usage INTEGER,
            cost_json TEXT,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (conversation_id) REFERENCES conversations(id)
        );

        CREATE TABLE IF NOT EXISTS conversation_memory (
            conversation_id TEXT PRIMARY KEY,
            summary TEXT NOT NULL,
            key_facts_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (conversation_id) REFERENCES conversations(id)
        );

        CREATE TABLE IF NOT EXISTS conversation_compact_boundaries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL,
            context_text TEXT NOT NULL,
            summary TEXT NOT NULL,
            key_facts_json TEXT NOT NULL,
            recent_limit INTEGER NOT NULL,
            omitted_message_count INTEGER NOT NULL,
            total_message_count INTEGER NOT NULL,
            estimated_tokens INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (conversation_id) REFERENCES conversations(id)
        );
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Backward-compatible migration for older databases.
    let alter_result = sqlx::query(
        "ALTER TABLE conversation_messages ADD COLUMN token_usage INTEGER",
    )
    .execute(pool)
    .await;

    if let Err(e) = alter_result {
        let msg = e.to_string().to_lowercase();
        if !msg.contains("duplicate column") {
            return Err(e.to_string());
        }
    }

    let alter_cost_result = sqlx::query(
        "ALTER TABLE conversation_messages ADD COLUMN cost_json TEXT",
    )
    .execute(pool)
    .await;

    if let Err(e) = alter_cost_result {
        let msg = e.to_string().to_lowercase();
        if !msg.contains("duplicate column") {
            return Err(e.to_string());
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn create_conversation(
    app: AppHandle,
    title: Option<String>,
) -> Result<ConversationMeta, String> {
    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let conv_title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "New chat".to_string());

    sqlx::query(
        "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&conv_title)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(ConversationMeta {
        id,
        title: conv_title,
        updated_at: now,
    })
}

#[tauri::command]
pub async fn list_conversations(app: AppHandle) -> Result<Vec<ConversationMeta>, String> {
    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

    let rows = sqlx::query(
        "SELECT id, title, updated_at FROM conversations ORDER BY updated_at DESC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let items = rows
        .into_iter()
        .map(|row| ConversationMeta {
            id: row.get::<String, _>("id"),
            title: row.get::<String, _>("title"),
            updated_at: row.get::<i64, _>("updated_at"),
        })
        .collect();

    Ok(items)
}

#[tauri::command]
pub async fn load_history(
    app: AppHandle,
    conversation_id: String,
) -> Result<Vec<HistoryMessage>, String> {
    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

    let rows = sqlx::query(
        "SELECT role, content, token_usage, cost_json FROM conversation_messages WHERE conversation_id = ? ORDER BY created_at ASC, id ASC",
    )
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let result = rows
        .into_iter()
        .map(|row| HistoryMessage {
            role: row.get::<String, _>("role"),
            content: row.get::<String, _>("content"),
            token_usage: row.get::<Option<i64>, _>("token_usage"),
            cost: row
                .get::<Option<String>, _>("cost_json")
                .and_then(|s| serde_json::from_str::<Value>(&s).ok()),
        })
        .collect();

    Ok(result)
}

#[tauri::command]
pub async fn append_history(
    app: AppHandle,
    conversation_id: String,
    message: HistoryMessage,
) -> Result<(), String> {
    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

    let now = chrono::Utc::now().timestamp();
    let role = message.role.clone();
    let content = message.content.clone();
    let token_usage = message.token_usage;
    let cost_json = message.cost.and_then(|v| serde_json::to_string(&v).ok());

    sqlx::query(
        "INSERT INTO conversation_messages (conversation_id, role, content, token_usage, cost_json, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
        .bind(&conversation_id)
        .bind(&role)
        .bind(&content)
        .bind(token_usage)
        .bind(cost_json)
        .bind(now)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    if role.eq_ignore_ascii_case("user") {
        let user_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM conversation_messages WHERE conversation_id = ? AND role = 'user'",
        )
        .bind(&conversation_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| e.to_string())?;

        if user_count == 1 {
            let current_title: Option<String> = sqlx::query_scalar(
                "SELECT title FROM conversations WHERE id = ?",
            )
            .bind(&conversation_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| e.to_string())?;

            let should_update = matches!(
                current_title.as_deref(),
                Some("New chat") | Some("新会话") | Some("") | None
            );

            if should_update {
                let new_title = derive_title_from_message(&content);
                sqlx::query("UPDATE conversations SET title = ? WHERE id = ?")
                    .bind(new_title)
                    .bind(&conversation_id)
                    .execute(&pool)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    refresh_conversation_memory(&pool, &conversation_id, now).await?;

    Ok(())
}

#[tauri::command]
pub async fn clear_history(app: AppHandle, conversation_id: Option<String>) -> Result<(), String> {
    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

    if let Some(id) = conversation_id {
        sqlx::query("DELETE FROM conversation_messages WHERE conversation_id = ?")
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        sqlx::query("DELETE FROM conversation_messages")
            .execute(&pool)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM conversations")
            .execute(&pool)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn delete_conversation(app: AppHandle, conversation_id: String) -> Result<(), String> {
    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

    sqlx::query("DELETE FROM conversation_messages WHERE conversation_id = ?")
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query("DELETE FROM conversation_memory WHERE conversation_id = ?")
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query("DELETE FROM conversation_compact_boundaries WHERE conversation_id = ?")
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_conversation_memory(
    app: AppHandle,
    conversation_id: String,
) -> Result<Option<ConversationMemory>, String> {
    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

    let row = sqlx::query(
        "SELECT summary, key_facts_json, updated_at FROM conversation_memory WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_optional(&pool)
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

#[tauri::command]
pub async fn get_conversation_handover(
    app: AppHandle,
    conversation_id: String,
    recent_limit: Option<i64>,
) -> Result<ConversationHandover, String> {
    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

    let limit = recent_limit.unwrap_or(12).clamp(1, 50);

    let meta_row = sqlx::query(
        "SELECT title, updated_at FROM conversations WHERE id = ?",
    )
    .bind(&conversation_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("Conversation '{}' not found", conversation_id))?;

    let total_message_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(1) FROM conversation_messages WHERE conversation_id = ?",
    )
    .bind(&conversation_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let rows = sqlx::query(
        "SELECT role, content, token_usage, cost_json FROM conversation_messages WHERE conversation_id = ? ORDER BY created_at DESC, id DESC LIMIT ?",
    )
    .bind(&conversation_id)
    .bind(limit)
    .fetch_all(&pool)
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
    let memory = match get_conversation_memory(app.clone(), conversation_id.clone()).await? {
        Some(memory) => memory,
        None => build_memory_from_history(&recent_messages, updated_at).unwrap_or(ConversationMemory {
            summary: String::new(),
            key_facts: Vec::new(),
            updated_at,
        }),
    };

    Ok(ConversationHandover {
        conversation_id,
        title: meta_row.get::<String, _>("title"),
        summary: memory.summary,
        key_facts: memory.key_facts,
        recent_messages,
        omitted_message_count: (total_message_count - limit).max(0),
        total_message_count,
        updated_at,
    })
}

#[tauri::command]
pub async fn get_conversation_compact_context(
    app: AppHandle,
    conversation_id: String,
    token_budget: Option<i64>,
    recent_limit: Option<i64>,
) -> Result<CompactContext, String> {
    let handover = get_conversation_handover(app, conversation_id.clone(), recent_limit).await?;
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
        let truncated = context_text.chars().take((token_budget * 4) as usize).collect::<String>();
        truncated
    } else {
        context_text
    };

    Ok(CompactContext {
        conversation_id,
        context_text: final_text.clone(),
        recent_limit,
        omitted_message_count: handover.omitted_message_count,
        total_message_count: handover.total_message_count,
        estimated_tokens: estimate_tokens(&final_text),
        updated_at: handover.updated_at,
    })
}

pub async fn record_compact_boundary(
    app: AppHandle,
    compact: &CompactContext,
    summary: &str,
    key_facts: &[String],
) -> Result<CompactBoundary, String> {
    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

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
    .execute(&pool)
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

#[tauri::command]
pub async fn get_latest_compact_boundary(
    app: AppHandle,
    conversation_id: String,
) -> Result<Option<CompactBoundary>, String> {
    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

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
    .bind(&conversation_id)
    .fetch_optional(&pool)
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

#[tauri::command]
pub async fn get_conversation_resume_context(
    app: AppHandle,
    conversation_id: String,
) -> Result<Option<ResumeContext>, String> {
    let boundary = match get_latest_compact_boundary(app.clone(), conversation_id.clone()).await? {
        Some(v) => v,
        None => return Ok(None),
    };

    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

    let rows = sqlx::query(
        r#"
        SELECT role, content, token_usage, cost_json
        FROM conversation_messages
        WHERE conversation_id = ? AND created_at >= ?
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(&conversation_id)
    .bind(boundary.created_at)
    .fetch_all(&pool)
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

    Ok(Some(ResumeContext {
        boundary,
        messages_since_boundary,
    }))
}

#[tauri::command]
pub async fn upsert_conversation_memory(
    app: AppHandle,
    conversation_id: String,
    summary: String,
    key_facts: Vec<String>,
) -> Result<(), String> {
    let pool = get_pool(&app).await?;
    ensure_schema(&pool).await?;

    let now = chrono::Utc::now().timestamp();
    let key_facts_json = serde_json::to_string(&key_facts).map_err(|e| e.to_string())?;

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
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}
