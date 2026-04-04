use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BackendErrorEvent {
    // 错误来源模块标识。
    pub source: String,
    // 错误消息正文。
    pub message: String,
    // 可选阶段信息（如 provider.send_request）。
    pub stage: Option<String>,
}

pub fn emit_backend_error(
    app: &AppHandle,
    source: &str,
    message: impl Into<String>,
    stage: Option<&str>,
) {
    // 组装统一错误事件 payload。
    let payload = BackendErrorEvent {
        // source 转为拥有所有权字符串。
        source: source.to_string(),
        // message 统一转换为 String。
        message: message.into(),
        // stage 从 Option<&str> 映射为 Option<String>。
        stage: stage.map(|s| s.to_string()),
    };

    // 广播后端错误事件给前端；失败不阻断主流程。
    let _ = app.emit("backend-error", payload.clone());
    // 同步写 stderr 便于本地调试和日志采集。
    eprintln!(
        "[backend-error] source={} stage={} message={}",
        payload.source,
        // stage 为空时打印占位符 "-"。
        payload.stage.as_deref().unwrap_or("-"),
        payload.message
    );
}
