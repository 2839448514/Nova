// llm/utils 模块入口：负责 Nova 的 LLM 运行时辅助功能。
// 这里按职责拆分为不同子模块，并在上层通过 `use crate::llm::utils::*` 进行调用。

// 加载系统提示 (system prompt)，包括 plan mode 附加内容。
pub mod system_prompt;

// 工具权限管理、用户鉴权、审批状态存储，和 tool 执行前检查紧密关联。
pub mod permissions;

// 对话会话恢复逻辑：构建被插入到 current_messages 中的恢复上下文。
pub mod session_restore;

// 统一后端错误事件输出到前端 telemetry 和 toast。
pub mod error_event;
