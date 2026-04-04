// compact 相关命令：构建/记录压缩上下文边界。
pub mod compact;
// memory 相关命令：会话摘要与关键事实维护。
pub mod memory;
// resume 相关命令：基于 compact 边界恢复上下文。
pub mod resume;
// 公共类型定义：供 command 与上层模块复用。
pub mod types;
