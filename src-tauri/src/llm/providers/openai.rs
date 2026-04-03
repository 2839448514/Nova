use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<Value>, // String or array of parts
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiReqToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct OpenAiReqToolCall {
    id: String,
    r#type: String,
    function: OpenAiReqFunction,
}

#[derive(Debug, Serialize, Clone)]
struct OpenAiReqFunction {
    name: String,
    arguments: String,
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
    #[allow(dead_code)]
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

#[derive(Debug, Default)]
struct PendingToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl OpenAiProvider {
    pub async fn send_request(
        &self,
        app: &AppHandle,
        messages: &[Message],
        plan_mode: bool,
    ) -> Result<Vec<Message>, String> {
        let settings = crate::command::settings::get_settings(app.clone());
        let profile = settings.active_provider_profile();
        
        let mut available_tools = tools::get_available_tools();
        available_tools.extend(mcp_tools::collect_mcp_tools(app).await);

        let system_prompt = load_system_prompt(app, plan_mode);
        
        let mut oai_messages = vec![OpenAiMessage {
            role: "system".into(),
            content: Some(Value::String(system_prompt)),
            tool_calls: None,
            tool_call_id: None,
        }];

        for m in messages {
            let base_role = match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            
            match &m.content {
                crate::llm::types::Content::Text(t) => {
                    oai_messages.push(OpenAiMessage {
                        role: base_role.into(),
                        content: Some(Value::String(t.clone())),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                crate::llm::types::Content::Blocks(blocks) => {
                    let mut text_parts = Vec::new();
                    let mut tool_calls = Vec::new();
                    let mut tool_results = Vec::new();
                    
                    for b in blocks {
                        match b {
                            ContentBlock::Text { text } => {
                                text_parts.push(text.clone());
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(OpenAiReqToolCall {
                                    id: id.clone(),
                                    r#type: "function".into(),
                                    function: OpenAiReqFunction {
                                        name: name.clone(),
                                        arguments: input.to_string(), // Input is a Value, we serialize it
                                    }
                                });
                            }
                            ContentBlock::ToolResult { tool_use_id, is_error: _, content } => {
                                let mut tr_text = Vec::new();
                                for tb in content {
                                    if let ContentBlock::Text { text } = tb {
                                        tr_text.push(text.clone());
                                    }
                                }
                                tool_results.push((tool_use_id.clone(), tr_text.join("\n")));
                            }
                        }
                    }
                    
                    if base_role == "assistant" {
                        let content_val = if text_parts.is_empty() && !tool_calls.is_empty() {
                            None // Optional for tool calls in assistant
                        } else {
                            Some(Value::String(text_parts.join("\n")))
                        };
                        
                        let tc = if tool_calls.is_empty() { None } else { Some(tool_calls) };
                        oai_messages.push(OpenAiMessage {
                            role: "assistant".into(),
                            content: content_val,
                            tool_calls: tc,
                            tool_call_id: None,
                        });
                    } else {
                        // User message might contain text AND tool results
                        if !text_parts.is_empty() {
                            oai_messages.push(OpenAiMessage {
                                role: "user".into(),
                                content: Some(Value::String(text_parts.join("\n"))),
                                tool_calls: None,
                                tool_call_id: None,
                            });
                        }
                        
                        for (tid, tr_text) in tool_results {
                            oai_messages.push(OpenAiMessage {
                                role: "tool".into(),
                                content: Some(Value::String(tr_text)),
                                tool_calls: None,
                                tool_call_id: Some(tid),
                            });
                        }
                    }
                }
            }
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
            model: profile.model.clone(),
            messages: oai_messages,
            tools,
            stream: true,
        };

        let client = Client::new();
        let mut url = profile.base_url.trim_end_matches('/').to_string();
        if !url.ends_with("/v1/chat/completions") && !url.ends_with("/chat/completions") {
            if url.ends_with("/v1") {
                url = format!("{}/chat/completions", url);
            } else {
                url = format!("{}/v1/chat/completions", url);
            }
        }

        let mut req_builder = client.post(&url).header("content-type", "application/json");

        if !profile.api_key.is_empty() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", profile.api_key));
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
        let mut pending_tool_calls: BTreeMap<usize, PendingToolCall> = BTreeMap::new();
        
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
                                        let entry = pending_tool_calls.entry(tc.index).or_default();

                                        if let Some(id) = tc.id {
                                            entry.id = Some(id);
                                        }

                                        if let Some(func) = tc.function {
                                            if let Some(name) = func.name {
                                                if entry.name.is_none() {
                                                    app.emit(
                                                        "chat-stream",
                                                        ChatMessageEvent {
                                                            r#type: "tool-use-start".into(),
                                                            text: None,
                                                            tool_use_id: entry.id.clone(),
                                                            tool_use_name: Some(name.clone()),
                                                            tool_use_input: None,
                                                            tool_result: None,
                                                            token_usage: None,
                                                            stop_reason: None,
                                                            turn_state: Some("tool_running".into()),
                                                        },
                                                    )
                                                    .ok();
                                                }
                                                entry.name = Some(name);
                                            }

                                            if let Some(args) = func.arguments {
                                                entry.arguments.push_str(&args);
                                                app.emit(
                                                    "chat-stream",
                                                    ChatMessageEvent {
                                                        r#type: "tool-json-delta".into(),
                                                        text: None,
                                                        tool_use_id: entry.id.clone(),
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
                                        let drained_calls: Vec<(usize, PendingToolCall)> =
                                            pending_tool_calls
                                                .iter()
                                                .map(|(k, v)| {
                                                    (
                                                        *k,
                                                        PendingToolCall {
                                                            id: v.id.clone(),
                                                            name: v.name.clone(),
                                                            arguments: v.arguments.clone(),
                                                        },
                                                    )
                                                })
                                                .collect();

                                        pending_tool_calls.clear();

                                        for (_, tc) in drained_calls {
                                            let (Some(id), Some(name)) = (tc.id, tc.name) else {
                                                continue;
                                            };

                                            let input_value: Value = serde_json::from_str(&tc.arguments)
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
                                                turn_state: Some("intermediate".into()),
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
                    turn_state: Some("intermediate".into()),
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
