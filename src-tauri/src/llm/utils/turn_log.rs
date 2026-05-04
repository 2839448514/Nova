use std::fs::{self, OpenOptions};
use std::io::Write as IoWrite;

use tauri::{AppHandle, Manager};

/// 设为 false 可全局关闭 turn_log 日志输出。
const TURN_LOG_ENABLED: bool = true;

fn log_path(app: &AppHandle, conversation_id: Option<&str>) -> Option<std::path::PathBuf> {
	let base = app.path().app_data_dir().ok()?;
	let dir = base.join("turn_logs");
	fs::create_dir_all(&dir).ok()?;
	let filename = conversation_id
		.map(|id| {
			let safe: String = id
				.chars()
				.map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
				.collect();
			format!("{}.jsonl", safe)
		})
		.unwrap_or_else(|| "default.jsonl".to_string());
	Some(dir.join(filename))
}

fn append_to_log(app: &AppHandle, conversation_id: Option<&str>, text: &str) {
	if !TURN_LOG_ENABLED {
		return;
	}
	let Some(path) = log_path(app, conversation_id) else {
		return;
	};
	let mut file = match OpenOptions::new().create(true).append(true).open(&path) {
		Ok(f) => f,
		Err(e) => {
			eprintln!("[turn_log] 无法打开日志文件 {:?}: {}", path, e);
			return;
		}
	};
	if let Err(e) = writeln!(file, "{}", text) {
		eprintln!("[turn_log] 写入日志失败: {}", e);
	}
}

pub fn log_wire_request(
	app: &AppHandle,
	conversation_id: Option<&str>,
	url: &str,
	body: &str,
) {
	let entry = serde_json::json!({
		"type": "wire_request",
		"ts": chrono::Local::now().to_rfc3339(),
		"url": url,
		"body": serde_json::from_str::<serde_json::Value>(body).unwrap_or(serde_json::Value::String(body.to_string())),
	});
	append_to_log(app, conversation_id, &entry.to_string());
}

pub fn log_wire_response(
	app: &AppHandle,
	conversation_id: Option<&str>,
	data: &str,
	input_tokens: Option<u32>,
	output_tokens: Option<u32>,
) {
	let entry = serde_json::json!({
		"type": "wire_response",
		"ts": chrono::Local::now().to_rfc3339(),
		"input_tokens": input_tokens,
		"output_tokens": output_tokens,
		"data": serde_json::from_str::<serde_json::Value>(data).unwrap_or(serde_json::Value::String(data.to_string())),
	});
	append_to_log(app, conversation_id, &entry.to_string());
}
