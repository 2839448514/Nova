use crate::llm::tools::skill_tool::{list_skill_summaries_with_app, SkillSummary};
use tauri::AppHandle;

#[tauri::command]
pub fn list_skills(app: AppHandle) -> Result<Vec<SkillSummary>, String> {
    // 返回技能摘要列表。
    list_skill_summaries_with_app(&app)
}
