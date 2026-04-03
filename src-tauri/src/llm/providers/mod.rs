pub mod anthropic;
pub mod openai;

use tauri::AppHandle;
use crate::llm::types::Message;

#[derive(Debug, Clone)]
pub struct ProviderTurnResult {
    pub messages: Vec<Message>,
    pub stop_reason: Option<String>,
}

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
        conversation_id: Option<&str>,
    ) -> Result<ProviderTurnResult, String> {
        match self {
            LlmProvider::Anthropic(p) => p.send_request(app, messages, plan_mode, conversation_id).await,
            LlmProvider::OpenAi(p) => p.send_request(app, messages, plan_mode, conversation_id).await,
        }
    }
}