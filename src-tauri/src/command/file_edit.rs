use std::fs;

/// 打开原生文件夹选择对话框，让用户选取 Coding 模式的工作区目录。
/// 返回所选路径字符串，用户取消时返回 None。
#[tauri::command]
pub async fn pick_coding_workspace() -> Result<Option<String>, String> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("选择 Coding 工作区目录")
        .pick_folder()
        .await;
    Ok(handle.map(|h| h.path().to_string_lossy().to_string()))
}

/// 撤回 replace_string_in_file 操作：将文件中首个 `new_string` 替换回 `old_string`。
#[tauri::command]
pub async fn revert_file_edit(
    path: String,
    old_string: String,
    new_string: String,
) -> Result<(), String> {
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("无法读取文件 `{path}`: {e}"))?;

    if !content.contains(&new_string) {
        return Err("文件中未找到目标内容，可能已被修改或已撤回".into());
    }

    let reverted = content.replacen(&new_string, &old_string, 1);

    fs::write(&path, reverted)
        .map_err(|e| format!("写入文件失败: {e}"))?;

    Ok(())
}

/// 撤回 write_file 操作：从 sidecar 快照恢复原始内容，或删除新建的文件。
#[tauri::command]
pub async fn revert_write_file(path: String) -> Result<(), String> {
    const NEW_FILE_MARKER: &str = "\x00NOVA_NEW_FILE\x00";
    let sidecar = format!("{}.nova-snapshot", path);

    let snapshot = fs::read_to_string(&sidecar)
        .map_err(|_| "未找到快照文件，无法撤回（可能此写入操作发生在快照功能启用之前）".to_string())?;

    if snapshot == NEW_FILE_MARKER {
        // 文件是由 write_file 新建的，撤回 = 删除
        fs::remove_file(&path)
            .map_err(|e| format!("删除文件失败: {e}"))?;
    } else {
        // 恢复原始内容
        fs::write(&path, &snapshot)
            .map_err(|e| format!("恢复文件失败: {e}"))?;
    }

    // 清理快照
    let _ = fs::remove_file(&sidecar);
    Ok(())
}

/// 接受 write_file 操作：删除 sidecar 快照文件（清理）。
#[tauri::command]
pub async fn accept_write_file(path: String) -> Result<(), String> {
    let sidecar = format!("{}.nova-snapshot", path);
    let _ = fs::remove_file(&sidecar);
    Ok(())
}

