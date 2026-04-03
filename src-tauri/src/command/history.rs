use tauri::AppHandle;

use crate::llm::commands::{compact, memory, resume};
use crate::llm::history;
pub use crate::llm::commands::types::{
    CompactBoundary, CompactContext, ConversationHandover, ConversationMemory, ConversationMeta,
    HistoryMessage, ResumeContext,
};

#[tauri::command]
pub async fn create_conversation(
    app: AppHandle,
    title: Option<String>,
) -> Result<ConversationMeta, String> {
    history::create_conversation(&app, title).await
}

#[tauri::command]
pub async fn list_conversations(app: AppHandle) -> Result<Vec<ConversationMeta>, String> {
    history::list_conversations(&app).await
}

#[tauri::command]
pub async fn load_history(
    app: AppHandle,
    conversation_id: String,
) -> Result<Vec<HistoryMessage>, String> {
    history::load_history(&app, &conversation_id).await
}

#[tauri::command]
pub async fn append_history(
    app: AppHandle,
    conversation_id: String,
    message: HistoryMessage,
) -> Result<(), String> {
    history::append_history(&app, &conversation_id, message).await
}

#[tauri::command]
pub async fn clear_history(app: AppHandle, conversation_id: Option<String>) -> Result<(), String> {
    history::clear_history(&app, conversation_id).await
}

#[tauri::command]
pub async fn delete_conversation(app: AppHandle, conversation_id: String) -> Result<(), String> {
    history::delete_conversation(&app, &conversation_id).await
}

#[tauri::command]
pub async fn get_conversation_memory(
    app: AppHandle,
    conversation_id: String,
) -> Result<Option<ConversationMemory>, String> {
    let pool = history::get_pool_with_schema(&app).await?;
    memory::get_conversation_memory_by_pool(&pool, &conversation_id).await
}

#[tauri::command]
pub async fn get_conversation_handover(
    app: AppHandle,
    conversation_id: String,
    recent_limit: Option<i64>,
) -> Result<ConversationHandover, String> {
    let pool = history::get_pool_with_schema(&app).await?;
    memory::get_conversation_handover_by_pool(&pool, &conversation_id, recent_limit).await
}

#[tauri::command]
pub async fn get_conversation_compact_context(
    app: AppHandle,
    conversation_id: String,
    token_budget: Option<i64>,
    recent_limit: Option<i64>,
) -> Result<CompactContext, String> {
    let pool = history::get_pool_with_schema(&app).await?;
    let handover = memory::get_conversation_handover_by_pool(&pool, &conversation_id, recent_limit).await?;
    Ok(compact::build_compact_context(
        conversation_id,
        handover,
        token_budget,
        recent_limit,
    ))
}

pub async fn record_compact_boundary(
    app: AppHandle,
    compact_ctx: &CompactContext,
    summary: &str,
    key_facts: &[String],
) -> Result<CompactBoundary, String> {
    let pool = history::get_pool_with_schema(&app).await?;
    compact::record_compact_boundary_by_pool(&pool, compact_ctx, summary, key_facts).await
}

#[tauri::command]
pub async fn get_latest_compact_boundary(
    app: AppHandle,
    conversation_id: String,
) -> Result<Option<CompactBoundary>, String> {
    let pool = history::get_pool_with_schema(&app).await?;
    compact::get_latest_compact_boundary_by_pool(&pool, &conversation_id).await
}

#[tauri::command]
pub async fn get_conversation_resume_context(
    app: AppHandle,
    conversation_id: String,
) -> Result<Option<ResumeContext>, String> {
    let pool = history::get_pool_with_schema(&app).await?;
    let boundary = match compact::get_latest_compact_boundary_by_pool(&pool, &conversation_id).await? {
        Some(v) => v,
        None => return Ok(None),
    };
    let ctx = resume::get_conversation_resume_context_by_pool(&pool, &conversation_id, boundary).await?;
    Ok(Some(ctx))
}

#[tauri::command]
pub async fn upsert_conversation_memory(
    app: AppHandle,
    conversation_id: String,
    summary: String,
    key_facts: Vec<String>,
) -> Result<(), String> {
    let pool = history::get_pool_with_schema(&app).await?;
    memory::upsert_conversation_memory_by_pool(&pool, &conversation_id, &summary, &key_facts).await
}
