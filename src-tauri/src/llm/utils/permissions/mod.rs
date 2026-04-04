use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::llm::types::{Content, ContentBlock, Message, Role};

// Command fragments considered destructive enough to always be gated.
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

// Path prefixes that should never be written without explicit override.
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

// Sensitive path markers that should be blocked even outside protected roots.
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
            // 统一做 trim + 小写，避免环境变量大小写或空格导致误判。
            let normalized = v.trim().to_ascii_lowercase();
            // normalized: 规范化后的环境变量值。
            // 兼容常见的布尔开关写法。
            normalized == "1" || normalized == "true" || normalized == "yes" || normalized == "on"
        })
        // 变量缺失时默认关闭不安全放行。
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
        // 权限过期只需要毫秒精度，u64 足够表达。
        .map(|d| d.as_millis() as u64)
        // 系统时钟异常（早于 epoch）时回退到 0，避免 panic。
        .unwrap_or(0)
}

fn conversation_scope_key(conversation_id: Option<&str>) -> String {
    conversation_id
        // 将 conversation_id 裁剪为没有前后空白的值。
        .map(str::trim)
        // 空字符串视为未提供会话 id。
        .filter(|id| !id.is_empty())
        // 缺省落到全局 scope。
        .unwrap_or(DEFAULT_PERMISSION_SCOPE)
        .to_string()
}

fn conversation_state_mut<'a>(
    state: &'a mut PermissionState,
    conversation_id: Option<&str>,
) -> &'a mut ConversationPermissionState {
    // Keep all permission decisions scoped by conversation, with a shared fallback scope.
    let scope = conversation_scope_key(conversation_id);
    // scope: 当前会话或全局 permission scope。
    state.conversations.entry(scope).or_default()
}

fn prune_expired_pending(state: &mut ConversationPermissionState) {
    // Expire old pending approvals to prevent stale request ids from being reused.
    let now = now_millis();
    // now: 当前时间毫秒。
    let mut expired_request_ids = Vec::new();
    // expired_request_ids: 需要删除的过期请求 id 列表。

    for (request_id, pending) in &state.pending {
        // request_id: pending map 的键；pending: 待审批数据。
        // saturating_sub 防止时钟回拨导致下溢。
        if now.saturating_sub(pending.created_at_ms) > PENDING_APPROVAL_TTL_MS {
            expired_request_ids.push(request_id.clone());
        }
    }

    for request_id in expired_request_ids {
        // request_id: 即将过期的待审批请求 id。
        // 两张索引表都要清理，避免 signature 指向已删除请求。
        if let Some(pending) = state.pending.remove(&request_id) {
            state.pending_by_signature.remove(&pending.operation.signature);
        }
    }
}

fn prune_processed_action_tokens(state: &mut ConversationPermissionState) {
    let now = now_millis();
    // now: 当前时间毫秒。
    state
        .processed_action_tokens
        // 仅保留 TTL 内 token，避免去重集合无限增长。
        .retain(|_, ts| {
            // ts: token 最后处理时间。
            now.saturating_sub(*ts) <= ACTION_TOKEN_TTL_MS
        });
}

fn upsert_pending_request_id(
    state: &mut ConversationPermissionState,
    operation: &ProtectedOperation,
) -> String {
    // Reuse an existing pending request for the same operation signature when possible.
    if let Some(existing_id) = state.pending_by_signature.get(&operation.signature).cloned() {
        // existing_id: 已记录的 request id。
        // signature -> request_id 命中且 request 仍在 pending，直接复用。
        if state.pending.contains_key(&existing_id) {
            return existing_id;
        }
        // 索引命中但主体缺失，说明是脏索引，先清掉再重建。
        state.pending_by_signature.remove(&operation.signature);
    }

    let request_id = next_request_id();
    // request_id: 新生成的审批请求 id。
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
    // 用统一分隔符与小写比较，减少跨平台路径写法差异。
    path.trim().replace('/', "\\").to_ascii_lowercase()
}

fn normalize_command_for_match(command: &str) -> String {
    command
        // 压缩空白，避免同义命令因空格差异得到不同签名。
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        // 统一小写，减少大小写差异干扰。
        .to_ascii_lowercase()
}

