use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::llm::types::{Content, ContentBlock, Message, Role};

const DANGEROUS_COMMAND_PATTERNS: &[&str] = &[
    "rm -rf",
    "rm -rf /",
    "rm -rf /*",
    "del /f /s",
    "del /f /s /q",
    "remove-item -recurse",
    "remove-item -force",
    "remove-item -recurse -force",
    "format c:",
    "diskpart",
    "shutdown /s",
    "shutdown -h",
    "reboot",
    "mkfs",
    "git reset --hard",
    "git clean -fd",
    "git clean -fdx",
];

const PROTECTED_PATH_PREFIXES: &[&str] = &[
    "c:\\windows",
    "c:\\program files",
    "c:\\program files (x86)",
    "c:\\programdata",
    "c:\\users\\public",
    "/etc",
    "/bin",
    "/sbin",
    "/usr",
    "/var",
    "/boot",
    "/system",
];

const PROTECTED_PATH_CONTAINS: &[&str] = &[
    "\\.ssh\\",
    "/.ssh/",
    "\\.aws\\",
    "/.aws/",
    "\\.gnupg\\",
    "/.gnupg/",
    "\\.config\\git",
    "/.config/git",
    "\\.git\\config",
    "/.git/config",
];

const DEFAULT_PERMISSION_SCOPE: &str = "__global__";
const PENDING_APPROVAL_TTL_MS: u64 = 15 * 60 * 1000;
const ACTION_TOKEN_TTL_MS: u64 = 60 * 60 * 1000;

fn unsafe_override_enabled() -> bool {
    std::env::var("NOVA_ALLOW_UNSAFE_TOOLS")
        .map(|v| {
            let normalized = v.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes" || normalized == "on"
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
struct ProtectedOperation {
    signature: String,
    preview: String,
    warning: Option<String>,
    needs_approval: bool,
}

#[derive(Debug, Clone)]
struct PendingApproval {
    operation: ProtectedOperation,
    created_at_ms: u64,
}

#[derive(Debug, Default)]
struct ConversationPermissionState {
    pending: HashMap<String, PendingApproval>,
    pending_by_signature: HashMap<String, String>,
    allow_once: HashSet<String>,
    allow_session: HashSet<String>,
    deny_session: HashSet<String>,
    processed_action_tokens: HashMap<String, u64>,
}

#[derive(Debug, Default)]
struct PermissionState {
    conversations: HashMap<String, ConversationPermissionState>,
}

#[derive(Debug, Clone, Copy)]
pub enum PermissionAction {
    AllowOnce,
    AllowSession,
    DenySession,
}

#[derive(Debug)]
pub enum PermissionEnforcement {
    Allow,
    Deny(String),
    AskUser(String),
}

fn permission_state() -> &'static Mutex<PermissionState> {
    static STATE: OnceLock<Mutex<PermissionState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(PermissionState::default()))
}

fn next_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn conversation_scope_key(conversation_id: Option<&str>) -> String {
    conversation_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(DEFAULT_PERMISSION_SCOPE)
        .to_string()
}

fn conversation_state_mut<'a>(
    state: &'a mut PermissionState,
    conversation_id: Option<&str>,
) -> &'a mut ConversationPermissionState {
    let scope = conversation_scope_key(conversation_id);
    state.conversations.entry(scope).or_default()
}

fn prune_expired_pending(state: &mut ConversationPermissionState) {
    let now = now_millis();
    let mut expired_request_ids = Vec::new();

    for (request_id, pending) in &state.pending {
        if now.saturating_sub(pending.created_at_ms) > PENDING_APPROVAL_TTL_MS {
            expired_request_ids.push(request_id.clone());
        }
    }

    for request_id in expired_request_ids {
        if let Some(pending) = state.pending.remove(&request_id) {
            state.pending_by_signature.remove(&pending.operation.signature);
        }
    }
}

fn prune_processed_action_tokens(state: &mut ConversationPermissionState) {
    let now = now_millis();
    state
        .processed_action_tokens
        .retain(|_, ts| now.saturating_sub(*ts) <= ACTION_TOKEN_TTL_MS);
}

