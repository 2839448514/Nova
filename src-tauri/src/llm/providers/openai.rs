use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::llm::query_engine::ChatMessageEvent;
use crate::llm::services::mcp_tools;
use crate::llm::services::mcp_tools::parse_mcp_tool_name;
use crate::llm::tools;
use crate::llm::types::{ContentBlock, Message, Role};
use crate::llm::utils::system_prompt::load_system_prompt;

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    stream: bool,
}

#[derive(Debug, Serialize, Clone)]
struct OpenAiMessage {
    role: String,
    content: Value, // String or array of parts
}

#[derive(Debug, Serialize)]
struct OpenAiTool {
    r#type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    delta: OpenAiDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCall {
    index: usize,
    id: Option<String>,
    function: Option<OpenAiFunctionCall>,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunctionCall {
    name: Option<String>,
    arguments: Option<String>,
}

pub struct OpenAiProvider;

impl OpenAiProvider {
    pub async fn send_request(
        &self,
        app: &AppHandle,
        messages: &[Message],
        plan_mode: bool,
    ) -> Result<Vec<Message>, String> {
        let settings = crate::command::settings::get_settings(app.clone());
        
        let mut available_tools = tools::get_available_tools();
        available_tools.extend(mcp_tools::collect_mcp_tools(app).await);

        let system_prompt = load_system_prompt(app, plan_mode);
        
        let mut oai_messages = vec![OpenAiMessage {
            role: "system".into(),
            content: Value::String(system_prompt),
        }];

        for m in messages {
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            
            // basic content translation
            let content_val = match &m.content {
                crate::llm::types::Content::Text(t) => Value::String(t.clone()),
                crate::llm::types::Content::Blocks(blocks) => {
                    let mut text_parts = Vec::new();
                    for b in blocks {
                        if let ContentBlock::Text { text } = b {
                            text_parts.push(text.clone());
                        }
                    }
                    Value::String(text_parts.join("\n"))
                }
            };
            oai_messages.push(OpenAiMessage {
                role: role.into(),
                content: content_val,
            });
        }

        let tools: Option<Vec<OpenAiTool>> = if available_tools.is_empty() {
            None
        } else {
            Some(
                available_tools
                    .into_iter()
                    .map(|t| OpenAiTool {
                        r#type: "function".into(),
                        function: OpenAiFunction {
                            name: t.name,
                            description: t.description,
                            parameters: t.input_schema,
                        },
                    })
                    .collect(),
            )
        };

        let request = OpenAiRequest {
            model: settings.model.clone(),
            messages: oai_messages,
            tools,
            stream: true,
        };

        let client = Client::new();
        let mut url = settings.base_url.trim_end_matches('/').to_string();
        if !url.ends_with("/v1/chat/completions") && !url.ends_with("/chat/completions") {
            if url.ends_with("/v1") {
                url = format!("{}/chat/completions", url);
            } else {
                url = format!("{}/v1/chat/completions", url);
            }
        }

        let mut req_builder = client.post(&url).header("content-type", "application/json");

        if !settings.api_key.is_empty() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", settings.api_key));
        }

        let resp = req_builder.json(&request).send().await;

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
        let mut generated_text = String::new();
        
        let mut current_tool_id = None;
        let mut current_tool_name = None;
        let mut current_tool_input = String::new();
        
        let mut output_blocks: Vec<ContentBlock> = Vec::new();
        let mut tool_result_blocks: Vec<ContentBlock> = Vec::new();
        
        let mut emitted_stop = false;

        while let Some(chunk) = stream.next().await {
            if let Ok(bytes) = chunk {
                let text = String::from_utf8_lossy(&bytes);
                for line in text.lines() {
                    let line = line.trim();
                    if line.starts_with("data: ") || line.starts_with("data:") {
                        let data = line.strip_prefix("data: ").unwrap_or_else(|| line.strip_prefix("data:").unwrap());
                        if data == "[DONE]" {
                            break;
                        }
                        if let Ok(chunk) = serde_json::from_str::<OpenAiStreamChunk>(data) {
                            for choice in chunk.choices {
                                if let Some(content) = choice.delta.content {
                                    generated_text.push_str(&content);
                                    app.emit(
                                        "chat-stream",
                                        ChatMessageEvent {
                                            r#type: "text".into(),
                                            text: Some(content),
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
                                
                                if let Some(tool_calls) = choice.delta.tool_calls {
                                    for tc in tool_calls {
                                        if let Some(id) = tc.id {
                                            current_tool_id = Some(id.clone());
                                        }
                                        if let Some(func) = tc.function {
                                            if let Some(name) = func.name {
                                                current_tool_name = Some(name.clone());
                                                current_tool_input.clear();
                                                
                                                app.emit(
                                                    "chat-stream",
                                                    ChatMessageEvent {
                                                        r#type: "tool-use-start".into(),
                                                        text: None,
                                                        tool_use_id: current_tool_id.clone(),
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
                                            if let Some(args) = func.arguments {
                                                current_tool_input.push_str(&args);
                                                app.emit(
                                                    "chat-stream",
                                                    ChatMessageEvent {
                                                        r#type: "tool-json-delta".into(),
                                                        text: None,
                                                        tool_use_id: current_tool_id.clone(),
                                                        tool_use_name: None,
                                                        tool_use_input: Some(args),
                                                        tool_result: None,
                                                        token_usage: None,
                                                        stop_reason: None,
                                                        turn_state: Some("tool_input_streaming".into()),
                                                    },
                                                )
                                                .ok();
                                            }
                                        }
                                    }
                                }

                                if let Some(finish_reason) = choice.finish_reason {
                                    if finish_reason == "tool_calls" {
                                        if let (Some(id), Some(name)) = (current_tool_id.take(), current_tool_name.take()) {
                                            let input_value: Value = serde_json::from_str(&current_tool_input)
                                                .unwrap_or_else(|_| serde_json::json!({}));

                                            output_blocks.push(ContentBlock::ToolUse {
                                                id: id.clone(),
                                                name: name.clone(),
                                                input: input_value.clone(),
                                            });

                                            let tool_result_str = if let Some((server_name, tool_name)) = parse_mcp_tool_name(&name) {
                                                match crate::command::mcp::call_mcp_tool(
                                                    app.clone(),
                                                    server_name,
                                                    tool_name,
                                                    input_value,
                                                ).await {
                                                    Ok(v) => v.to_string(),
                                                    Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string()
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
                                                    token_usage: None,
                                                    stop_reason: None,
                                                    turn_state: Some("tool_completed".into()),
                                                },
                                            )
                                            .ok();

                                            tool_result_blocks.push(ContentBlock::ToolResult {
                                                tool_use_id: id,
                                                is_error: false,
                                                content: vec![ContentBlock::Text {
                                                    text: tool_result_str,
                                                }],
                                            });
                                        }
                                    } else if finish_reason == "stop" {
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
                                                token_usage: None,
                                                stop_reason: Some(finish_reason),
                                                turn_state: Some("completed".into()),
                                            },
                                        )
                                        .ok();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if !generated_text.is_empty() {
            output_blocks.push(ContentBlock::Text {
                text: generated_text.clone(),
            });
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
                    token_usage: None,
                    stop_reason: None,
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
