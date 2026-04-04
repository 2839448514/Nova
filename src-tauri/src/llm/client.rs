use tauri::AppHandle;

use crate::llm::types::Message;

// 对外复用 query_engine 的事件类型定义。
pub use crate::llm::query_engine::ChatMessageEvent;

#[tauri::command]
pub async fn send_chat_message(
    app: AppHandle,
    conversation_id: Option<String>,
    messages: Vec<Message>,
    plan_mode: Option<bool>,
) -> Result<(), String> {
    // 克隆会话 ID，便于请求前后使用同一作用域 key。
    let conversation_scope = conversation_id.clone();
    // 标记本轮开始，初始化取消标志位。
    crate::llm::cancellation::begin_turn(conversation_scope.as_deref());

    // 通过兼容层入口发送请求；plan_mode 缺省为 false。
    let result = crate::llm::query_engine::send_chat_message(
        app,
        conversation_id,
        messages,
        plan_mode.unwrap_or(false),
    )
    .await;

    // 无论请求成功失败都结束本轮，清理取消状态。
    crate::llm::cancellation::finish_turn(conversation_scope.as_deref());
    // 返回下游执行结果。
    result
}

#[tauri::command]
pub async fn cancel_chat_message(conversation_id: Option<String>) -> Result<bool, String> {
    // 提交取消请求并返回是否成功命中运行中的会话。
    Ok(crate::llm::cancellation::request_cancel(
        conversation_id.as_deref(),
    ))
}
