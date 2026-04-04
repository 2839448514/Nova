use crate::llm::types::Tool;
use serde_json::{json, Value};

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

fn percent_encode_query(s: &str) -> String {
    s.chars()
        .flat_map(|c| -> Vec<String> {
            match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                    vec![c.to_string()]
                }
                ' ' => vec!["+".to_string()],
                _ => {
                    let mut buf = [0u8; 4];
                    let encoded = c.encode_utf8(&mut buf);
                    encoded.bytes().map(|b| format!("%{:02X}", b)).collect::<Vec<_>>()
                }
            }
        })
        .collect()
}

pub fn execute(input: Value) -> String {
    let query = match input.get("query").and_then(|v| v.as_str()) {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => return "Error: Missing 'query' argument".into(),
    };

    let encoded = percent_encode_query(query);
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
