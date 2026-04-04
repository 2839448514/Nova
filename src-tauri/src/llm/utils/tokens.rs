//export function tokenCountWithEstimation(messages: readonly Message[]): number {
pub fn tokenCountWithEstimation(messages: &[crate::llm::types::Message]) -> usize {
    // For simplicity, we use a very rough estimation: 1 token ~= 4 characters in English.
    // This is a common heuristic but can vary based on the actual content and language.
    // 遍历所有消息并累加文本字符数。
    let char_count: usize = messages.iter().map(|m| m.content.text.chars().count()).sum();
    // 使用 4 字符约等于 1 token 的经验规则估算 token 数。
    char_count / 4
}
