fn contains_shell_word(command: &str, target: &str) -> bool {
    command.split_whitespace().any(|token| {
        // token: 当前命令片段。
        // 去掉包裹在 token 两侧的标点，保留单词内部的 -/_。
        let cleaned = token.trim_matches(|c: char| {
            !c.is_ascii_alphanumeric() && c != '-' && c != '_'
        });
        // cleaned: 去除边界标点后的纯命令单词。
        // 这里做“完整单词”比较，避免误伤如 "rmdir" 对 "rm" 的包含。
        cleaned == target
    })
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect::<String>()
}

fn looks_like_shell_mcp(server: &str, tool: &str) -> bool {
    let s = format!("{} {}", server.to_ascii_lowercase(), tool.to_ascii_lowercase());
    // s: server+tool 的小写拼接字符串。
    ["bash", "shell", "powershell", "pwsh", "terminal"]
        .iter()
        // 关键字模糊匹配：适配不同 MCP server/tool 命名习惯。
        .any(|k| s.contains(k))
}

fn looks_like_file_mcp(server: &str, tool: &str) -> bool {
    let s = format!("{} {}", server.to_ascii_lowercase(), tool.to_ascii_lowercase());
    // s: server+tool 的小写拼接字符串。
    ["file", "filesystem", "fs", "write", "edit", "replace"]
        .iter()
        // 关键字命中即按文件写操作风控处理。
        .any(|k| s.contains(k))
}

fn pick_string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        // key: 当前尝试提取的字段名。
        if let Some(v) = value.get(*key).and_then(|v| v.as_str()) {
            // v: JSON 字段值。
            let trimmed = v.trim();
            // trimmed: 去掉前后空白后的字符串。
            if !trimmed.is_empty() {
                // 返回原始 JSON 字符串切片，零拷贝。
                return Some(trimmed);
            }
        }
    }
    None
}

