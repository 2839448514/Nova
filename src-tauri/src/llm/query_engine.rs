use reqwest::Client;
use serde::Serialize;
use tauri::AppHandle;

use crate::llm::services::{compact, mcp_tools, streaming};
use crate::llm::tools;
use crate::llm::types::{AnthropicRequest, Content, ContentBlock, Message};
use crate::llm::utils::system_prompt::load_system_prompt;

#[derive(Debug, Serialize, Clone)]
pub struct ChatMessageEvent {
    pub r#type: String,
    pub text: Option<String>,
    pub tool_use_id: Option<String>,
    pub tool_use_name: Option<String>,
    pub tool_use_input: Option<String>,
    pub tool_result: Option<String>,
    pub token_usage: Option<u32>,
    pub stop_reason: Option<String>,
    pub turn_state: Option<String>,
}

pub async fn send_request(
    app: &AppHandle,
    messages: &[Message],
    plan_mode: bool,
) -> Result<Vec<Message>, String> {
    let settings = crate::command::settings::get_settings(app.clone());
    let api_key = settings.api_key;

    if api_key.is_empty() {
        return Err("API error: No API key configured. Please set it in Settings.".to_string());
    }

    let mut available_tools = tools::get_available_tools();
    available_tools.extend(mcp_tools::collect_mcp_tools(app).await);

    let request = AnthropicRequest {
        model: settings.model,
        max_tokens: 4096,
        system: Some(load_system_prompt(app, plan_mode)),
        messages: messages.to_vec(),
        tools: available_tools,
        stream: true,
    };

    let client = Client::new();
    let mut url = settings.base_url.trim_end_matches('/').to_string();
    if !url.ends_with("/v1/messages") && !url.ends_with("/messages") {
        if url.ends_with("/v1") {
            url = format!("{}/messages", url);
        } else {
            url = format!("{}/v1/messages", url);
        }
    }

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await;

    match resp {
        Ok(res) => {
            if !res.status().is_success() {
                let status = res.status();
                let error_text = res.text().await.unwrap_or_default();
                eprintln!("API Error: {}", error_text);
                return Err(format!("API Error [{}] {} => {}", status, url, error_text));
            }

            streaming::process_stream_response(app, res).await
        }
        Err(e) => Err(e.to_string()),
    }
}

pub async fn send_chat_message(
    app: AppHandle,
    conversation_id: Option<String>,
    messages: Vec<Message>,
    plan_mode: bool,
) -> Result<(), String> {
    let mut current_messages =
        compact::prepare_messages_for_turn(&app, conversation_id.as_deref(), &messages).await;

    loop {
        let new_messages = send_request(&app, &current_messages, plan_mode).await?;
        current_messages.extend(new_messages.clone());

        let has_tool_result = new_messages.iter().any(|m| {
            if let Content::Blocks(blocks) = &m.content {
                blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }))
            } else {
                false
            }
        });

        if compact::has_needs_user_input(&new_messages) {
            break;
        }

        if !has_tool_result {
            break;
        }
    }

    Ok(())
}
