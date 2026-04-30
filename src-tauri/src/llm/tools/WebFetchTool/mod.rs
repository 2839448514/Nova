use crate::llm::tools::{sync_tool, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};
#[cfg(not(target_os = "windows"))]
use std::process::Command;

// 返回 web_fetch 的注册信息。
// `read_only=true`，因为它只抓网页内容，不会修改本地状态。
pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, true, None)
}

// 返回模型可见的 web_fetch 元数据。
// 模型需要提供 `url`，工具再去抓取该地址的正文内容。
pub fn tool() -> Tool {
    Tool {
        name: "web_fetch".into(),
        description: "Fetch the main textual content of a web page URL.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "HTTP/HTTPS URL to fetch" }
            },
            "required": ["url"]
        }),
    }
}

// 把抓到的正文按字节裁剪到安全长度。
// `max_bytes` 限制返回大小，`boundary` 用来退到合法 UTF-8 边界，避免截断多字节字符。
fn truncate(s: String, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s;
    }

    let mut boundary = max_bytes;
    while !s.is_char_boundary(boundary) {
        boundary -= 1;
    }

    format!("{}\n...(truncated)", &s[..boundary])
}

// 抓取 input 里的 `url`，并把网页正文返回给模型。
// `url` 是要访问的 HTTP/HTTPS 地址，`out` 是底层命令执行后的原始结果。
pub fn execute(input: Value) -> String {
    let url = match input.get("url").and_then(|v| v.as_str()) {
        Some(v) if v.starts_with("http://") || v.starts_with("https://") => v,
        _ => return "Error: Missing or invalid 'url' argument".into(),
    };

    #[cfg(target_os = "windows")]
    let out = crate::llm::tools::process::run_hidden_pwsh(&format!(
        "(Invoke-WebRequest -UseBasicParsing -Uri '{}' -TimeoutSec 20).Content",
        url.replace('\'', "''")
    ));

    #[cfg(not(target_os = "windows"))]
    let out = Command::new("curl")
        .args(["-L", "--max-time", "20", url])
        .output();

    match out {
        Ok(output) => {
            // stdout: 实际抓到的网页内容；stderr: 抓取命令输出的错误信息。
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if output.status.success() {
                if stdout.trim().is_empty() {
                    "(fetched successfully but content is empty)".into()
                } else {
                    truncate(stdout, 12000)
                }
            } else {
                format!("Error fetching url: {}", stderr)
            }
        }
        Err(e) => format!("Failed to fetch url: {}", e),
    }
}