fn check_mcp_operation(server: &str, tool: &str, arguments: &Value) -> Result<(), String> {
    if looks_like_shell_mcp(server, tool) {
        // 兼容不同 server 的参数命名。
        let command = pick_string_field(arguments, &["command", "cmd", "script"]).unwrap_or_default();
        // command: shell 操作中提取到的命令字符串。
        return check_command(command);
    }

    if looks_like_file_mcp(server, tool) {
        // 常见路径参数别名统一提取。
        let path = pick_string_field(
            arguments,
            &["path", "file", "file_path", "target", "target_path"],
        )
        .unwrap_or_default();
        // path: 文件操作中提取到的目标路径。
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
            // command: 原始 shell 命令文本。

            if command.is_empty() {
                return Some(ProtectedOperation {
                    signature: format!("{}:<empty>", tool_name),
                    preview: "命令为空".to_string(),
                    warning: Some("命令为空，无法执行。".to_string()),
                    needs_approval: false,
                });
            }

            let normalized = normalize_command_for_match(command);
            // normalized: 规范化后用于签名匹配的命令文本。
            // 只提取错误文本，不抛出错误，让外层走审批流程。
            let risk = check_command(command).err();

            Some(ProtectedOperation {
                // signature 用归一化命令，避免等价命令生成不同权限项。
                signature: format!("{}:{}", tool_name, normalized),
                preview: format!("{}: {}", tool_name, truncate_chars(command, 180)),
                warning: risk.clone(),
                needs_approval: risk.is_some(),
            })
        }
        "read_file" => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim();
            // path: 目标读取路径。

            if path.is_empty() {
                return Some(ProtectedOperation {
                    signature: "read_file:<empty>".to_string(),
                    preview: "路径为空".to_string(),
                    warning: Some("目标路径为空，无法执行。".to_string()),
                    needs_approval: false,
                });
            }

            let normalized = normalize_path_for_match(path);
            // normalized: 规范化后的路径签名。
            // 仅对敏感路径进行权限拦截（如 .ssh、.aws、系统目录等）。
            let risk = check_sensitive_read_path(path).err();

            Some(ProtectedOperation {
                signature: format!("read_file:{}", normalized),
                preview: format!("read_file: {}", truncate_chars(path, 200)),
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
            // path: 目标写入路径。

            if path.is_empty() {
                return Some(ProtectedOperation {
                    signature: format!("{}:<empty>", tool_name),
                    preview: "路径为空".to_string(),
                    warning: Some("目标路径为空，无法执行。".to_string()),
                    needs_approval: false,
                });
            }

            let normalized = normalize_path_for_match(path);
            // normalized: 规范化后的路径签名。
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
            // server: mcp_tool 的 server 名称。
            let tool = input
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim();
            // tool: mcp_tool 的具体工具名。
            let arguments = input
                .get("arguments")
                .cloned()
                // 缺省为空对象，避免后续字段提取分支处理 Option。
                .unwrap_or_else(|| json!({}));
            // arguments: MCP 工具参数对象。

            if server.is_empty() || tool.is_empty() {
                return Some(ProtectedOperation {
                    signature: "mcp_tool:<empty>".to_string(),
                    preview: "mcp_tool: server/tool 为空".to_string(),
                    warning: Some("mcp_tool 缺少 server 或 tool，无法执行。".to_string()),
                    needs_approval: false,
                });
            }

            let risk = check_mcp_operation(server, tool, &arguments).err();
            // risk: 该 MCP 操作的风险提示文本。

            Some(ProtectedOperation {
                signature: format!(
                    "mcp_tool:{}:{}:{}",
                    server.to_ascii_lowercase(),
                    tool.to_ascii_lowercase(),
                    // arguments 序列化后再归一化，作为近似去重签名。
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
    // context: 用户审批提示上下文。
    if let Some(w) = &operation.warning {
        // w: 风险提示文本。
        // 把规则命中的风险信息拼进上下文，便于用户做授权决策。
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
    // Parse approval tokens embedded in free-form user replies, e.g. ALLOW_ONCE::<id>.
    let mut out = Vec::new();
    // out: 提取到的动作 token 列表。
    for (prefix, action) in approval_action_tokens() {
        // prefix: token 前缀；action: 对应的权限动作。
        let mut cursor = 0usize;
        // cursor: 当前扫描位置。
        while let Some(rel) = text[cursor..].find(prefix) {
            // rel: 相对于 cursor 的前缀偏移。
            let start = cursor + rel + prefix.len();
            // start: token id 开始位置。
            let id = text[start..]
                .chars()
                // request_id 限制为安全字符集合，防止把后续自然语言吞进去。
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect::<String>();
            // id: 从 token 后面提取到的审批请求 id。
            if !id.is_empty() {
                let token = format!("{}{}", prefix, id);
                // token: 完整的审批 token 字符串。
                out.push((action, id, token));
            }
            // 从当前命中后的起点继续扫描，允许同一消息里提取多个 token。
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
    // pending: 找到的待审批请求，如果不存在则说明该 token 已失效。

    let signature = pending.operation.signature;
    // signature: 该操作的唯一归一化签名。
    state.pending_by_signature.remove(&signature);
    // 先移除旧决策，确保同一 signature 在三种集合里互斥。
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
            // blocks: 消息内容块列表。
            // 仅提取文本块，忽略图片/工具等非文本块。
            .filter_map(|b| {
                if let ContentBlock::Text { text } = b {
                    // text: 文本块内容。
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
    // Apply each action token at most once to make user decisions idempotent.
    let mut applied = 0usize;
    // applied: 成功应用的审批决策数。
    let mut guard = match permission_state().lock() {
        Ok(g) => g,
        Err(_) => return 0,
    };
    // guard: permission_state 的锁保护引用。

    let state = conversation_state_mut(&mut guard, conversation_id);
    // state: 当前会话的权限状态。
    prune_expired_pending(state);
    prune_processed_action_tokens(state);

    for message in messages {
        // message: 逐条处理输入消息。
        // 只有用户消息允许携带审批指令。
        if message.role != Role::User {
            continue;
        }

        for text in extract_text_from_message(message) {
            // text: 当前消息中的文本片段。
            for (action, request_id, token) in extract_action_tokens(&text) {
                // action: 用户选择的权限动作。
                // request_id: 审批请求 id。
                // token: 完整审批 token 字符串。
                // 同一个 token 只消费一次，避免历史消息重复应用。
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
    // normalized: 规范化后用于风险检测的命令文本。
    if normalized.is_empty() {
        return Err("Blocked by permission gate: command is empty".to_string());
    }

    for dangerous_word in ["rm", "del", "remove-item"] {
        // dangerous_word: 当前检查的危险关键字。
        // 先做单词级命中，拦截最常见的删除命令。
        if contains_shell_word(&normalized, dangerous_word) {
            return Err(format!(
                "Blocked by permission gate: command contains dangerous shell command '{}'. Set NOVA_ALLOW_UNSAFE_TOOLS=1 only for trusted debugging.",
                dangerous_word
            ));
        }
    }

    for pattern in DANGEROUS_COMMAND_PATTERNS {
        // pattern: 当前检查的危险命令片段。
        // 再做模式级命中，覆盖参数组合等高危片段。
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
    // normalized: 规范化后用于路径风险匹配的路径字符串。
    if normalized.is_empty() {
        return Err("Blocked by permission gate: target path is empty".to_string());
    }

    for prefix in PROTECTED_PATH_PREFIXES {
        // prefix: 当前检查的受保护路径前缀。
        // 前缀命中用于阻止系统目录写入。
        if normalized.starts_with(prefix) {
            return Err(format!(
                "Blocked by permission gate: writing protected path '{}'. Set NOVA_ALLOW_UNSAFE_TOOLS=1 only for trusted debugging.",
                path
            ));
        }
    }

    for marker in PROTECTED_PATH_CONTAINS {
        // marker: 当前检查的敏感路径标记。
        // contains 命中用于阻止凭据/密钥等敏感目录。
        if normalized.contains(marker) {
            return Err(format!(
                "Blocked by permission gate: writing sensitive path '{}'. Set NOVA_ALLOW_UNSAFE_TOOLS=1 only for trusted debugging.",
                path
            ));
        }
    }

    Ok(())
}

fn check_sensitive_read_path(path: &str) -> Result<(), String> {
    let normalized = normalize_path_for_match(path);
    // normalized: 规范化后用于读取路径风险匹配的路径字符串。
    if normalized.is_empty() {
        return Err("Blocked by permission gate: target path is empty".to_string());
    }

    // 读取操作仅对包含凭据/密钥的敏感目录进行拦截，不阻止系统目录的普通读取。
    for marker in PROTECTED_PATH_CONTAINS {
        // marker: 敏感路径标记（如 .ssh、.aws、.gnupg、.git/config 等）。
        if normalized.contains(marker) {
            return Err(format!(
                "Blocked by permission gate: reading sensitive path '{}'. Set NOVA_ALLOW_UNSAFE_TOOLS=1 only for trusted debugging.",
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
    // Decision order: unsafe override > deny cache > session allow > one-time allow > ask user.
    if unsafe_override_enabled() {
        // 显式调试开关打开时直接放行，不进入任何审批状态机。
        return PermissionEnforcement::Allow;
    }

    let Some(operation) = operation_from_input(tool_name, input) else {
        // 非受控工具默认放行。
        return PermissionEnforcement::Allow;
    };
    // operation: 当前待评估的受控操作。

    let mut guard = match permission_state().lock() {
        Ok(g) => g,
        Err(_) => {
            return PermissionEnforcement::Deny(
                "Permission state unavailable due to lock poisoning".to_string(),
            )
        }
    };
    // guard: 全局 permission state 的锁引用。

    let state = conversation_state_mut(&mut guard, conversation_id);
    // state: 当前 conversation 的权限状态。
    prune_expired_pending(state);

    if state.deny_session.contains(&operation.signature) {
        // 会话级拒绝优先级最高，直接阻断。
        return PermissionEnforcement::Deny(format!(
            "Blocked by permission gate: this operation was denied in current session ({})",
            operation.preview
        ));
    }

    if state.allow_session.contains(&operation.signature) {
        // 会话级允许可重复使用。
        return PermissionEnforcement::Allow;
    }

    if state.allow_once.remove(&operation.signature) {
        // 一次性允许命中后立即消费，确保只生效一次。
        return PermissionEnforcement::Allow;
    }

    if operation.needs_approval {
        let request_id = upsert_pending_request_id(state, &operation);
        // request_id: 生成或复用的待审批请求 id。
        return PermissionEnforcement::AskUser(build_permission_prompt_payload(
            &request_id,
            &operation,
        ));
    }

    PermissionEnforcement::Allow
}
