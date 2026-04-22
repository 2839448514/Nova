use crate::llm::types::Tool;
use serde_json::{json, Value};
use std::fs;

/// Sentinel stored in the sidecar to indicate the file did not exist before writing.
const NOVA_NEW_FILE_MARKER: &str = "\x00NOVA_NEW_FILE\x00";

pub fn tool() -> Tool {
    Tool {
        name: "write_file".into(),
        description: "Write content to a file on the host machine. This completely overwrites the file.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute path to the file" },
                "content": { "type": "string", "description": "The content to write" }
            },
            "required": ["path", "content"]
        }),
    }
}

pub fn execute(input: Value) -> String {
    if let (Some(path), Some(content)) = (
        input.get("path").and_then(|v| v.as_str()),
        input.get("content").and_then(|v| v.as_str())
    ) {
        // Save a snapshot of the original file (for revert support).
        let sidecar = format!("{}.nova-snapshot", path);
        let original = match fs::read_to_string(path) {
            Ok(orig) => orig,
            Err(_) => NOVA_NEW_FILE_MARKER.to_string(),
        };
        let _ = fs::write(&sidecar, &original);

        match fs::write(path, content) {
            Ok(_) => "Successfully wrote to file".into(),
            Err(e) => format!("Error writing file: {}", e),
        }
    } else {
        "Error: Missing 'path' or 'content' argument".into()
    }
}

