use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

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
}

#[derive(Debug, Clone)]
struct PendingApproval {
    operation: ProtectedOperation,
}

#[derive(Debug, Default)]
struct PermissionState {
    pending: HashMap<String, PendingApproval>,
    allow_once: HashSet<String>,
    allow_session: HashSet<String>,
    deny_session: HashSet<String>,
    processed_action_tokens: HashSet<String>,
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

fn truncate_chars(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect::<String>()
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
                });
            }

            let normalized = normalize_command_for_match(command);
            let warning = DANGEROUS_COMMAND_PATTERNS
                .iter()
                .find(|p| normalized.contains(**p))
                .map(|p| format!("命令命中高危模式 '{}'", p));

            Some(ProtectedOperation {
                signature: format!("{}:{}", tool_name, normalized),
                preview: format!("{}: {}", tool_name, truncate_chars(command, 180)),
                warning,
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
                });
            }

            let normalized = normalize_path_for_match(path);
            let mut warning = None;

            if PROTECTED_PATH_PREFIXES
                .iter()
                .any(|prefix| normalized.starts_with(prefix))
            {
                warning = Some("目标路径位于受保护目录前缀".to_string());
            } else if PROTECTED_PATH_CONTAINS
                .iter()
                .any(|marker| normalized.contains(marker))
            {
                warning = Some("目标路径包含敏感配置目录".to_string());
            }

            Some(ProtectedOperation {
                signature: format!("{}:{}", tool_name, normalized),
                preview: format!("{}: {}", tool_name, truncate_chars(path, 200)),
                warning,
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

fn apply_decision(state: &mut PermissionState, action: PermissionAction, request_id: &str) -> bool {
    let Some(pending) = state.pending.remove(request_id) else {
        return false;
    };

    let signature = pending.operation.signature;
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

pub fn consume_user_permission_decisions(messages: &[Message]) -> usize {
    let mut applied = 0usize;
    let mut guard = match permission_state().lock() {
        Ok(g) => g,
        Err(_) => return 0,
    };

    for message in messages {
        if message.role != Role::User {
            continue;
        }

        for text in extract_text_from_message(message) {
            for (action, request_id, token) in extract_action_tokens(&text) {
                if guard.processed_action_tokens.contains(&token) {
                    continue;
                }
                guard.processed_action_tokens.insert(token);
                if apply_decision(&mut guard, action, &request_id) {
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

pub fn enforce_tool_permission(_app: &AppHandle, tool_name: &str, input: &Value) -> PermissionEnforcement {
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

    if guard.deny_session.contains(&operation.signature) {
        return PermissionEnforcement::Deny(format!(
            "Blocked by permission gate: this operation was denied in current session ({})",
            operation.preview
        ));
    }

    if guard.allow_session.contains(&operation.signature) {
        return PermissionEnforcement::Allow;
    }

    if guard.allow_once.remove(&operation.signature) {
        return PermissionEnforcement::Allow;
    }

    match tool_name {
        "execute_bash" | "execute_powershell" => {
            let command = input.get("command").and_then(|v| v.as_str()).unwrap_or_default();
            if let Err(_e) = check_command(command) {
                let request_id = next_request_id();
                guard.pending.insert(
                    request_id.clone(),
                    PendingApproval {
                        operation: operation.clone(),
                    },
                );
                return PermissionEnforcement::AskUser(build_permission_prompt_payload(
                    &request_id,
                    &operation,
                ));
            }
            return PermissionEnforcement::Allow;
        }
        "replace_string_in_file" | "write_file" => {
            let path = input.get("path").and_then(|v| v.as_str()).unwrap_or_default();
            if let Err(_e) = check_file_path(path) {
                let request_id = next_request_id();
                guard.pending.insert(
                    request_id.clone(),
                    PendingApproval {
                        operation: operation.clone(),
                    },
                );
                return PermissionEnforcement::AskUser(build_permission_prompt_payload(
                    &request_id,
                    &operation,
                ));
            }
            return PermissionEnforcement::Allow;
        }
        _ => {
            return PermissionEnforcement::Allow;
        }
    }
}
