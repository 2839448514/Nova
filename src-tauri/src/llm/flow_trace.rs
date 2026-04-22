//! FlowTracer — per-turn helper that emits `flow-node` events to the frontend.
//!
//! All flow-graph emission logic (struct definitions, detail builders, emit calls)
//! lives here so that `query.rs` stays focused on orchestration.

use tauri::{AppHandle, Emitter};

use crate::llm::types::{AgentMode, Content, ContentBlock, Message, Role};

// ─── Internal event payload ───────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
struct FlowNodeEvent {
    node_id: String,
    label: String,
    status: String,
    detail: Option<String>,
    conversation_id: Option<String>,
}

// ─── FlowTracer ───────────────────────────────────────────────────────────────

/// Attach once per query turn; re-uses `AppHandle` and `conversation_id` on every emit.
pub struct FlowTracer {
    app: AppHandle,
    conversation_id: Option<String>,
}

impl FlowTracer {
    pub fn new(app: &AppHandle, conversation_id: Option<&str>) -> Self {
        Self {
            app: app.clone(),
            conversation_id: conversation_id.map(String::from),
        }
    }

    // ── Base emit ────────────────────────────────────────────────────────────

    /// Fire-and-forget: send one flow-node event to the frontend.
    pub fn emit(&self, node_id: &str, label: &str, status: &str, detail: Option<String>) {
        self.app
            .emit(
                "flow-node",
                FlowNodeEvent {
                    node_id: node_id.into(),
                    label: label.into(),
                    status: status.into(),
                    detail,
                    conversation_id: self.conversation_id.clone(),
                },
            )
            .ok();
    }

    // ── Detail builders ──────────────────────────────────────────────────────

