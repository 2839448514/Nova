use crate::llm::tools::{sync_tool, ToolPermissionDescriptor, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};
#[cfg(not(target_os = "windows"))]
use std::process::Command;

// 返回 BashTool 的注册信息。
// 这里把 `tool`/`execute`/`permission` 绑定到统一注册表里，并声明 `read_only=false`，
// 表示这个工具会真正执行命令，不能放进只读并发队列。
pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, false, Some(permission))
}

// 从 input 里读取 `command`，生成本次命令执行对应的权限描述。
// 返回的 descriptor 会告诉权限层：
// `signature` 用什么字符串做去重，`preview` 给用户展示什么摘要，`warning` 提示什么风险。
fn permission(input: &Value) -> Option<ToolPermissionDescriptor> {
    // "execute_bash" 用来生成这次操作的稳定签名；
    // "终端命令" 会显示在权限弹窗里；
    // input 里会读取 command 字段，用它判断风险并构造 preview/warning。
    crate::llm::utils::permissions::describe_shell_command_permission(
        "execute_bash",
        "终端命令",
        input,
    )
}

// 返回暴露给模型的静态元数据。
// 模型只能看到这里定义的 `name`、`description` 和 `input_schema`，
// 并据此决定什么时候调用 `execute_bash`。
pub fn tool() -> Tool {
    Tool {
        name: "execute_bash".into(),
        description: "Execute a bash or powershell command on the host machine.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command to execute" }
            },
            "required": ["command"]
        }),
    }
}

// 执行 input 里的 `command` 字符串，并把执行结果转换成返回给模型的文本。
// `input` 是模型传来的 JSON 参数；这里只关心其中的 `command` 字段。
pub fn execute(input: Value) -> String {
    if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
        // cmd: 模型请求执行的原始命令文本。
        #[cfg(target_os = "windows")]
        let out = crate::llm::tools::process::run_hidden_pwsh(cmd);

        #[cfg(not(target_os = "windows"))]
        let out = Command::new("sh").arg("-c").arg(cmd).output();

        match out {
            Ok(output) => {
                // stdout/stderr: 子进程标准输出与错误输出，统一转成字符串回给模型。
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                if output.status.success() {
                    stdout
                } else {
                    format!("Error: {}\nStdout: {}", stderr, stdout)
                }
            }
            Err(e) => format!("Failed to execute command: {}", e),
        }
    } else {
        "Error: Missing 'command' argument".into()
    }
}
