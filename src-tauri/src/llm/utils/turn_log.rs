use std::fs::{self, OpenOptions};
use std::io::Write as IoWrite;

use tauri::{AppHandle, Manager};

use crate::llm::types::Message;

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

pub fn log_request(
	app: &AppHandle,
	conversation_id: Option<&str>,
	system: Option<&str>,
	messages: &[Message],
) {
	let entry = serde_json::json!({
		"type": "request",
		"ts": chrono::Local::now().to_rfc3339(),
		"system": system,
		"messages": messages,
	});
	append_to_log(app, conversation_id, &entry.to_string());
}

pub fn log_response(
	app: &AppHandle,
	conversation_id: Option<&str>,
	messages: &[Message],
	input_tokens: Option<u32>,
	output_tokens: Option<u32>,
) {
	let entry = serde_json::json!({
		"type": "response",
		"ts": chrono::Local::now().to_rfc3339(),
		"input_tokens": input_tokens,
		"output_tokens": output_tokens,
		"messages": messages,
	});
	append_to_log(app, conversation_id, &entry.to_string());
}
