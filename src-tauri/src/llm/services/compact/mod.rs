use tauri::AppHandle;
use serde_json::Value;

use crate::llm::types::{Content, ContentBlock, Message, Role};

// 每条消息、块、工具使用/工具结果的静态开销。用于 token 估算近似，防止只依赖字符数导致低估。
const TOKEN_OVERHEAD_PER_MESSAGE: i64 = 6;
const TOKEN_OVERHEAD_PER_BLOCK: i64 = 3;
const TOKEN_OVERHEAD_TOOL_USE: i64 = 20;
const TOKEN_OVERHEAD_TOOL_RESULT: i64 = 14;

// 策略阈值：目前根据历史消息数和估算 token 来判断是否启用Micro/Full压缩。
const MICRO_COMPACT_MESSAGE_THRESHOLD: usize = 24;
const MICRO_COMPACT_TOKEN_THRESHOLD: i64 = 3200;
const FULL_COMPACT_MESSAGE_THRESHOLD: usize = 42;
const FULL_COMPACT_TOKEN_THRESHOLD: i64 = 5600;

// 工具结果体积判断：超过这个阈值时，直接走 Micro 压缩。
const LARGE_TOOL_RESULT_CHAR_THRESHOLD: usize = 2800;

// 截断值：在 tool_result 里保持头尾信息, 避免 payload 过长。
const TOOL_RESULT_TEXT_TRUNCATE_LIMIT: usize = 1200;

// JSON 压缩上限，避免深层数组/对象导致多次迭代爆炸。
const TOOL_RESULT_JSON_MAX_DEPTH: usize = 3;
const TOOL_RESULT_JSON_MAX_ITEMS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompactLevel {
    None,
    Micro,
    Full,
}

#[derive(Debug, Clone, Copy)]
struct CompactDecision {
    level: CompactLevel,
    estimated_tokens: i64,
    message_count: usize,
    has_large_tool_result: bool,
}

// 判断字符是否属于中日韩Unicode块。此处通过字节范围直接判断，避免调用 heavy regex。
fn is_cjk_char(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0x3040..=0x30FF
            | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF
    )
}

// 估算纯文本片段的 token 数量。
// 规则：
// - CJK 每字符计 1 token
// - ASCII 字母数字 视为平均 4 个字符 1 token
// - 标点/符号按 2 字符 1 token
// - 空白按 16 字符 1 token
fn estimate_text_tokens(text: &str) -> i64 {
    // 统计各类字符数量
    let mut cjk = 0_i64;
    let mut latin_or_digit = 0_i64;
    let mut punctuation_or_symbol = 0_i64;
    let mut whitespace = 0_i64;

    for ch in text.chars() {
        if ch.is_whitespace() {
            // 空白字符（空格、换行等）按更低权重计算
            whitespace += 1;
        } else if is_cjk_char(ch) {
            // 中日韩字符每个近似 1 token
            cjk += 1;
        } else if ch.is_ascii_alphanumeric() {
            // ASCII 字母数字日常出现频率高：4 字符约 1 token
            latin_or_digit += 1;
        } else {
            // 标点/符号按 2 字符约 1 token
            punctuation_or_symbol += 1;
        }
    }

    // 最终汇总：每类按比例归一并加权
    cjk + (latin_or_digit + 3) / 4 + (punctuation_or_symbol + 1) / 2 + (whitespace + 15) / 16
}

// 估算 JSON 数据结构的 token 大小，包含结构符号 + 键值 + 嵌套内容。
// 用于 tool_use / tool_result 中包含结构化 JSON 字符串时提供更加合理的估算。
fn estimate_json_tokens(value: &Value) -> i64 {
    match value {
        Value::Null => 1,
        Value::Bool(_) => 1,
        Value::Number(_) => 2,
        Value::String(s) => estimate_text_tokens(s) + 1,
        Value::Array(items) => {
            // array 头尾 + 项目分隔符
            2 + items.iter().map(estimate_json_tokens).sum::<i64>() + items.len() as i64
        }
        Value::Object(map) => {
            // object 头尾 + key/value + key value 分隔符
            3 + map
                .iter()
                .map(|(k, v)| estimate_text_tokens(k) + estimate_json_tokens(v) + 2)
                .sum::<i64>()
        }
    }
}

// 估算一个 ContentBlock 的 token。对不同块类型使用差异化计算：
// - Text 按 text 字符内容估算
// - ToolUse 带 json 参数，需要额外估算 JSON 结构
// - ToolResult 递归估算嵌套块，并加上工具结果固定开销
fn estimate_block_tokens(block: &ContentBlock) -> i64 {
    match block {
        ContentBlock::Text { text } => TOKEN_OVERHEAD_PER_BLOCK + estimate_text_tokens(text),
        ContentBlock::ToolUse { input, .. } => {
            TOKEN_OVERHEAD_PER_BLOCK + TOKEN_OVERHEAD_TOOL_USE + estimate_json_tokens(input)
        }
        ContentBlock::ToolResult { content, .. } => {
            TOKEN_OVERHEAD_PER_BLOCK
                + TOKEN_OVERHEAD_TOOL_RESULT
                + content.iter().map(estimate_block_tokens).sum::<i64>()
        }
    }
}

