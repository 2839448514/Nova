use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfileMeta {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub updated_at: i64,
    pub path: String,
}

fn agents_root_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app_data_dir for agents: {}", e))?;
    Ok(app_data_dir.join("agents"))
}

fn ensure_agents_root_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let root = agents_root_dir(app)?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root)
}

fn system_time_to_unix_secs(value: Option<SystemTime>) -> i64 {
    value
        .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn normalize_profile_file_name(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("profile_id is required".to_string());
    }

    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err("profile_id must not contain path separators".to_string());
    }

    let file_name = if trimmed.to_ascii_lowercase().ends_with(".md") {
        trimmed.to_string()
    } else {
        format!("{}.md", trimmed)
    };

    let path = Path::new(&file_name);
    if path.components().count() != 1 {
        return Err("profile_id is invalid".to_string());
    }

    Ok(file_name)
}

fn sanitize_agent_name(raw: &str) -> String {
    let trimmed = raw.trim();
    let fallback = "agent";
    if trimmed.is_empty() {
        return fallback.to_string();
    }

    let mut result = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        let invalid = matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control();
        if invalid {
            result.push('_');
        } else {
            result.push(ch);
        }
    }

    let cleaned = result.trim_matches(['.', ' ']).trim();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.to_string()
    }
}

fn file_stem_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|v| v.to_str())
        .map(|v| v.to_string())
        .unwrap_or_else(|| "agent".to_string())
}

fn build_profile_meta(path: &Path) -> Result<AgentProfileMeta, String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("profile path is not a file".to_string());
    }

    let file_name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| "invalid profile file name".to_string())?
        .to_string();

    Ok(AgentProfileMeta {
        id: file_name.clone(),
        name: file_stem_name(path),
        file_name,
        updated_at: system_time_to_unix_secs(metadata.modified().ok()),
        path: path.to_string_lossy().to_string(),
    })
}

fn list_agent_profiles_internal(app: &AppHandle) -> Result<Vec<AgentProfileMeta>, String> {
    let root = ensure_agents_root_dir(app)?;
    let mut items: Vec<AgentProfileMeta> = Vec::new();

    for entry_result in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
        let entry = entry_result.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let is_markdown = path
            .extension()
            .and_then(|v| v.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("md"))
            .unwrap_or(false);
        if !is_markdown {
            continue;
        }

        let meta = build_profile_meta(&path)?;
        items.push(meta);
    }

    items.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.file_name.cmp(&b.file_name))
    });

    Ok(items)
}

fn unique_profile_path(root: &Path, base_name: &str) -> PathBuf {
    let mut attempt = 0usize;
    loop {
        let candidate_name = if attempt == 0 {
            base_name.to_string()
        } else {
            format!("{}-{}", base_name, attempt + 1)
        };

        let file_name = format!("{}.md", candidate_name);
        let path = root.join(file_name);
        if !path.exists() {
            return path;
        }

        attempt += 1;
    }
}

fn resolve_profile_path(app: &AppHandle, profile_id: &str) -> Result<PathBuf, String> {
    let root = ensure_agents_root_dir(app)?;
    let file_name = normalize_profile_file_name(profile_id)?;
    Ok(root.join(file_name))
}

fn resolve_default_profile_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = ensure_agents_root_dir(app)?;
    Ok(root.join("default.md"))
}

#[tauri::command]
pub fn list_agent_profiles(app: AppHandle) -> Result<Vec<AgentProfileMeta>, String> {
    list_agent_profiles_internal(&app)
}

#[tauri::command]
pub fn create_agent_profile(
    app: AppHandle,
    name: Option<String>,
) -> Result<AgentProfileMeta, String> {
    let root = ensure_agents_root_dir(&app)?;
    let now = system_time_to_unix_secs(Some(SystemTime::now()));
    let default_name = format!("agent-{}", now);
    let requested_name = name.unwrap_or(default_name);
    let safe_name = sanitize_agent_name(&requested_name);
    let profile_path = unique_profile_path(&root, &safe_name);

    let title = requested_name.trim();
    let initial_title = if title.is_empty() {
        safe_name.as_str()
    } else {
        title
    };
    let initial_content = format!("# {}\n\n", initial_title);

    std::fs::write(&profile_path, initial_content).map_err(|e| e.to_string())?;
    build_profile_meta(&profile_path)
}

#[tauri::command]
pub fn delete_agent_profile(app: AppHandle, profile_id: String) -> Result<(), String> {
    let path = resolve_profile_path(&app, &profile_id)?;
    if !path.exists() {
        return Err(format!("Agent profile not found: {}", profile_id));
    }

    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("profile path is not a file".to_string());
    }

    std::fs::remove_file(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_agent_profile_markdown(
    app: AppHandle,
    profile_id: String,
) -> Result<String, String> {
    let path = resolve_profile_path(&app, &profile_id)?;
    if !path.exists() {
        return Err(format!("Agent profile not found: {}", profile_id));
    }

    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_agent_profile_markdown(
    app: AppHandle,
    profile_id: String,
    content: String,
) -> Result<(), String> {
    let path = resolve_profile_path(&app, &profile_id)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    std::fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_agent_markdown_path(app: AppHandle) -> Result<String, String> {
    let path = resolve_default_profile_path(&app)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn load_agent_markdown(app: AppHandle) -> Result<String, String> {
    let path = resolve_default_profile_path(&app)?;
    if !path.exists() {
        return Err("Agent profile not found: default.md".to_string());
    }

    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_agent_markdown(app: AppHandle, content: String) -> Result<(), String> {
    let path = resolve_default_profile_path(&app)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    std::fs::write(path, content).map_err(|e| e.to_string())
}
