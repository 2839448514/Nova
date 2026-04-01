use reqwest::Client;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri::Manager;
use futures_util::StreamExt;
use std::path::PathBuf;

use crate::llm::types::{AnthropicRequest, StreamEvent, Message, Content, ContentBlock, Role, Tool};
use crate::llm::tools;

const FALLBACK_SYSTEM_PROMPT: &str = "You are a helpful coding assistant running in a local Tauri desktop app. You will answer questions briefly and write accurate code.";

fn read_non_empty_file(path: &PathBuf) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn load_system_prompt(app: &AppHandle) -> String {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(app_dir) = app.path().app_data_dir() {
        candidates.push(app_dir.join("system_prompt.md"));
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("src-tauri").join("prompts").join("system_prompt.md"));
        candidates.push(cwd.join("prompts").join("system_prompt.md"));
    }

    for path in candidates {
        if let Some(prompt) = read_non_empty_file(&path) {
            return prompt;
        }
    }

    FALLBACK_SYSTEM_PROMPT.to_string()
}

fn parse_mcp_tool_name(name: &str) -> Option<(String, String)> {
    let raw = name.strip_prefix("mcp/")?;
    let mut parts = raw.splitn(2, '/');
    let server = parts.next()?.trim();
    let tool = parts.next()?.trim();
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server.to_string(), tool.to_string()))
}

async fn collect_mcp_tools(app: &AppHandle) -> Vec<Tool> {
    let mut statuses = match crate::command::mcp::get_mcp_server_statuses(app.clone()).await {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let has_enabled = statuses.iter().any(|s| s.enabled);
    let has_connected = statuses.iter().any(|s| s.enabled && s.status == "connected");
    if has_enabled && !has_connected {
        let _ = crate::command::mcp::reload_all_mcp_servers(app.clone()).await;
        statuses = match crate::command::mcp::get_mcp_server_statuses(app.clone()).await {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
    }

    let mut tools_vec = Vec::new();
    for status in statuses.into_iter().filter(|s| s.enabled && s.status == "connected") {
        let listed = match crate::command::mcp::list_mcp_tools(app.clone(), status.name.clone()).await {
            Ok(v) => v,
            Err(_) => continue,
        };

        for t in listed {
            tools_vec.push(Tool {
                name: format!("mcp/{}/{}", status.name, t.name),
                description: t
                    .description
                    .unwrap_or_else(|| format!("MCP tool '{}' from server '{}'.", t.name, status.name)),
                input_schema: t
                    .input_schema
                    .unwrap_or_else(|| serde_json::json!({ "type": "object", "properties": {} })),
            });
        }
    }

    tools_vec
}

fn has_needs_user_input(messages: &[Message]) -> bool {
    messages.iter().any(|m| {
        let Content::Blocks(blocks) = &m.content else {
            return false;
        };

        blocks.iter().any(|b| {
            let ContentBlock::ToolResult { content, .. } = b else {
                return false;
            };

            content.iter().any(|inner| {
                let ContentBlock::Text { text } = inner else {
                    return false;
                };

                serde_json::from_str::<serde_json::Value>(text)
                    .ok()
                    .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(|s| s == "needs_user_input"))
                    .unwrap_or(false)
            })
        })
    })
}

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

fn is_needs_user_input_payload(raw: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(|s| s == "needs_user_input"))
        .unwrap_or(false)
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

    let mut available_tools = tools::get_available_tools();
    available_tools.extend(collect_mcp_tools(app).await);

    let request = AnthropicRequest {
        model: settings.model,
        max_tokens: 4096,
        system: Some(load_system_prompt(app)),
        messages: messages.clone(),
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
            let mut tool_result_blocks: Vec<ContentBlock> = Vec::new();
            let mut emitted_stop = false;
            let mut current_output_tokens: Option<u32> = None;
            let mut stop_emitted_for_user_input = false;

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
                                            let tool_result_str = if let Some((server_name, tool_name)) = parse_mcp_tool_name(&name) {
                                                match crate::command::mcp::call_mcp_tool(
                                                    app.clone(),
                                                    server_name,
                                                    tool_name,
                                                    input_value,
                                                )
                                                .await
                                                {
                                                    Ok(v) => v.to_string(),
                                                    Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
                                                }
                                            } else {
                                                tools::execute_tool(&name, input_value)
                                            };
                                            
                                            app.emit("chat-stream", ChatMessageEvent {
                                                r#type: "tool-result".into(),
                                                text: None,
                                                tool_use_id: Some(id.clone()),
                                                tool_use_name: Some(name),
                                                tool_use_input: None,
                                                tool_result: Some(tool_result_str.clone()),
                                                token_usage: current_output_tokens,
                                            }).ok();

                                            let needs_user_input = is_needs_user_input_payload(&tool_result_str);

                                            // Only stop early when the tool explicitly asks for user input.
                                            if needs_user_input && !stop_emitted_for_user_input {
                                                stop_emitted_for_user_input = true;
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

            let mut result_messages = vec![Message {
                role: Role::Assistant,
                content: Content::Blocks(output_blocks),
            }];

            if !tool_result_blocks.is_empty() {
                result_messages.push(Message {
                    role: Role::User,
                    content: Content::Blocks(tool_result_blocks),
                });
            }

            Ok(result_messages)
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

        if has_needs_user_input(&new_messages) {
            break;
        }
        
        if !has_tool_result {
            break;
        }
    }
    
    Ok(())
}
