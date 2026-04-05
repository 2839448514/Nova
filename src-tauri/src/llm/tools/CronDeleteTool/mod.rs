use crate::llm::tools::shared::cron_store::remove_job;
use crate::llm::types::Tool;
use serde_json::{json, Value};
use tauri::AppHandle;

pub fn tool() -> Tool {
    Tool {
        name: "CronDelete".into(),
        description: "Delete a scheduled cron job by id.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Job id returned by CronCreate" }
            },
            "required": ["id"]
        }),
    }
}

pub fn execute(_input: Value) -> String {
    json!({
        "ok": false,
        "message": "CronDelete requires AppHandle-aware execution and should be routed via execute_tool_with_app."
    })
    .to_string()
}

pub async fn execute_with_app(app: &AppHandle, input: Value) -> String {
    let id = match input.get("id").and_then(|v| v.as_str()).map(str::trim) {
        Some(v) if !v.is_empty() => v,
        _ => return json!({ "ok": false, "error": "CronDelete requires non-empty 'id'" }).to_string(),
    };

    match remove_job(app, id) {
        Ok(true) => json!({ "ok": true, "id": id }).to_string(),
        Ok(false) => json!({ "ok": false, "error": format!("No scheduled job with id '{}'", id) }).to_string(),
        Err(e) => json!({ "ok": false, "error": e }).to_string(),
    }
}
