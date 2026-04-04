use crate::llm::types::{Content, ContentBlock, Message, Role};

#[derive(Debug, Default, Clone)]
pub struct HookOutcome {
    pub additional_messages: Vec<Message>,
    pub prevent_continuation: bool,
    pub stop_reason: Option<String>,
    pub override_error: Option<String>,
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn env_csv_lower_list(key: &str) -> Vec<String> {
    std::env::var(key)
        .ok()
        .map(|v| {
            v.split(',')
                .map(|part| part.trim().to_ascii_lowercase())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn latest_assistant_text(messages: &[Message]) -> String {
    messages
        .iter()
        .rev()
        .find_map(|m| {
            if m.role != Role::Assistant {
                return None;
            }
            match &m.content {
                Content::Text(t) => Some(t.clone()),
                Content::Blocks(blocks) => Some(
                    blocks
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::Text { text } = b {
                                Some(text.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
            }
        })
        .unwrap_or_default()
}

fn has_exact_user_message(messages: &[Message], expected: &str) -> bool {
    messages.iter().any(|m| {
        if m.role != Role::User {
            return false;
        }
        matches!(&m.content, Content::Text(text) if text == expected)
    })
}

fn context_message(prefix: &str, text: &str) -> Message {
    Message {
        role: Role::User,
        content: Content::Text(format!("{} {}", prefix, text.trim())),
    }
}

pub fn run_pre_tool_use_hooks(
    tool_name: &str,
    _input: &serde_json::Value,
    _conversation_id: Option<&str>,
) -> HookOutcome {
    let mut out = HookOutcome::default();
    let deny_list = env_csv_lower_list("NOVA_PRE_TOOL_DENY_TOOLS");
    if !deny_list.is_empty() {
        let tool_lower = tool_name.to_ascii_lowercase();
        if deny_list.iter().any(|name| name == &tool_lower) {
            out.override_error = Some(format!(
                "Blocked by PreToolUse hook: tool '{}' is deny-listed via NOVA_PRE_TOOL_DENY_TOOLS",
                tool_name
            ));
        }
    }

    if let Ok(extra) = std::env::var("NOVA_PRE_TOOL_CONTEXT") {
        if !extra.trim().is_empty() {
            out.additional_messages
                .push(context_message("[PreToolUse]", &extra));
        }
    }

    out
}

pub fn run_post_tool_use_hooks(
    tool_name: &str,
    _input: &serde_json::Value,
    output: &str,
    is_error: bool,
    _conversation_id: Option<&str>,
) -> HookOutcome {
    let mut out = HookOutcome::default();

    if let Ok(extra) = std::env::var("NOVA_POST_TOOL_CONTEXT") {
        if !extra.trim().is_empty() {
            out.additional_messages
                .push(context_message("[PostToolUse]", &extra));
        }
    }

    if env_truthy("NOVA_POST_TOOL_STOP_ON_ERROR") && is_error {
        out.prevent_continuation = true;
        out.stop_reason = Some(format!(
            "PostToolUse hook stopped continuation after '{}' returned an error",
            tool_name
        ));
    }

    if let Ok(block_pattern) = std::env::var("NOVA_POST_TOOL_BLOCK_PATTERN") {
        let pattern = block_pattern.trim();
        if !pattern.is_empty() && output.contains(pattern) {
            out.prevent_continuation = true;
            out.stop_reason = Some(format!(
                "PostToolUse hook stopped continuation because tool output matched pattern '{}'",
                pattern
            ));
        }
    }

    out
}

pub fn run_post_tool_use_failure_hooks(
    tool_name: &str,
    _input: &serde_json::Value,
    error: &str,
    _conversation_id: Option<&str>,
) -> HookOutcome {
    let mut out = HookOutcome::default();

    if let Ok(extra) = std::env::var("NOVA_POST_TOOL_FAILURE_CONTEXT") {
        if !extra.trim().is_empty() {
            out.additional_messages
                .push(context_message("[PostToolUseFailure]", &extra));
        }
    }

    if env_truthy("NOVA_POST_TOOL_FAILURE_STOP") {
        out.prevent_continuation = true;
        out.stop_reason = Some(format!(
            "PostToolUseFailure hook stopped continuation after '{}' failed: {}",
            tool_name, error
        ));
    }

    out
}

pub fn run_stop_hooks(messages: &[Message], _conversation_id: Option<&str>) -> HookOutcome {
    let mut out = HookOutcome::default();

    if let Ok(limit) = std::env::var("NOVA_STOP_HOOK_MAX_ASSISTANT_MESSAGES") {
        if let Ok(max_assistant_messages) = limit.trim().parse::<usize>() {
            if max_assistant_messages > 0 {
                let assistant_count = messages
                    .iter()
                    .filter(|m| m.role == Role::Assistant)
                    .count();
                if assistant_count > max_assistant_messages {
                    out.prevent_continuation = true;
                    out.stop_reason = Some(format!(
                        "Stop hook prevented continuation: assistant message count {} exceeds limit {}",
                        assistant_count, max_assistant_messages
                    ));
                    return out;
                }
            }
        }
    }

    if let Ok(pattern) = std::env::var("NOVA_STOP_HOOK_BLOCK_PATTERN") {
        let block_pattern = pattern.trim();
        if !block_pattern.is_empty() {
            let assistant_text = latest_assistant_text(messages);
            if assistant_text.contains(block_pattern) {
                out.prevent_continuation = true;
                out.stop_reason = Some(format!(
                    "Stop hook prevented continuation because assistant text matched pattern '{}'",
                    block_pattern
                ));
                return out;
            }
        }
    }

    if let Ok(extra) = std::env::var("NOVA_STOP_HOOK_APPEND_CONTEXT") {
        let trimmed = extra.trim();
        if !trimmed.is_empty() {
            let body = format!("[StopHookContext] {}", trimmed);
            if !has_exact_user_message(messages, &body) {
                out.additional_messages.push(Message {
                    role: Role::User,
                    content: Content::Text(body),
                });
            }
        }
    }

    out
}
