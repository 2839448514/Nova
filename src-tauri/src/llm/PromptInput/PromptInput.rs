




// 处理提交逻辑
pub async fn on_submit(
    input_param: String,
    is_submitting_slash_command: Option<bool>,
) -> Result<(), String> {
    let is_submitting_slash_command = is_submitting_slash_command.unwrap_or(false);
    // ...
}