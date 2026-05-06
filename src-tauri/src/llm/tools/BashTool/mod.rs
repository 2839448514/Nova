use crate::llm::tools::{app_tool, AppExecuteFuture, ToolPermissionDescriptor, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};
use tauri::AppHandle;

// 返回 BashTool 的注册信息。
pub(crate) fn registration() -> ToolRegistration {
    app_tool(tool, execute_sync_stub, execute_with_app_boxed, false, Some(permission))
}

fn permission(input: &Value) -> Option<ToolPermissionDescriptor> {
    crate::llm::utils::permissions::describe_shell_command_permission(
        "execute_bash",
        "终端命令",
        input,
    )
}

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

// execute_with_app 优先；此路径仅在脱离 AppHandle 的同步调用链中触发。
pub fn execute_sync_stub(_input: Value) -> String {
    json!({ "ok": false, "error": "BashTool requires async execution context" }).to_string()
}

fn execute_with_app_boxed(
    _app: AppHandle,
    conversation_id: Option<String>,
    input: Value,
) -> AppExecuteFuture {
    Box::pin(async move { execute_async(conversation_id.as_deref(), input).await })
}

async fn execute_async(conversation_id: Option<&str>, input: Value) -> String {
    let cmd = match input.get("command").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
        None => return "Error: Missing 'command' argument".into(),
    };

    #[cfg(target_os = "windows")]
    let mut child = {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const PWSH_PATH: &str = "C:\\Program Files\\PowerShell\\7\\pwsh.exe";
        let wrapped = format!(
            "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; {}",
            cmd
        );
        match tokio::process::Command::new(PWSH_PATH)
            .args(["-NoLogo", "-NoProfile", "-Command", &wrapped])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return format!("Failed to execute command: {}", e),
        }
    };

    #[cfg(not(target_os = "windows"))]
    let mut child = {
        match tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return format!("Failed to execute command: {}", e),
        }
    };

    // stdout/stderr を独立タスクで収集。wait() は &mut self なので child を消費しない。
    let stdout_reader = child.stdout.take();
    let stderr_reader = child.stderr.take();
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut r) = stdout_reader {
            use tokio::io::AsyncReadExt;
            let _ = r.read_to_end(&mut buf).await;
        }
        buf
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut r) = stderr_reader {
            use tokio::io::AsyncReadExt;
            let _ = r.read_to_end(&mut buf).await;
        }
        buf
    });

    // child.wait() 取 &mut self，不消费所有权，select! 另一臂可在事后 kill。
    let status_result = tokio::select! {
        s = child.wait() => Some(s),
        _ = async {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if crate::llm::cancellation::is_cancelled(conversation_id) { break; }
            }
        } => None,
    };

    match status_result {
        Some(status_result) => {
            let stdout_bytes = stdout_task.await.unwrap_or_default();
            let stderr_bytes = stderr_task.await.unwrap_or_default();
            match status_result {
                Ok(status) => {
                    let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
                    let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
                    if status.success() { stdout }
                    else { format!("Error: {}\nStdout: {}", stderr, stdout) }
                }
                Err(e) => format!("Failed to execute command: {}", e),
            }
        }
        None => {
            stdout_task.abort();
            stderr_task.abort();
            let _ = child.kill().await;
            json!({ "ok": false, "error": "cancelled" }).to_string()
        }
    }
}

