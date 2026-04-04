use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const RAG_STORE_VERSION: u32 = 1;
const MAX_DOCUMENT_CHARS: usize = 200_000;
const MAX_BATCH_SIZE: usize = 200;

fn default_store_version() -> u32 {
    RAG_STORE_VERSION
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RagDocumentInput {
    pub source_name: String,
    pub source_type: String,
    pub mime_type: Option<String>,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RagDocument {
    pub id: String,
    pub source_name: String,
    pub source_type: String,
    pub mime_type: Option<String>,
    pub content: String,
    pub content_chars: usize,
    pub checksum: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RagStore {
    #[serde(default = "default_store_version")]
    pub version: u32,
    #[serde(default)]
    pub documents: Vec<RagDocument>,
}

impl Default for RagStore {
    fn default() -> Self {
        Self {
            version: RAG_STORE_VERSION,
            documents: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RagDocumentMeta {
    pub id: String,
    pub source_name: String,
    pub source_type: String,
    pub mime_type: Option<String>,
    pub content_chars: usize,
    pub preview: String,
    pub checksum: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RagStats {
    pub document_count: usize,
    pub total_chars: usize,
    pub last_updated_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RagRejectedItem {
    pub source_name: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RagUpsertResult {
    pub added: u32,
    pub updated: u32,
    pub rejected: Vec<RagRejectedItem>,
    pub total_documents: usize,
    pub total_chars: usize,
}

fn rag_store_path(app: &AppHandle) -> PathBuf {
    // RAG 数据文件优先使用 app_data_dir，失败时回退到当前目录下的 .nova。
    match app.path().app_data_dir() {
        Ok(dir) => dir.join("rag").join("documents.json"),
        Err(_) => std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".nova")
            .join("rag")
            .join("documents.json"),
    }
}

fn load_store(app: &AppHandle) -> Result<RagStore, String> {
    let path = rag_store_path(app);
    if !path.exists() {
        return Ok(RagStore::default());
    }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(RagStore::default());
    }

    let mut store = serde_json::from_str::<RagStore>(&content)
        .map_err(|e| format!("Failed to parse RAG store: {}", e))?;
    if store.version == 0 {
        store.version = RAG_STORE_VERSION;
    }
    Ok(store)
}

fn save_store(app: &AppHandle, store: &RagStore) -> Result<(), String> {
    let path = rag_store_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let content = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}

fn normalize_content(raw: &str) -> String {
    raw.replace("\r\n", "\n").trim().to_string()
}

fn normalize_source_type(raw: &str) -> String {
    let key = raw.trim().to_ascii_lowercase();
    if key.is_empty() {
        "text".to_string()
    } else {
        key
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn normalize_source_name(raw: &str, fallback_index: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        format!("document-{}", fallback_index + 1)
    } else {
        trimmed.to_string()
    }
}

fn preview_text(content: &str) -> String {
    let compact = content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let mut chars = compact.chars();
    let preview: String = chars.by_ref().take(160).collect();
    if chars.next().is_some() {
        format!("{}...", preview)
    } else {
        preview
    }
}

fn fnv1a_64_hex(input: &str) -> String {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let mut hash = OFFSET_BASIS;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{:016x}", hash)
}

fn calculate_stats(docs: &[RagDocument]) -> RagStats {
    let total_chars = docs.iter().map(|d| d.content_chars).sum::<usize>();
    let last_updated_at = docs.iter().map(|d| d.updated_at).max();
    RagStats {
        document_count: docs.len(),
        total_chars,
        last_updated_at,
    }
}

#[tauri::command]
pub fn rag_get_stats(app: AppHandle) -> Result<RagStats, String> {
    let store = load_store(&app)?;
    Ok(calculate_stats(&store.documents))
}

#[tauri::command]
pub fn rag_list_documents(app: AppHandle) -> Result<Vec<RagDocumentMeta>, String> {
    let store = load_store(&app)?;
    let mut items = store
        .documents
        .into_iter()
        .map(|doc| RagDocumentMeta {
            id: doc.id,
            source_name: doc.source_name,
            source_type: doc.source_type,
            mime_type: doc.mime_type,
            content_chars: doc.content_chars,
            preview: preview_text(&doc.content),
            checksum: doc.checksum,
            created_at: doc.created_at,
            updated_at: doc.updated_at,
        })
        .collect::<Vec<_>>();

    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(items)
}

#[tauri::command]
pub fn rag_upsert_documents(
    app: AppHandle,
    documents: Vec<RagDocumentInput>,
) -> Result<RagUpsertResult, String> {
    if documents.is_empty() {
        return Err("No documents provided".to_string());
    }
    if documents.len() > MAX_BATCH_SIZE {
        return Err(format!(
            "Batch size exceeded: max {} documents per request",
            MAX_BATCH_SIZE
        ));
    }

    let mut store = load_store(&app)?;
    let now = Utc::now().timestamp();

    let mut added = 0u32;
    let mut updated = 0u32;
    let mut rejected: Vec<RagRejectedItem> = Vec::new();

    for (index, item) in documents.into_iter().enumerate() {
        let source_name = normalize_source_name(&item.source_name, index);
        let content = normalize_content(&item.content);

        if content.is_empty() {
            rejected.push(RagRejectedItem {
                source_name,
                reason: "内容为空".to_string(),
            });
            continue;
        }

        let content_chars = content.chars().count();
        if content_chars > MAX_DOCUMENT_CHARS {
            rejected.push(RagRejectedItem {
                source_name,
                reason: format!("内容过长，最大允许 {} 字符", MAX_DOCUMENT_CHARS),
            });
            continue;
        }

        let checksum = fnv1a_64_hex(&content);
        let source_type = normalize_source_type(&item.source_type);
        let mime_type = normalize_optional_string(item.mime_type);

        if let Some(existing) = store.documents.iter_mut().find(|d| d.checksum == checksum) {
            existing.source_name = source_name;
            existing.source_type = source_type;
            existing.mime_type = mime_type;
            existing.content = content;
            existing.content_chars = content_chars;
            existing.updated_at = now;
            updated += 1;
            continue;
        }

        store.documents.push(RagDocument {
            id: Uuid::new_v4().to_string(),
            source_name,
            source_type,
            mime_type,
            content,
            content_chars,
            checksum,
            created_at: now,
            updated_at: now,
        });
        added += 1;
    }

    if added > 0 || updated > 0 {
        save_store(&app, &store)?;
    }

    let stats = calculate_stats(&store.documents);
    Ok(RagUpsertResult {
        added,
        updated,
        rejected,
        total_documents: stats.document_count,
        total_chars: stats.total_chars,
    })
}

#[tauri::command]
pub fn rag_remove_document(app: AppHandle, document_id: String) -> Result<bool, String> {
    let id = document_id.trim();
    if id.is_empty() {
        return Err("document_id is required".to_string());
    }

    let mut store = load_store(&app)?;
    let before = store.documents.len();
    store.documents.retain(|doc| doc.id != id);
    let removed = before != store.documents.len();

    if removed {
        save_store(&app, &store)?;
    }

    Ok(removed)
}

#[tauri::command]
pub fn rag_clear_documents(app: AppHandle) -> Result<(), String> {
    let mut store = load_store(&app)?;
    if store.documents.is_empty() {
        return Ok(());
    }

    store.documents.clear();
    save_store(&app, &store)
}
