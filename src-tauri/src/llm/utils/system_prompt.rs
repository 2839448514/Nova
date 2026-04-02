use std::path::PathBuf;

use tauri::{AppHandle, Manager};

const SYSTEM_PROMPT_FILE_NAME: &str = "system_prompt.md";
const FALLBACK_SYSTEM_PROMPT: &str = "You are a helpful coding assistant running in a local Tauri desktop app. You will answer questions briefly and write accurate code.";

fn read_non_empty_file(path: &PathBuf) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

pub fn load_system_prompt(app: &AppHandle) -> String {
    if let Ok(cwd) = std::env::current_dir() {
        let path = cwd
            .join("src-tauri")
            .join("src")
            .join("prompt")
            .join(SYSTEM_PROMPT_FILE_NAME);
        if let Some(prompt) = read_non_empty_file(&path) {
            return prompt;
        }
    }

    FALLBACK_SYSTEM_PROMPT.to_string()
}
