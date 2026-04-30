use crate::llm::tools::{sync_tool, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};

// 返回 web_search 的注册信息。
// 这个工具只生成搜索链接，所以可以作为只读工具并发执行。
pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, true, None)
}

// 返回模型可见的 web_search 元数据。
// 模型传入 `query` 后，这个工具不会联网搜索，只负责构造后续可抓取的搜索结果页 URL。
pub fn tool() -> Tool {
    Tool {
        name: "web_search".into(),
        description: "Create a web search URL for a query and provide guidance for next fetch.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" }
            },
            "required": ["query"]
        }),
    }
}

// 根据 `query` 生成搜索引擎 URL，供后续 `web_fetch` 继续抓取。
// `encoded` 是把空格替换后的简单查询串，`ddg`/`bing` 是两个候选结果页地址。
pub fn execute(input: Value) -> String {
    let query = match input.get("query").and_then(|v| v.as_str()) {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => return "Error: Missing 'query' argument".into(),
    };

    let encoded = query.replace(' ', "+");
    let ddg = format!("https://duckduckgo.com/?q={}", encoded);
    let bing = format!("https://www.bing.com/search?q={}", encoded);

    json!({
        "query": query,
        "search_urls": {
            "duckduckgo": ddg,
            "bing": bing
        },
        "note": "Use web_fetch with one of these URLs to inspect result pages."
    })
    .to_string()
}
