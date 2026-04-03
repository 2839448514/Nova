

//export function tokenCountWithEstimation(messages: readonly Message[]): number {
pub fn tokenCountWithEstimation(messages: &[crate::llm::types::Message]) -> usize {
    // For simplicity, we use a very rough estimation: 1 token ~= 4 characters in English.
    // This is a common heuristic but can vary based on the actual content and language.
    let char_count: usize = messages.iter().map(|m| m.content.text.chars().count()).sum();
    char_count / 4
}









