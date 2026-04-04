use crate::llm::tools::skill_tool::{list_skill_summaries, SkillSummary};

#[tauri::command]
pub fn list_skills() -> Vec<SkillSummary> {
    // 返回技能摘要列表。
    list_skill_summaries()
}
