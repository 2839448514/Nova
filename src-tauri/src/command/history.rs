use serde::{Deserialize, Serialize};
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
        "SELECT role, content, token_usage FROM conversation_messages WHERE conversation_id = ? ORDER BY created_at ASC, id ASC",
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

    sqlx::query(
        "INSERT INTO conversation_messages (conversation_id, role, content, token_usage, created_at) VALUES (?, ?, ?, ?, ?)",
    )
        .bind(&conversation_id)
        .bind(&role)
        .bind(&content)
        .bind(token_usage)
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
        .bind(conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

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

    sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
