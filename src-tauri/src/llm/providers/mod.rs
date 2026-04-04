// Anthropic provider 实现。
pub mod anthropic;
// OpenAI provider 实现。
pub mod openai;

use tauri::AppHandle;
use crate::llm::types::Message;

#[derive(Debug, Clone)]
pub struct ProviderTurnResult {
    // 本轮生成并需要并入上下文的消息。
    pub messages: Vec<Message>,
    // 本轮停止原因（可选）。
    pub stop_reason: Option<String>,
    // provider 上报的输出 token 数（可选）。
    pub output_tokens: Option<u32>,
    // 是否阻止 query 层继续发起下一轮。
    pub prevent_continuation: bool,
}

pub enum LlmProvider {
    // Anthropic provider 分支。
    Anthropic(anthropic::AnthropicProvider),
    // OpenAI provider 分支。
    OpenAi(openai::OpenAiProvider),
}

impl LlmProvider {
    pub fn new(app: &AppHandle) -> Self {
        // 读取运行时设置。
        let settings = crate::command::settings::get_settings(app.clone());
        // provider 名统一转小写做匹配。
        let provider = settings.provider.to_lowercase();
        
        // anthropic/claude 都路由到 AnthropicProvider。
        if provider == "anthropic" || provider == "claude" {
            LlmProvider::Anthropic(anthropic::AnthropicProvider)
        } else {
            // 其余 provider 名统一走 OpenAI 协议实现。
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
        // 根据当前枚举分支转发到具体 provider 实现。
        match self {
            LlmProvider::Anthropic(p) => p.send_request(app, messages, plan_mode, conversation_id).await,
            LlmProvider::OpenAi(p) => p.send_request(app, messages, plan_mode, conversation_id).await,
        }
    }
}