    /// Detail string for the `context_assemble` node: role distribution + token estimate.
    pub fn context_assemble_detail(messages: &[Message]) -> String {
        let user_count = messages.iter().filter(|m| m.role == Role::User).count();
        let assistant_count = messages
            .iter()
            .filter(|m| m.role == Role::Assistant)
            .count();
        let total_chars: usize = messages
            .iter()
            .map(|m| match &m.content {
                Content::Text(t) => t.len(),
                Content::Blocks(blocks) => blocks
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text { text } => text.len(),
                        _ => 0,
                    })
                    .sum(),
            })
            .sum();
        let est_tokens = total_chars / 4;
        format!(
            "共 {} 条（用户 {}，助手 {}）\n约 {} tokens",
            messages.len(),
            user_count,
            assistant_count,
            est_tokens
        )
    }

    /// JSON CompactDiff detail string for the `compact` node (used by the frontend diff view).
    pub fn compact_detail(
        before: &[Message],
        after: &[Message],
        before_tokens: i64,
        after_tokens: i64,
        plan: &str,
        diff_report: &str,
    ) -> String {
        #[derive(serde::Serialize)]
        struct MsgEntry {
            role: String,
            content: String,
            chars: usize,
        }

        fn msg_to_entry(m: &Message) -> MsgEntry {
            let role = match m.role {
                Role::User => "用户",
                Role::Assistant => "助手",
            }
            .to_string();
            let content = match &m.content {
                Content::Text(t) => t.clone(),
                Content::Blocks(blocks) => blocks
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text { text } => text.clone(),
                        ContentBlock::ToolUse { name, id, input } => format!(
                            "[工具调用: {} ({})]\n{}",
                            name,
                            id,
                            serde_json::to_string_pretty(input).unwrap_or_default()
                        ),
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } => {
                            let inner: Vec<String> = content
                                .iter()
                                .filter_map(|cb| {
                                    if let ContentBlock::Text { text } = cb {
                                        Some(text.clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            format!("[工具结果: {}]\n{}", tool_use_id, inner.join("\n"))
                        }
                        ContentBlock::Thinking { thinking } => {
                            format!("[思考]\n{}", thinking)
                        }
                        ContentBlock::Image { .. } => "[图片]".to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            let chars = content.chars().count();
            MsgEntry { role, content, chars }
        }

        #[derive(serde::Serialize)]
        struct CompactDiff {
            #[serde(rename = "type")]
            kind: &'static str,
            summary: String,
            before: Vec<MsgEntry>,
            after: Vec<MsgEntry>,
        }

        let data = CompactDiff {
            kind: "compact_diff",
            summary: format!(
                "压缩前：{} 条 / {} tokens\n压缩后：{} 条 / {} tokens\n\n{}\n\n{}",
                before.len(),
                before_tokens,
                after.len(),
                after_tokens,
                plan,
                diff_report
            ),
            before: before.iter().map(msg_to_entry).collect(),
            after: after.iter().map(msg_to_entry).collect(),
        };
        serde_json::to_string(&data).unwrap_or_default()
    }

    /// Detail string for the `context_final` node: full content of every message sent to the LLM.
    pub fn context_final_detail(messages: &[Message]) -> String {
        let mut lines: Vec<String> = Vec::new();
        let mut total_chars: usize = 0;
        for (idx, msg) in messages.iter().enumerate() {
            let role_label = match msg.role {
                Role::User => "用户",
                Role::Assistant => "助手",
            };
            let text: String = match &msg.content {
                Content::Text(t) => t.clone(),
                Content::Blocks(blocks) => {
                    let mut parts: Vec<String> = Vec::new();
                    for b in blocks {
                        match b {
                            ContentBlock::Text { text } => parts.push(text.clone()),
                            ContentBlock::ToolUse { id, name, input } => parts.push(format!(
                                "[调用工具: {} ({})]\n{}",
                                name,
                                id,
                                serde_json::to_string_pretty(input).unwrap_or_default()
                            )),
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                ..
                            } => {
                                let inner: Vec<String> = content
                                    .iter()
                                    .filter_map(|cb| {
                                        if let ContentBlock::Text { text } = cb {
                                            Some(text.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                parts.push(format!(
                                    "[工具结果: {}]\n{}",
                                    tool_use_id,
                                    inner.join("\n")
                                ));
                            }
                            ContentBlock::Thinking { thinking } => {
                                parts.push(format!("[思考]\n{}", thinking))
                            }
                            ContentBlock::Image { .. } => parts.push("[图片]".into()),
                        }
                    }
                    parts.join("\n")
                }
            };
            let char_count = text.chars().count();
            total_chars += char_count;
            lines.push(format!(
                "── 第 {} 条 [{}] ({} 字符) ──\n{}",
                idx + 1,
                role_label,
                char_count,
                text
            ));
        }
        let approx_tokens = total_chars / 4;
        format!(
            "共 {} 条消息，约 {} tokens\n\n{}",
            messages.len(),
            approx_tokens,
            lines.join("\n\n")
        )
    }

    /// Detail string for the `llm` node: message token estimate + full system prompt.
    pub fn llm_detail(&self, messages: &[Message], agent_mode: AgentMode) -> String {
        use crate::llm::utils::system_prompt::load_system_prompt;
        let sp_info = match load_system_prompt(&self.app, agent_mode) {
            Ok(sp) => {
                let approx_tokens = sp.chars().count() / 4;
                format!("约 {} tokens\n\n{}", approx_tokens, sp)
            }
            Err(_) => "无法加载 system prompt".into(),
        };
        let msg_chars: usize = messages
            .iter()
            .map(|m| match &m.content {
                Content::Text(t) => t.chars().count(),
                Content::Blocks(blocks) => blocks
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text { text } => text.chars().count(),
                        ContentBlock::ToolResult { content, .. } => content
                            .iter()
                            .map(|cb| {
                                if let ContentBlock::Text { text } = cb {
                                    text.chars().count()
                                } else {
                                    0
                                }
                            })
                            .sum(),
                        _ => 0,
                    })
                    .sum(),
            })
            .sum();
        let approx_msg_tokens = msg_chars / 4;
        format!(
            "消息约 {} tokens\n\nSystem Prompt:\n{}",
            approx_msg_tokens, sp_info
        )
    }
}
