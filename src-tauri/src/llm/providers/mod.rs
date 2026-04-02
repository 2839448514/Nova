pub mod anthropic;
pub mod openai;

use tauri::AppHandle;
use crate::llm::types::Message;

pub enum LlmProvider {
    Anthropic(anthropic::AnthropicProvider),
    OpenAi(openai::OpenAiProvider),
}

impl LlmProvider {
    pub fn new(app: &AppHandle) -> Self {
        let settings = crate::command::settings::get_settings(app.clone());
        let provider = settings.provider.to_lowercase();
        
        if provider == "anthropic" || provider == "claude" {
            LlmProvider::Anthropic(anthropic::AnthropicProvider)
        } else {
            LlmProvider::OpenAi(openai::OpenAiProvider)
        }
    }

    pub async fn send_request(
        &self,
        app: &AppHandle,
        messages: &[Message],
        plan_mode: bool,
    ) -> Result<Vec<Message>, String> {
        match self {
            LlmProvider::Anthropic(p) => p.send_request(app, messages, plan_mode).await,
            LlmProvider::OpenAi(p) => p.send_request(app, messages, plan_mode).await,
        }
    }
}