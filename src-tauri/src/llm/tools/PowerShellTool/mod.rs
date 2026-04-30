use crate::llm::tools::{sync_tool, ToolPermissionDescriptor, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};

pub(crate) fn registration() -> ToolRegistration {
    sync_tool(tool, execute, false, Some(permission))
}

fn permission(input: &Value) -> Option<ToolPermissionDescriptor> {
    // PowerShellTool 和 BashTool 一样，由工具自己提供命令级权限描述。
    crate::llm::utils::permissions::describe_shell_command_permission(
        "execute_powershell",
        "PowerShell 命令",
        input,
    )
}

pub fn tool() -> Tool {
    Tool {
        name: "execute_powershell".into(),
        description: "Execute a PowerShell command on Windows.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "PowerShell command text" }
            },
            "required": ["command"]
        }),
    }
}

pub fn execute(input: Value) -> String {
    // cmd: 用户或模型传入的 PowerShell 命令文本。
    let cmd = match input.get("command").and_then(|v| v.as_str()) {
        Some(v) if !v.trim().is_empty() => v,
        _ => return "Error: Missing 'command' argument".into(),
    };

    #[cfg(target_os = "windows")]
    {
        let out = crate::llm::tools::process::run_hidden_pwsh(cmd);

        return match out {
            Ok(output) => {
                // stdout/stderr: PowerShell 子进程执行结果。
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                if output.status.success() {
                    if stdout.trim().is_empty() {
                        "(command executed with no output)".into()
                    } else {
                        stdout
                    }
                } else {
                    format!("Error: {}\nStdout: {}", stderr, stdout)
                }
            }
            Err(e) => format!("Failed to execute PowerShell command: {}", e),
        };
    }

    #[cfg(not(target_os = "windows"))]
    {
        format!("Error: execute_powershell is only available on Windows. Command was: {}", cmd)
    }
}
