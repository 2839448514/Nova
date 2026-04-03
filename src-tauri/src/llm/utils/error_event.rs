use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BackendErrorEvent {
    pub source: String,
    pub message: String,
    pub stage: Option<String>,
}

pub fn emit_backend_error(
    app: &AppHandle,
    source: &str,
    message: impl Into<String>,
    stage: Option<&str>,
) {
    let payload = BackendErrorEvent {
        source: source.to_string(),
        message: message.into(),
        stage: stage.map(|s| s.to_string()),
    };

    let _ = app.emit("backend-error", payload.clone());
    eprintln!(
        "[backend-error] source={} stage={} message={}",
        payload.source,
        payload.stage.as_deref().unwrap_or("-"),
        payload.message
    );
}
