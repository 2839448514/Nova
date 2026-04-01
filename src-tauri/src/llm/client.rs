use reqwest::Client;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use futures_util::StreamExt;

use crate::llm::types::{AnthropicRequest, StreamEvent, Message, Content, ContentBlock, Role};
use crate::llm::tools;

#[derive(Debug, Serialize, Clone)]
pub struct ChatMessageEvent {
    pub r#type: String,
    pub text: Option<String>,
    pub tool_use_id: Option<String>,
    pub tool_use_name: Option<String>,
    pub tool_use_input: Option<String>, // Partial JSON string
    pub tool_result: Option<String>, // Result from tool execution
    pub token_usage: Option<u32>,
}

pub async fn send_request(
    app: &AppHandle,
    messages: &Vec<Message>,
) -> Result<Vec<Message>, String> {
    let settings = crate::command::settings::get_settings(app.clone());
    let api_key = settings.api_key;
    
    if api_key.is_empty() {
        return Err("API error: No API key configured. Please set it in Settings.".to_string());
    }

    let request = AnthropicRequest {
        model: settings.model,
        max_tokens: 4096,
        system: Some("You are a helpful coding assistant running in a local Tauri desktop app. You will answer questions briefly and write accurate code.".to_string()),
        messages: messages.clone(),
        tools: tools::get_available_tools(),
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
        .header("Authorization", format!("Bearer {}", api_key)) // Adding generic authorization, some APIs like Qwen/OpenAI use this
        .header("x-api-key", &api_key) // Anthropic custom header
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

            let mut stream = res.bytes_stream();
            let mut current_tool_id = None;
            let mut current_tool_name = None;
            let mut current_tool_input = String::new();
            let mut generated_text = String::new();
            let mut output_blocks: Vec<ContentBlock> = Vec::new();
            let mut emitted_stop = false;
            let mut current_output_tokens: Option<u32> = None;

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
                                        match content_block {
                                            crate::llm::types::StreamContentBlock::ToolUse { id, name, .. } => {
                                                current_tool_id = Some(id.clone());
                                                current_tool_name = Some(name.clone());
                                                current_tool_input.clear();
                                                app.emit("chat-stream", ChatMessageEvent {
                                                    r#type: "tool-use-start".into(),
                                                    text: None,
                                                    tool_use_id: Some(id),
                                                    tool_use_name: Some(name),
                                                    tool_use_input: None,
                                                    tool_result: None,
                                                    token_usage: None,
                                                }).ok();
                                            }
                                            _ => {}
                                        }
                                    }
                                    StreamEvent::ContentBlockDelta { delta, .. } => {
                                        match delta {
                                            crate::llm::types::StreamDelta::TextDelta { text } => {
                                                generated_text.push_str(&text);
                                                app.emit("chat-stream", ChatMessageEvent {
                                                    r#type: "text".into(),
                                                    text: Some(text),
                                                    tool_use_id: None,
                                                    tool_use_name: None,
                                                    tool_use_input: None,
                                                    tool_result: None,
                                                    token_usage: None,
                                                }).ok();
                                            }
                                            crate::llm::types::StreamDelta::InputJsonDelta { partial_json } => {
                                                current_tool_input.push_str(&partial_json);
                                                app.emit("chat-stream", ChatMessageEvent {
                                                    r#type: "tool-json-delta".into(),
                                                    text: None,
                                                    tool_use_id: current_tool_id.clone(),
                                                    tool_use_name: None,
                                                    tool_use_input: Some(partial_json),
                                                    tool_result: None,
                                                    token_usage: None,
                                                }).ok();
                                            }
                                        }
                                    }
                                    StreamEvent::ContentBlockStop { .. } => {
                                        if let (Some(id), Some(name)) = (current_tool_id.take(), current_tool_name.take()) {
                                            let input_value: serde_json::Value = serde_json::from_str(&current_tool_input)
                                                .unwrap_or_else(|_| serde_json::json!({}));
                                            
                                            // Append ToolUse Block
                                            output_blocks.push(ContentBlock::ToolUse {
                                                id: id.clone(),
                                                name: name.clone(),
                                                input: input_value.clone(),
                                            });

                                            // Execute Tool
                                            let tool_result_str = tools::execute_tool(&name, input_value);
                                            
                                            app.emit("chat-stream", ChatMessageEvent {
                                                r#type: "tool-result".into(),
                                                text: None,
                                                tool_use_id: Some(id.clone()),
                                                tool_use_name: Some(name),
                                                tool_use_input: None,
                                                tool_result: Some(tool_result_str.clone()),
                                                token_usage: current_output_tokens,
                                            }).ok();

                                            // We append the tool result to another message directly
                                            return Ok(vec![
                                                Message {
                                                    role: Role::Assistant,
                                                    content: Content::Blocks(output_blocks),
                                                },
                                                Message {
                                                    role: Role::User,
                                                    content: Content::Blocks(vec![ContentBlock::ToolResult {
                                                        tool_use_id: id,
                                                        is_error: false,
                                                        content: vec![ContentBlock::Text {
                                                            text: tool_result_str,
                                                        }],
                                                    }]),
                                                }
                                            ]);
                                        } else if !generated_text.is_empty() {
                                            output_blocks.push(ContentBlock::Text {
                                                text: generated_text.clone(),
                                            });
                                            generated_text.clear();
                                        }
                                    }
                                    StreamEvent::MessageDelta { usage, .. } => {
                                        current_output_tokens = Some(usage.output_tokens);
                                        app.emit("chat-stream", ChatMessageEvent {
                                            r#type: "token-usage".into(),
                                            text: None,
                                            tool_use_id: None,
                                            tool_use_name: None,
                                            tool_use_input: None,
                                            tool_result: None,
                                            token_usage: current_output_tokens,
                                        }).ok();
                                    }
                                    StreamEvent::MessageStop => {
                                        emitted_stop = true;
                                        app.emit("chat-stream", ChatMessageEvent {
                                            r#type: "stop".into(),
                                            text: None,
                                            tool_use_id: None,
                                            tool_use_name: None,
                                            tool_use_input: None,
                                            tool_result: None,
                                            token_usage: current_output_tokens,
                                        }).ok();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }

            if !emitted_stop {
                app.emit("chat-stream", ChatMessageEvent {
                    r#type: "stop".into(),
                    text: None,
                    tool_use_id: None,
                    tool_use_name: None,
                    tool_use_input: None,
                    tool_result: None,
                    token_usage: current_output_tokens,
                }).ok();
            }

            Ok(vec![Message {
                role: Role::Assistant,
                content: Content::Blocks(output_blocks),
            }])
        }
        Err(e) => {
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn send_chat_message(
    app: AppHandle,
    messages: Vec<crate::llm::types::Message>,
) -> Result<(), String> {
    let mut current_messages = messages.clone();
    
    loop {
        let new_messages = send_request(&app, &current_messages).await?;
        current_messages.extend(new_messages.clone());
        
        let has_tool_result = new_messages.iter().any(|m| {
            if let Content::Blocks(blocks) = &m.content {
                blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }))
            } else {
                false
            }
        });
        
        if !has_tool_result {
            break;
        }
    }
    
    Ok(())
}
