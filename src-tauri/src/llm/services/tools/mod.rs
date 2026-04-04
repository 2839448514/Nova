// Compatibility shim: the hook runtime moved to services/hooks.
// Keep this re-export so existing imports continue to compile during migration.
pub use crate::llm::services::hooks::*;