// 更细颗粒度 token 估算：
// - 文本按 CJK/ASCII/符号分桶估算
// - 工具结构按 message/block/json 增加固定结构开销
fn estimate_message_tokens(messages: &[Message]) -> i64 {
    messages
        .iter()
        .map(|m| {
            let body = match &m.content {
                Content::Text(text) => estimate_text_tokens(text),
                Content::Blocks(blocks) => blocks.iter().map(estimate_block_tokens).sum::<i64>(),
            };
            TOKEN_OVERHEAD_PER_MESSAGE + body
        })
        .sum::<i64>()
}

fn max_tool_result_text_chars(messages: &[Message]) -> usize {
    messages
        .iter()
        .filter_map(|m| match &m.content {
            Content::Blocks(blocks) => Some(blocks),
            Content::Text(_) => None,
        })
        .flat_map(|blocks| blocks.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolResult { content, .. } => Some(content),
            _ => None,
        })
        .flat_map(|content| content.iter())
        .filter_map(|inner| match inner {
            ContentBlock::Text { text } => Some(text.chars().count()),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

fn decide_compact_strategy(messages: &[Message]) -> CompactDecision {
    let estimated_tokens = estimate_message_tokens(messages);
    let message_count = messages.len();
    let has_large_tool_result = max_tool_result_text_chars(messages) >= LARGE_TOOL_RESULT_CHAR_THRESHOLD;

    let level = if message_count >= FULL_COMPACT_MESSAGE_THRESHOLD
        || estimated_tokens >= FULL_COMPACT_TOKEN_THRESHOLD
    {
        CompactLevel::Full
    } else if message_count >= MICRO_COMPACT_MESSAGE_THRESHOLD
        || estimated_tokens >= MICRO_COMPACT_TOKEN_THRESHOLD
        || has_large_tool_result
    {
        CompactLevel::Micro
    } else {
        CompactLevel::None
    };

    CompactDecision {
        level,
        estimated_tokens,
        message_count,
        has_large_tool_result,
    }
}

fn maybe_needs_user_input_payload(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| {
            v.get("type")
                .and_then(|t| t.as_str())
                .map(|s| s == "needs_user_input")
        })
        .unwrap_or(false)
}

fn truncate_text_by_chars(text: &str, limit: usize) -> String {
    let len = text.chars().count();
    if len <= limit {
        return text.to_string();
    }

    let head_len = (limit * 60) / 100;
    let tail_len = (limit * 30) / 100;
    let omitted = len.saturating_sub(head_len + tail_len);

    let head: String = text.chars().take(head_len).collect();
    let tail: String = text
        .chars()
        .rev()
        .take(tail_len)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();

    format!(
        "{}\n...[micro-compact truncated {} chars]...\n{}",
        head, omitted, tail
    )
}

fn compact_json_value(value: &Value, depth: usize) -> Value {
    if depth >= TOOL_RESULT_JSON_MAX_DEPTH {
        return Value::String("<truncated: max depth reached>".to_string());
    }

    match value {
        Value::Array(items) => {
            let mut out: Vec<Value> = items
                .iter()
                .take(TOOL_RESULT_JSON_MAX_ITEMS)
                .map(|v| compact_json_value(v, depth + 1))
                .collect();
            if items.len() > TOOL_RESULT_JSON_MAX_ITEMS {
                out.push(serde_json::json!({
                    "_truncated_items": items.len() - TOOL_RESULT_JSON_MAX_ITEMS
                }));
            }
            Value::Array(out)
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();

            let mut out = serde_json::Map::new();
            for key in keys.into_iter().take(TOOL_RESULT_JSON_MAX_ITEMS) {
                if let Some(v) = map.get(key) {
                    out.insert(key.clone(), compact_json_value(v, depth + 1));
                }
            }
            if map.len() > TOOL_RESULT_JSON_MAX_ITEMS {
                out.insert(
                    "_truncated_keys".to_string(),
                    Value::from((map.len() - TOOL_RESULT_JSON_MAX_ITEMS) as i64),
                );
            }
            Value::Object(out)
        }
        Value::String(s) => Value::String(truncate_text_by_chars(s, TOOL_RESULT_TEXT_TRUNCATE_LIMIT)),
        _ => value.clone(),
    }
}

fn compact_tool_result_text(text: &str) -> String {
    // 交互类 payload 不能压缩，否则会破坏后续 ask-user 的语义。
    if maybe_needs_user_input_payload(text) {
        return text.to_string();
    }

    if let Ok(value) = serde_json::from_str::<Value>(text) {
        let compacted = compact_json_value(&value, 0);
        if let Ok(serialized) = serde_json::to_string(&compacted) {
            return truncate_text_by_chars(&serialized, TOOL_RESULT_TEXT_TRUNCATE_LIMIT);
        }
    }

    truncate_text_by_chars(text, TOOL_RESULT_TEXT_TRUNCATE_LIMIT)
}

fn apply_micro_compact(messages: &[Message]) -> Vec<Message> {
    messages
        .iter()
        .map(|m| {
            let content = match &m.content {
                Content::Text(text) => Content::Text(text.clone()),
                Content::Blocks(blocks) => Content::Blocks(
                    blocks
                        .iter()
                        .map(|block| match block {
                            ContentBlock::ToolResult {
                                tool_use_id,
                                is_error,
                                content,
                            } => {
                                let compacted_content = content
                                    .iter()
                                    .map(|inner| match inner {
                                        ContentBlock::Text { text } => ContentBlock::Text {
                                            text: compact_tool_result_text(text),
                                        },
                                        _ => inner.clone(),
                                    })
                                    .collect();

                                ContentBlock::ToolResult {
                                    tool_use_id: tool_use_id.clone(),
                                    is_error: *is_error,
                                    content: compacted_content,
                                }
                            }
                            _ => block.clone(),
                        })
                        .collect(),
                ),
            };

            Message {
                role: m.role.clone(),
                content,
            }
        })
        .collect()
}

async fn apply_full_compact(
    app: &AppHandle,
    conversation_id: Option<&str>,
    messages: &[Message],
) -> Vec<Message> {
    let Some(conversation_id) = conversation_id.filter(|id| !id.trim().is_empty()) else {
        return messages.to_vec();
    };

    let compact = match crate::command::history::get_conversation_compact_context(
        app.clone(),
        conversation_id.to_string(),
        Some(2200),
        Some(10),
    )
    .await
    {
        Ok(v) => v,
        Err(_) => return messages.to_vec(),
    };

    let handover = crate::command::history::get_conversation_handover(
        app.clone(),
        conversation_id.to_string(),
        Some(compact.recent_limit),
    )
    .await
    .ok();

    if let Some(handover) = handover {
        let _ = crate::command::history::record_compact_boundary(
            app.clone(),
            &compact,
            &handover.summary,
            &handover.key_facts,
        )
        .await;
    }

    let keep_count = compact.recent_limit.clamp(6, 30) as usize;
    let recent_messages = if messages.len() > keep_count {
        messages[messages.len() - keep_count..].to_vec()
    } else {
        messages.to_vec()
    };

    let compact_message = Message {
        role: Role::User,
        content: Content::Text(compact.context_text),
    };

    let mut prepared = Vec::with_capacity(recent_messages.len() + 1);
    prepared.push(compact_message);
    prepared.extend(recent_messages);
    prepared
}

// 入口：按层级执行 compact。
// - None: 不压缩
// - Micro: 仅本地清洗 tool_result（尤其长 JSON/长文本）
// - Full: 先做 Micro，再拼接 compact 历史上下文 + 最近窗口
pub async fn prepare_messages_for_turn(
    app: &AppHandle,
    conversation_id: Option<&str>,
    messages: &[Message],
) -> Vec<Message> {
    let decision = decide_compact_strategy(messages);
    eprintln!(
        "[compact] level={:?} message_count={} estimated_tokens={} has_large_tool_result={}",
        decision.level,
        decision.message_count,
        decision.estimated_tokens,
        decision.has_large_tool_result
    );

    match decision.level {
        CompactLevel::None => messages.to_vec(),
        CompactLevel::Micro => apply_micro_compact(messages),
        CompactLevel::Full => {
            let micro_compacted = apply_micro_compact(messages);
            apply_full_compact(app, conversation_id, &micro_compacted).await
        }
    }
}

// 检查当前输出消息是否包含工具结果里标记为需要用户输入的 payload，
// 用于跑宏任务时暂停回合并向前端触发交互。
pub fn has_needs_user_input(messages: &[Message]) -> bool {
    messages.iter().any(|m| {
        let Content::Blocks(blocks) = &m.content else {
            return false;
        };

        blocks.iter().any(|b| {
            let ContentBlock::ToolResult { content, .. } = b else {
                return false;
            };

            content.iter().any(|inner| {
                let ContentBlock::Text { text } = inner else {
                    return false;
                };

                // 解析 JSON 字符串，若 type==needs_user_input 则认为需要用户继续输入。
                serde_json::from_str::<serde_json::Value>(text)
                    .ok()
                    .and_then(|v| {
                        v.get("type")
                            .and_then(|t| t.as_str())
                            .map(|s| s == "needs_user_input")
                    })
                    .unwrap_or(false)
            })
        })
    })
}