fn upsert_pending_request_id(
    state: &mut ConversationPermissionState,
    operation: &ProtectedOperation,
) -> String {
    if let Some(existing_id) = state.pending_by_signature.get(&operation.signature).cloned() {
        if state.pending.contains_key(&existing_id) {
            return existing_id;
        }
        state.pending_by_signature.remove(&operation.signature);
    }

    let request_id = next_request_id();
    state.pending.insert(
        request_id.clone(),
        PendingApproval {
            operation: operation.clone(),
            created_at_ms: now_millis(),
        },
    );
    state
        .pending_by_signature
        .insert(operation.signature.clone(), request_id.clone());
    request_id
}

fn normalize_path_for_match(path: &str) -> String {
    path.trim().replace('/', "\\").to_ascii_lowercase()
}

fn normalize_command_for_match(command: &str) -> String {
    command
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn contains_shell_word(command: &str, target: &str) -> bool {
    command.split_whitespace().any(|token| {
        let cleaned = token.trim_matches(|c: char| {
            !c.is_ascii_alphanumeric() && c != '-' && c != '_'
        });
        cleaned == target
    })
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect::<String>()
}

fn looks_like_shell_mcp(server: &str, tool: &str) -> bool {
    let s = format!("{} {}", server.to_ascii_lowercase(), tool.to_ascii_lowercase());
    ["bash", "shell", "powershell", "pwsh", "terminal"]
        .iter()
        .any(|k| s.contains(k))
}

fn looks_like_file_mcp(server: &str, tool: &str) -> bool {
    let s = format!("{} {}", server.to_ascii_lowercase(), tool.to_ascii_lowercase());
    ["file", "filesystem", "fs", "write", "edit", "replace"]
        .iter()
        .any(|k| s.contains(k))
}

fn pick_string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(v) = value.get(*key).and_then(|v| v.as_str()) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn check_mcp_operation(server: &str, tool: &str, arguments: &Value) -> Result<(), String> {
    if looks_like_shell_mcp(server, tool) {
        let command = pick_string_field(arguments, &["command", "cmd", "script"]).unwrap_or_default();
        return check_command(command);
    }

    if looks_like_file_mcp(server, tool) {
        let path = pick_string_field(
            arguments,
            &["path", "file", "file_path", "target", "target_path"],
        )
        .unwrap_or_default();
        return check_file_path(path);
    }

    Ok(())
}

fn operation_from_input(tool_name: &str, input: &Value) -> Option<ProtectedOperation> {
    match tool_name {
        "execute_bash" | "execute_powershell" => {
            let command = input
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim();

            if command.is_empty() {
                return Some(ProtectedOperation {
                    signature: format!("{}:<empty>", tool_name),
                    preview: "命令为空".to_string(),
                    warning: Some("命令为空，无法执行。".to_string()),
                    needs_approval: false,
                });
            }

            let normalized = normalize_command_for_match(command);
            let risk = check_command(command).err();

            Some(ProtectedOperation {
                signature: format!("{}:{}", tool_name, normalized),
                preview: format!("{}: {}", tool_name, truncate_chars(command, 180)),
                warning: risk.clone(),
                needs_approval: risk.is_some(),
            })
        }
        "replace_string_in_file" | "write_file" => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim();

            if path.is_empty() {
                return Some(ProtectedOperation {
                    signature: format!("{}:<empty>", tool_name),
                    preview: "路径为空".to_string(),
                    warning: Some("目标路径为空，无法执行。".to_string()),
                    needs_approval: false,
                });
            }

            let normalized = normalize_path_for_match(path);
            let risk = check_file_path(path).err();

            Some(ProtectedOperation {
                signature: format!("{}:{}", tool_name, normalized),
                preview: format!("{}: {}", tool_name, truncate_chars(path, 200)),
                warning: risk.clone(),
                needs_approval: risk.is_some(),
            })
        }
        "mcp_tool" => {
            let server = input
                .get("server")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim();
            let tool = input
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim();
            let arguments = input
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));

            if server.is_empty() || tool.is_empty() {
                return Some(ProtectedOperation {
                    signature: "mcp_tool:<empty>".to_string(),
                    preview: "mcp_tool: server/tool 为空".to_string(),
                    warning: Some("mcp_tool 缺少 server 或 tool，无法执行。".to_string()),
                    needs_approval: false,
                });
            }

            let risk = check_mcp_operation(server, tool, &arguments).err();

            Some(ProtectedOperation {
                signature: format!(
                    "mcp_tool:{}:{}:{}",
                    server.to_ascii_lowercase(),
                    tool.to_ascii_lowercase(),
                    normalize_command_for_match(&arguments.to_string())
                ),
                preview: format!(
                    "mcp_tool {}::{} {}",
                    server,
                    tool,
                    truncate_chars(&arguments.to_string(), 160)
                ),
                warning: risk.clone(),
                needs_approval: risk.is_some(),
            })
        }
        _ => None,
    }
}

