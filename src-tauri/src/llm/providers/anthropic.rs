use futures_util::StreamExt;
use reqwest::Client;
use tauri::{AppHandle, Emitter};

use crate::llm::query_engine::ChatMessageEvent;
use crate::llm::services::mcp_tools;
use crate::llm::services::mcp_tools::parse_mcp_tool_name;
use crate::llm::tools;
use crate::llm::types::{
    AnthropicRequest, ContentBlock, Message, Role, StreamContentBlock, StreamDelta, StreamEvent,
};
use crate::llm::utils::system_prompt::load_system_prompt;

pub struct AnthropicProvider;

fn is_needs_user_input_payload(raw: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| {
            v.get("type")
                .and_then(|t| t.as_str())
                .map(|s| s == "needs_user_input")
        })
        .unwrap_or(false)
}

impl AnthropicProvider {
    pub async fn send_request(
        &self,
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
            model: settings.model.clone(),
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

                self.process_stream_response(app, res).await
            }
            Err(e) => Err(e.to_string()),
        }
    }

    async fn process_stream_response(
        &self,
        app: &AppHandle,
        response: reqwest::Response,
    ) -> Result<Vec<Message>, String> {
    let mut stream = response.bytes_stream();
    let mut current_tool_id = None;
    let mut current_tool_name = None;
    let mut current_tool_input = String::new();
    let mut generated_text = String::new();
    let mut output_blocks: Vec<ContentBlock> = Vec::new();
    let mut tool_result_blocks: Vec<ContentBlock> = Vec::new();
    let mut emitted_stop = false;
    let mut current_output_tokens: Option<u32> = None;
    let mut stop_emitted_for_user_input = false;
    let mut last_stop_reason: Option<String> = None;

    while let Some(chunk) = stream.next().await {
        if let Ok(bytes) = chunk {
            let text = String::from_utf8_lossy(&bytes);
            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim_start();
                    if data == "[DONE]" {
                        break;
                    }
                    if let Ok(event) = serde_json::from_str::<StreamEvent>(data) {
                        match event {
                            StreamEvent::ContentBlockStart { content_block, .. } => {
                                if let StreamContentBlock::ToolUse { id, name, .. } = content_block {
                                    current_tool_id = Some(id.clone());
                                    current_tool_name = Some(name.clone());
                                    current_tool_input.clear();
                                    app.emit(
                                        "chat-stream",
                                        ChatMessageEvent {
                                            r#type: "tool-use-start".into(),
                                            text: None,
                                            tool_use_id: Some(id),
                                            tool_use_name: Some(name),
                                            tool_use_input: None,
                                            tool_result: None,
                                            token_usage: None,
                                            stop_reason: None,
                                            turn_state: Some("tool_running".into()),
                                        },
                                    )
                                    .ok();
                                }
                            }
                            StreamEvent::ContentBlockDelta { delta, .. } => match delta {
                                StreamDelta::TextDelta { text } => {
                                    generated_text.push_str(&text);
                                    app.emit(
                                        "chat-stream",
                                        ChatMessageEvent {
                                            r#type: "text".into(),
                                            text: Some(text),
                                            tool_use_id: None,
                                            tool_use_name: None,
                                            tool_use_input: None,
                                            tool_result: None,
                                            token_usage: None,
                                            stop_reason: None,
                                            turn_state: Some("streaming_text".into()),
                                        },
                                    )
                                    .ok();
                                }
                                StreamDelta::InputJsonDelta { partial_json } => {
                                    current_tool_input.push_str(&partial_json);
                                    app.emit(
                                        "chat-stream",
                                        ChatMessageEvent {
                                            r#type: "tool-json-delta".into(),
                                            text: None,
                                            tool_use_id: current_tool_id.clone(),
                                            tool_use_name: None,
                                            tool_use_input: Some(partial_json),
                                            tool_result: None,
                                            token_usage: None,
                                            stop_reason: None,
                                            turn_state: Some("tool_input_streaming".into()),
                                        },
                                    )
                                    .ok();
                                }
                            },
                            StreamEvent::ContentBlockStop { .. } => {
                                if let (Some(id), Some(name)) =
                                    (current_tool_id.take(), current_tool_name.take())
                                {
                                    let input_value: serde_json::Value =
                                        serde_json::from_str(&current_tool_input)
                                            .unwrap_or_else(|_| serde_json::json!({}));

                                    output_blocks.push(ContentBlock::ToolUse {
                                        id: id.clone(),
                                        name: name.clone(),
                                        input: input_value.clone(),
                                    });

                                    let tool_result_str =
                                        if let Some((server_name, tool_name)) =
                                            parse_mcp_tool_name(&name)
                                        {
                                            match crate::command::mcp::call_mcp_tool(
                                                app.clone(),
                                                server_name,
                                                tool_name,
                                                input_value,
                                            )
                                            .await
                                            {
                                                Ok(v) => v.to_string(),
                                                Err(e) => {
                                                    serde_json::json!({ "ok": false, "error": e })
                                                        .to_string()
                                                }
                                            }
                                        } else {
                                            tools::execute_tool_with_app(app, &name, input_value).await
                                        };

                                    app.emit(
                                        "chat-stream",
                                        ChatMessageEvent {
                                            r#type: "tool-result".into(),
                                            text: None,
                                            tool_use_id: Some(id.clone()),
                                            tool_use_name: Some(name),
                                            tool_use_input: None,
                                            tool_result: Some(tool_result_str.clone()),
                                            token_usage: current_output_tokens,
                                            stop_reason: None,
                                            turn_state: Some("tool_completed".into()),
                                        },
                                    )
                                    .ok();

                                    let needs_user_input =
                                        is_needs_user_input_payload(&tool_result_str);

                                    if needs_user_input && !stop_emitted_for_user_input {
                                        stop_emitted_for_user_input = true;
                                        app.emit(
                                            "chat-stream",
                                            ChatMessageEvent {
                                                r#type: "stop".into(),
                                                text: None,
                                                tool_use_id: None,
                                                tool_use_name: None,
                                                tool_use_input: None,
                                                tool_result: None,
                                                token_usage: current_output_tokens,
                                                stop_reason: Some("needs_user_input".into()),
                                                turn_state: Some("awaiting_user_input".into()),
                                            },
                                        )
                                        .ok();
                                    }

                                    tool_result_blocks.push(ContentBlock::ToolResult {
                                        tool_use_id: id,
                                        is_error: false,
                                        content: vec![ContentBlock::Text {
                                            text: tool_result_str,
                                        }],
                                    });
                                } else if !generated_text.is_empty() {
                                    output_blocks.push(ContentBlock::Text {
                                        text: generated_text.clone(),
                                    });
                                    generated_text.clear();
                                }
                            }
                            StreamEvent::MessageDelta { delta, usage } => {
                                if let Some(reason) = delta.stop_reason.clone() {
                                    last_stop_reason = Some(reason);
                                }
                                current_output_tokens = Some(usage.output_tokens);
                                app.emit(
                                    "chat-stream",
                                    ChatMessageEvent {
                                        r#type: "token-usage".into(),
                                        text: None,
                                        tool_use_id: None,
                                        tool_use_name: None,
                                        tool_use_input: None,
                                        tool_result: None,
                                        token_usage: current_output_tokens,
                                        stop_reason: last_stop_reason.clone(),
                                        turn_state: Some("streaming".into()),
                                    },
                                )
                                .ok();
                            }
                            StreamEvent::MessageStop => {
                                emitted_stop = true;
                                app.emit(
                                    "chat-stream",
                                    ChatMessageEvent {
                                        r#type: "stop".into(),
                                        text: None,
                                        tool_use_id: None,
                                        tool_use_name: None,
                                        tool_use_input: None,
                                        tool_result: None,
                                        token_usage: current_output_tokens,
                                        stop_reason: last_stop_reason.clone(),
                                        turn_state: Some("completed".into()),
                                    },
                                )
                                .ok();
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    if !emitted_stop {
        app.emit(
            "chat-stream",
            ChatMessageEvent {
                r#type: "stop".into(),
                text: None,
                tool_use_id: None,
                tool_use_name: None,
                tool_use_input: None,
                tool_result: None,
                token_usage: current_output_tokens,
                stop_reason: last_stop_reason.clone(),
                turn_state: Some("completed".into()),
            },
        )
        .ok();
    }

    let mut result_messages = vec![Message {
        role: Role::Assistant,
        content: crate::llm::types::Content::Blocks(output_blocks),
    }];

    if !tool_result_blocks.is_empty() {
        result_messages.push(Message {
            role: Role::User,
            content: crate::llm::types::Content::Blocks(tool_result_blocks),
        });
    }

    Ok(result_messages)
}
}
