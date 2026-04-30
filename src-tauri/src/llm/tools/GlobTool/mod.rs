use crate::llm::tools::{sync_tool, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

// 返回 glob_search 的注册信息。
// 这个工具只扫描文件路径，不会写文件，所以标成只读。
pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, true, None)
}

// 返回模型可见的 glob_search 元数据。
// `root` 是搜索起点目录，`pattern` 是相对路径通配符，`max_results` 控制返回上限。
pub fn tool() -> Tool {
    Tool {
        name: "glob_search".into(),
        description: "Search files by wildcard pattern (supports * and ?).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "root": { "type": "string", "description": "Root directory to search" },
                "pattern": { "type": "string", "description": "Wildcard pattern against relative path" },
                "max_results": { "type": "integer", "description": "Maximum number of matches" }
            },
            "required": ["root", "pattern"]
        }),
    }
}

// 用简单的 `*`/`?` 规则判断文本是否命中通配符。
// `pattern` 是用户输入的通配表达式，`text` 是当前相对路径字符串。
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();

    let (mut i, mut j) = (0usize, 0usize);
    let (mut star, mut match_j) = (None, 0usize);

    while j < t.len() {
        if i < p.len() && (p[i] == b'?' || p[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == b'*' {
            star = Some(i);
            i += 1;
            match_j = j;
        } else if let Some(star_idx) = star {
            i = star_idx + 1;
            match_j += 1;
            j = match_j;
        } else {
            return false;
        }
    }

    while i < p.len() && p[i] == b'*' {
        i += 1;
    }

    i == p.len()
}

// 递归遍历目录树，把命中的文件绝对路径塞进 `out`。
// `root` 用来计算相对路径，`current` 是当前递归到的目录，`max` 用来限制结果数量。
fn walk(root: &Path, current: &Path, pattern: &str, out: &mut Vec<String>, max: usize) {
    if out.len() >= max {
        return;
    }

    let entries = match fs::read_dir(current) {
        Ok(v) => v,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if out.len() >= max {
            break;
        }

        let p = entry.path();
        if p.is_dir() {
            walk(root, &p, pattern, out, max);
            continue;
        }

        if let Ok(rel) = p.strip_prefix(root) {
            let rel_s = rel.to_string_lossy().replace('\\', "/");
            if wildcard_match(pattern, &rel_s) {
                out.push(p.display().to_string());
            }
        }
    }
}

// 在 `root` 目录下按通配符搜索文件。
// `max_results` 是最后允许返回的最大命中数，`out` 收集所有匹配到的文件路径。
pub fn execute(input: Value) -> String {
    let root = match input.get("root").and_then(|v| v.as_str()) {
        Some(v) if !v.trim().is_empty() => v,
        _ => return "Error: Missing 'root' argument".into(),
    };

    let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => return "Error: Missing 'pattern' argument".into(),
    };

    // max_results: 把模型传来的数量限制裁剪到 1..=2000 之间，避免扫描结果过大。
    let max_results = input
        .get("max_results")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(200)
        .max(1)
        .min(2000);

    let root_path = Path::new(root);
    if !root_path.exists() {
        return format!("Error: Root path does not exist: {}", root);
    }
    if !root_path.is_dir() {
        return format!("Error: Root path is not a directory: {}", root);
    }

    // out: 存放所有命中的文件绝对路径，最终按换行拼成返回文本。
    let mut out = Vec::new();
    walk(root_path, root_path, pattern, &mut out, max_results);

    if out.is_empty() {
        "No files matched the pattern".into()
    } else {
        out.join("\n")
    }
}
