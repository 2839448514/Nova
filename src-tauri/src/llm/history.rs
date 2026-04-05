use serde_json::Value;
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::llm::commands::memory;
use crate::llm::commands::types::{ConversationMeta, HistoryMessage};

// Build sqlite database URL under app data directory.
// Format: sqlite:<path>?mode=rwc (read/write/create).
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

// Create a sqlx sqlite pool for history DB.
async fn get_pool(app: &AppHandle) -> Result<SqlitePool, String> {
    let db_url = get_db_url(app)?;
    SqlitePool::connect(&db_url)
        .await
        .map_err(|e| e.to_string())
}

// Ensure required schema exists.
// This includes base tables and lightweight backward-compatible column migrations.
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
            attachments_json TEXT,
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

    // Backward-compatible migration: add token_usage for older DB files.
    // Ignore "duplicate column" so startup remains idempotent.
    let alter_result = sqlx::query("ALTER TABLE conversation_messages ADD COLUMN token_usage INTEGER")
        .execute(pool)
        .await;

    if let Err(e) = alter_result {
        let msg = e.to_string().to_lowercase();
        if !msg.contains("duplicate column") {
            return Err(e.to_string());
        }
    }

    // Backward-compatible migration: add attachments_json for older DB files.
    // Ignore duplicate-column errors for repeated startup runs.
    let alter_attachments_result =
        sqlx::query("ALTER TABLE conversation_messages ADD COLUMN attachments_json TEXT")
            .execute(pool)
            .await;

    if let Err(e) = alter_attachments_result {
        let msg = e.to_string().to_lowercase();
        if !msg.contains("duplicate column") {
            return Err(e.to_string());
        }
    }

    // Backward-compatible migration: add cost_json for older DB files.
    // Ignore duplicate-column errors for repeated startup runs.
    let alter_cost_result = sqlx::query("ALTER TABLE conversation_messages ADD COLUMN cost_json TEXT")
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

// Public helper used by command handlers:
// open DB pool and guarantee schema is ready before query.
pub async fn get_pool_with_schema(app: &AppHandle) -> Result<SqlitePool, String> {
    let pool = get_pool(app).await?;
    ensure_schema(&pool).await?;
    Ok(pool)
}

// Create a new conversation row with generated UUID and optional title.
// If title is empty, fallback to default "New chat".
pub async fn create_conversation(
    app: &AppHandle,
    title: Option<String>,
) -> Result<ConversationMeta, String> {
    let pool = get_pool_with_schema(app).await?;

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let conv_title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "New chat".to_string());

    sqlx::query("INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
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

// List conversations ordered by latest update time.
pub async fn list_conversations(app: &AppHandle) -> Result<Vec<ConversationMeta>, String> {
    let pool = get_pool_with_schema(app).await?;

    let rows = sqlx::query("SELECT id, title, updated_at FROM conversations ORDER BY updated_at DESC")
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

// Load all persisted messages for a conversation in stable chronological order.
// cost_json is parsed into JSON when possible; malformed JSON is safely ignored.
pub async fn load_history(
    app: &AppHandle,
    conversation_id: &str,
) -> Result<Vec<HistoryMessage>, String> {
    let pool = get_pool_with_schema(app).await?;

    let rows = sqlx::query(
        "SELECT role, content, attachments_json, token_usage, cost_json FROM conversation_messages WHERE conversation_id = ? ORDER BY created_at ASC, id ASC",
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
            attachments: row
                .get::<Option<String>, _>("attachments_json")
                .and_then(|s| serde_json::from_str(&s).ok()),
            token_usage: row.get::<Option<i64>, _>("token_usage"),
            cost: row
                .get::<Option<String>, _>("cost_json")
                .and_then(|s| serde_json::from_str::<Value>(&s).ok()),
        })
        .collect();

    Ok(result)
}

// Append one message to conversation history and maintain related metadata:
// 1) insert message row
// 2) auto-derive title on first user message when title is still default
// 3) update conversation updated_at
// 4) refresh conversation memory summary/facts
pub async fn append_history(
    app: &AppHandle,
    conversation_id: &str,
    message: HistoryMessage,
) -> Result<(), String> {
    let pool = get_pool_with_schema(app).await?;

    let now = chrono::Utc::now().timestamp();
    let role = message.role.clone();
    let content = message.content.clone();
    let attachments_json = message
        .attachments
        .and_then(|v| serde_json::to_string(&v).ok());
    let token_usage = message.token_usage;
    let cost_json = message.cost.and_then(|v| serde_json::to_string(&v).ok());

    sqlx::query(
        "INSERT INTO conversation_messages (conversation_id, role, content, attachments_json, token_usage, cost_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(conversation_id)
    .bind(&role)
    .bind(&content)
    .bind(attachments_json)
    .bind(token_usage)
    .bind(cost_json)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    // Auto-title only when first user message arrives and current title is default/empty.
    if role.eq_ignore_ascii_case("user") {
        let user_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM conversation_messages WHERE conversation_id = ? AND role = 'user'",
        )
        .bind(conversation_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| e.to_string())?;

        if user_count == 1 {
            let current_title: Option<String> =
                sqlx::query_scalar("SELECT title FROM conversations WHERE id = ?")
                    .bind(conversation_id)
                    .fetch_optional(&pool)
                    .await
                    .map_err(|e| e.to_string())?;

            let should_update = matches!(
                current_title.as_deref(),
                Some("New chat") | Some("新会话") | Some("") | None
            );

            if should_update {
                let new_title = memory::derive_title_from_message(&content);
                sqlx::query("UPDATE conversations SET title = ? WHERE id = ?")
                    .bind(new_title)
                    .bind(conversation_id)
                    .execute(&pool)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    // Touch conversation timestamp so list order reflects latest activity.
    sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    // Keep summary memory in sync after each append.
    memory::refresh_conversation_memory(&pool, conversation_id, now).await?;

    Ok(())
}

// Clear history data.
// - with conversation_id: clear only that conversation's messages
// - without conversation_id: clear all messages and all conversation rows
pub async fn clear_history(app: &AppHandle, conversation_id: Option<String>) -> Result<(), String> {
    let pool = get_pool_with_schema(app).await?;

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

// Delete one conversation and all dependent rows.
// Order matters to satisfy FK constraints in environments that enforce them.
pub async fn delete_conversation(app: &AppHandle, conversation_id: &str) -> Result<(), String> {
    let pool = get_pool_with_schema(app).await?;

    sqlx::query("DELETE FROM conversation_messages WHERE conversation_id = ?")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query("DELETE FROM conversation_memory WHERE conversation_id = ?")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query("DELETE FROM conversation_compact_boundaries WHERE conversation_id = ?")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    crate::command::rag::rag_remove_conversation_documents(app, conversation_id)?;

    Ok(())
}