fn build_permission_prompt_payload(request_id: &str, operation: &ProtectedOperation) -> String {
    let mut context = format!("请求执行高风险操作：{}", operation.preview);
    if let Some(w) = &operation.warning {
        context.push_str("。风险提示：");
        context.push_str(w);
    }

    json!({
        "type": "needs_user_input",
        "context": context,
        "allow_freeform": true,
        "questions": [
            {
                "header": "权限审批",
                "question": "请选择处理方式",
                "multi_select": false,
                "options": [
                    {
                        "label": format!("仅本次允许 [ALLOW_ONCE::{}]", request_id),
                        "description": "只放行这一次，执行后自动失效"
                    },
                    {
                        "label": format!("本会话允许 [ALLOW_SESSION::{}]", request_id),
                        "description": "本次应用运行期间对同一操作持续放行"
                    },
                    {
                        "label": format!("拒绝并记住 [DENY_SESSION::{}]", request_id),
                        "description": "本会话拒绝同一操作，直到会话结束"
                    }
                ]
            }
        ]
    })
    .to_string()
}

fn approval_action_tokens() -> [(&'static str, PermissionAction); 3] {
    [
        ("ALLOW_ONCE::", PermissionAction::AllowOnce),
        ("ALLOW_SESSION::", PermissionAction::AllowSession),
        ("DENY_SESSION::", PermissionAction::DenySession),
    ]
}

fn extract_action_tokens(text: &str) -> Vec<(PermissionAction, String, String)> {
    let mut out = Vec::new();
    for (prefix, action) in approval_action_tokens() {
        let mut cursor = 0usize;
        while let Some(rel) = text[cursor..].find(prefix) {
            let start = cursor + rel + prefix.len();
            let id = text[start..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect::<String>();
            if !id.is_empty() {
                let token = format!("{}{}", prefix, id);
                out.push((action, id, token));
            }
            cursor = start;
            if cursor >= text.len() {
                break;
            }
        }
    }
    out
}

fn apply_decision(
    state: &mut ConversationPermissionState,
    action: PermissionAction,
    request_id: &str,
) -> bool {
    let Some(pending) = state.pending.remove(request_id) else {
        return false;
    };

    let signature = pending.operation.signature;
    state.pending_by_signature.remove(&signature);
    state.allow_once.remove(&signature);
    state.allow_session.remove(&signature);
    state.deny_session.remove(&signature);

    match action {
        PermissionAction::AllowOnce => {
            state.allow_once.insert(signature);
        }
        PermissionAction::AllowSession => {
            state.allow_session.insert(signature);
        }
        PermissionAction::DenySession => {
            state.deny_session.insert(signature);
        }
    }

    true
}

fn extract_text_from_message(message: &Message) -> Vec<String> {
    match &message.content {
        Content::Text(t) => vec![t.clone()],
        Content::Blocks(blocks) => blocks
            .iter()
            .filter_map(|b| {
                if let ContentBlock::Text { text } = b {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect(),
    }
}

pub fn consume_user_permission_decisions(
    conversation_id: Option<&str>,
    messages: &[Message],
) -> usize {
    let mut applied = 0usize;
    let mut guard = match permission_state().lock() {
        Ok(g) => g,
        Err(_) => return 0,
    };

    let state = conversation_state_mut(&mut guard, conversation_id);
    prune_expired_pending(state);
    prune_processed_action_tokens(state);

    for message in messages {
        if message.role != Role::User {
            continue;
        }

        for text in extract_text_from_message(message) {
            for (action, request_id, token) in extract_action_tokens(&text) {
                if state.processed_action_tokens.contains_key(&token) {
                    continue;
                }
                state.processed_action_tokens.insert(token, now_millis());
                if apply_decision(state, action, &request_id) {
                    applied += 1;
                }
            }
        }
    }

    applied
}

fn check_command(command: &str) -> Result<(), String> {
    let normalized = normalize_command_for_match(command);
    if normalized.is_empty() {
        return Err("Blocked by permission gate: command is empty".to_string());
    }

    for dangerous_word in ["rm", "del", "remove-item"] {
        if contains_shell_word(&normalized, dangerous_word) {
            return Err(format!(
                "Blocked by permission gate: command contains dangerous shell command '{}'. Set NOVA_ALLOW_UNSAFE_TOOLS=1 only for trusted debugging.",
                dangerous_word
            ));
        }
    }

    for pattern in DANGEROUS_COMMAND_PATTERNS {
        if normalized.contains(pattern) {
            return Err(format!(
                "Blocked by permission gate: command contains dangerous pattern '{}'. Set NOVA_ALLOW_UNSAFE_TOOLS=1 only for trusted debugging.",
                pattern
            ));
        }
    }

    Ok(())
}

fn check_file_path(path: &str) -> Result<(), String> {
    let normalized = normalize_path_for_match(path);
    if normalized.is_empty() {
        return Err("Blocked by permission gate: target path is empty".to_string());
    }

    for prefix in PROTECTED_PATH_PREFIXES {
        if normalized.starts_with(prefix) {
            return Err(format!(
                "Blocked by permission gate: writing protected path '{}'. Set NOVA_ALLOW_UNSAFE_TOOLS=1 only for trusted debugging.",
                path
            ));
        }
    }

    for marker in PROTECTED_PATH_CONTAINS {
        if normalized.contains(marker) {
            return Err(format!(
                "Blocked by permission gate: writing sensitive path '{}'. Set NOVA_ALLOW_UNSAFE_TOOLS=1 only for trusted debugging.",
                path
            ));
        }
    }

    Ok(())
}

pub fn enforce_tool_permission(
    _app: &AppHandle,
    conversation_id: Option<&str>,
    tool_name: &str,
    input: &Value,
) -> PermissionEnforcement {
    if unsafe_override_enabled() {
        return PermissionEnforcement::Allow;
    }

    let Some(operation) = operation_from_input(tool_name, input) else {
        return PermissionEnforcement::Allow;
    };

    let mut guard = match permission_state().lock() {
        Ok(g) => g,
        Err(_) => {
            return PermissionEnforcement::Deny(
                "Permission state unavailable due to lock poisoning".to_string(),
            )
        }
    };

    let state = conversation_state_mut(&mut guard, conversation_id);
    prune_expired_pending(state);

    if state.deny_session.contains(&operation.signature) {
        return PermissionEnforcement::Deny(format!(
            "Blocked by permission gate: this operation was denied in current session ({})",
            operation.preview
        ));
    }

    if state.allow_session.contains(&operation.signature) {
        return PermissionEnforcement::Allow;
    }

    if state.allow_once.remove(&operation.signature) {
        return PermissionEnforcement::Allow;
    }

    if operation.needs_approval {
        let request_id = upsert_pending_request_id(state, &operation);
        return PermissionEnforcement::AskUser(build_permission_prompt_payload(
            &request_id,
            &operation,
        ));
    }

    PermissionEnforcement::Allow
}
