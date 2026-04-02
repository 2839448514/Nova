use crate::llm::tools::skill_tool::{list_skill_summaries, SkillSummary};

#[tauri::command]
pub fn list_skills() -> Vec<SkillSummary> {
    list_skill_summaries()
}
