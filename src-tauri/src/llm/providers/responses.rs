use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::time::timeout;

use crate::llm::query_engine::ChatMessageEvent;
use crate::llm::providers::ProviderTurnResult;
use crate::llm::tools;
use crate::llm::types::{AgentMode, ContentBlock, Message, Role};
use crate::llm::utils::error_event::emit_backend_error;
use crate::llm::utils::system_prompt::load_system_prompt;

// OpenAI Responses API Provider。
// 端点: POST /v1/responses
// 与 Chat Completions 的主要区别：
//   - 消息数组字段名 messages → input
//   - system prompt 通过顶层 instructions 字段传递
//   - 工具结果格式: type=function_call_output + call_id（不是 tool_call_id）
//   - 工具调用格式: type=function_call + call_id
//   - SSE 事件类型: response.output_text.delta / response.function_call_arguments.delta 等
//   - usage 在 response.completed 事件里，字段名 input_tokens / output_tokens

pub struct ResponsesProvider;

#[derive(Debug, Serialize)]
struct ResponsesRequest {
    // 目标模型名。
    model: String,
    // 输入项数组（消息、函数调用、函数结果）。
    input: Vec<Value>,
    // 系统提示词，作为顶层 instructions 传递。
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    // 工具定义列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ResponsesTool>>,
    // 开启流式返回。
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ResponsesTool {
    // 固定为 function。
    r#type: String,
    // 工具名。
    name: String,
    // 工具描述。
    description: String,
    // 工具输入 schema。
    parameters: Value,
}

// 流内正在累积的 function call 状态（按 output_index 索引）。
#[derive(Debug, Default)]
struct PendingFunctionCall {
    // 调用 ID（用于关联工具结果）。
    call_id: Option<String>,
    // 工具函数名。
    name: Option<String>,
    // 累积中的 JSON 参数字符串。
    arguments: String,
}

fn truncate_for_log(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let preview: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}...", preview)
    } else {
        preview
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn find_sse_event_delimiter(input: &[u8]) -> Option<(usize, usize)> {
    let lf = find_bytes(input, b"\n\n").map(|i| (i, 2));
    let crlf = find_bytes(input, b"\r\n\r\n").map(|i| (i, 4));
    match (lf, crlf) {
        (Some(l), Some(r)) => Some(if l.0 <= r.0 { l } else { r }),
        (Some(v), None) | (None, Some(v)) => Some(v),
        _ => None,
    }
}

fn extract_sse_data(event_raw: &str) -> String {
    event_raw
        .lines()
        .filter_map(|line| {
            line.trim_start()
                .strip_prefix("data:")
                .map(|data| data.trim_start().to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// 将内部 Message 数组转换为 Responses API input 数组。
// 规则：
//   User(Text)        → {type:message, role:user, content:[{type:input_text}]}
//   User(Blocks)      → text/image 合并为 message item，ToolResult 转换为 function_call_output item
//   Assistant(Text)   → {type:message, role:assistant, content:[{type:output_text}]}
//   Assistant(Blocks) → text 转换为 message item，ToolUse 转换为 function_call item
fn messages_to_input(messages: &[Message]) -> Vec<Value> {
    let mut input: Vec<Value> = Vec::new();

    for m in messages {
        match m.role {
            Role::User => match &m.content {
                crate::llm::types::Content::Text(t) => {
                    input.push(serde_json::json!({
                        "type": "message",
                        "role": "user",
                        "content": [{"type": "input_text", "text": t}]
                    }));
                }
                crate::llm::types::Content::Blocks(blocks) => {
                    let mut content_parts: Vec<Value> = Vec::new();

                    for b in blocks {
                        match b {
                            ContentBlock::Text { text } => {
                                content_parts.push(serde_json::json!({
                                    "type": "input_text",
                                    "text": text
                                }));
                            }
                            ContentBlock::Image { source } => {
                                if source.source_type.eq_ignore_ascii_case("base64")
                                    && !source.media_type.is_empty()
                                    && !source.data.is_empty()
                                {
                                    content_parts.push(serde_json::json!({
                                        "type": "input_image",
                                        "image_url": format!("data:{};base64,{}", source.media_type, source.data)
                                    }));
                                }
                            }
                            ContentBlock::ToolResult { tool_use_id, content, .. } => {
                                // 工具结果作为顶层 function_call_output 项，call_id 对应工具调用 ID。
                                let text = content
                                    .iter()
                                    .filter_map(|b| {
                                        if let ContentBlock::Text { text } = b {
                                            Some(text.as_str())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                // function_call_output 先于同轮 user 消息推入 input。
                                input.push(serde_json::json!({
                                    "type": "function_call_output",
                                    "call_id": tool_use_id,
                                    "output": text
                                }));
                            }
                            _ => {}
                        }
                    }

                    if !content_parts.is_empty() {
                        input.push(serde_json::json!({
                            "type": "message",
                            "role": "user",
                            "content": content_parts
                        }));
                    }
                }
            },

            Role::Assistant => match &m.content {
                crate::llm::types::Content::Text(t) => {
                    input.push(serde_json::json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": t}]
                    }));
                }
                crate::llm::types::Content::Blocks(blocks) => {
                    let mut text_content: Vec<&str> = Vec::new();
                    let mut tool_uses: Vec<(&String, &String, &Value)> = Vec::new();

                    for b in blocks {
                        match b {
                            ContentBlock::Text { text } => {
                                text_content.push(text.as_str());
                            }
                            ContentBlock::ToolUse { id, name, input: tool_input } => {
                                tool_uses.push((id, name, tool_input));
                            }
                            // Thinking 块跳过（Responses API 不支持）。
                            _ => {}
                        }
                    }

                    if !text_content.is_empty() {
                        let combined = text_content.join("\n");
                        input.push(serde_json::json!({
                            "type": "message",
                            "role": "assistant",
                            "content": [{"type": "output_text", "text": combined}]
                        }));
                    }

                    for (id, name, tool_input) in tool_uses {
                        let args = serde_json::to_string(tool_input).unwrap_or_default();
                        input.push(serde_json::json!({
                            "type": "function_call",
                            "call_id": id,
                            "name": name,
                            "arguments": args
                        }));
                    }
                }
            },
        }
    }

    input
}

impl ResponsesProvider {
    pub async fn send_request(
        &self,
        app: &AppHandle,
        messages: &[Message],
        agent_mode: AgentMode,
        conversation_id: Option<&str>,
    ) -> Result<ProviderTurnResult, String> {
        let settings = crate::command::settings::get_settings(app.clone());
        let profile = settings.active_provider_profile();

        let available_tools = tools::get_available_tools();
        let system_prompt = load_system_prompt(app, agent_mode)?;

        let input = messages_to_input(messages);

        let tools_list: Option<Vec<ResponsesTool>> = if available_tools.is_empty() {
            None
        } else {
            Some(
                available_tools
                    .into_iter()
                    .map(|t| ResponsesTool {
                        r#type: "function".into(),
                        name: t.name,
                        description: t.description,
                        parameters: t.input_schema,
                    })
                    .collect(),
            )
        };

        let request = ResponsesRequest {
            model: profile.model.clone(),
            input,
            instructions: Some(system_prompt),
            tools: tools_list,
            stream: true,
        };

        let client = Client::new();

        // 规范化到 /v1/responses 端点。
        let mut url = profile.base_url.trim_end_matches('/').to_string();
        if !url.ends_with("/v1/responses") && !url.ends_with("/responses") {
            if url.ends_with("/v1") {
                url = format!("{}/responses", url);
            } else {
                url = format!("{}/v1/responses", url);
            }
        }

        let mut req_builder = client.post(&url).header("content-type", "application/json");

        if !profile.api_key.is_empty() {
            req_builder =
                req_builder.header("Authorization", format!("Bearer {}", profile.api_key));
        }

        let resp = req_builder.json(&request).send().await;

        match resp {
            Ok(res) => {
                if !res.status().is_success() {
                    let status = res.status();
                    let error_text = res.text().await.unwrap_or_default();
                    let msg = format!("API Error [{}] {} => {}", status, url, error_text);
                    emit_backend_error(
                        app,
                        "llm.providers.responses",
                        msg.clone(),
                        Some("http.non_success"),
                    );
                    return Err(msg);
                }
                self.process_stream_response(app, res, conversation_id).await
            }
            Err(e) => {
                let msg = e.to_string();
                emit_backend_error(
                    app,
                    "llm.providers.responses",
                    msg.clone(),
                    Some("http.request"),
                );
                Err(msg)
            }
        }
    }

    async fn process_stream_response(
        &self,
        app: &AppHandle,
        response: reqwest::Response,
        conversation_id: Option<&str>,
    ) -> Result<ProviderTurnResult, String> {
        let mut stream = response.bytes_stream();
        let mut sse_buffer: Vec<u8> = Vec::new();

        // 累积文本输出。
        let mut generated_text = String::new();
        // 按 output_index 累积未完成的 function call 状态。
        let mut pending_fn_calls: BTreeMap<usize, PendingFunctionCall> = BTreeMap::new();

        let mut output_blocks: Vec<ContentBlock> = Vec::new();
        let mut tool_result_blocks: Vec<ContentBlock> = Vec::new();
        let mut additional_context_messages: Vec<Message> = Vec::new();
        let mut prevent_continuation = false;
        let mut hook_stop_reason: Option<String> = None;

        let mut emitted_stop = false;
        let mut current_input_tokens: Option<u32> = None;
        let mut current_output_tokens: Option<u32> = None;

        loop {
            if crate::llm::cancellation::is_cancelled(conversation_id) {
                return Ok(ProviderTurnResult {
                    messages: Vec::new(),
                    stop_reason: Some("cancelled".into()),
                    input_tokens: current_input_tokens,
                    output_tokens: current_output_tokens,
                    prevent_continuation: false,
                });
            }

            let next_chunk = match timeout(Duration::from_millis(200), stream.next()).await {
                Ok(v) => v,
                Err(_) => continue,
            };

            let Some(chunk) = next_chunk else {
                break;
            };

            let bytes = match chunk {
                Ok(v) => v,
                Err(e) => {
                    let msg = format!("Responses API stream chunk error: {}", e);
                    emit_backend_error(
                        app,
                        "llm.providers.responses",
                        msg.clone(),
                        Some("stream.chunk"),
                    );
                    return Err(msg);
                }
            };
            sse_buffer.extend_from_slice(&bytes);

            while let Some((event_end, delimiter_len)) = find_sse_event_delimiter(&sse_buffer) {
                let event_bytes = sse_buffer[..event_end].to_vec();
                sse_buffer.drain(..event_end + delimiter_len);

                let event_raw = match String::from_utf8(event_bytes) {
                    Ok(s) => s,
                    Err(e) => {
                        let preview = String::from_utf8_lossy(e.as_bytes()).into_owned();
                        let msg = format!(
                            "Responses API stream returned non-UTF-8 SSE event. Preview: {}",
                            truncate_for_log(&preview, 800)
                        );
                        emit_backend_error(
                            app,
                            "llm.providers.responses",
                            msg.clone(),
                            Some("stream.utf8"),
                        );
                        return Err(msg);
                    }
                };

                let data = extract_sse_data(&event_raw);
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }

                // 回传 raw-json 给前端调试。
                app.emit(
                    "chat-stream",
                    ChatMessageEvent {
                        r#type: "raw-json".into(),
                        text: Some(data.clone()),
                        tool_use_id: None,
                        tool_use_name: None,
                        tool_use_input: None,
                        tool_result: None,
                        token_usage: None,
                        stop_reason: None,
                        turn_state: Some("raw_stream".into()),
                        conversation_id: conversation_id.map(str::to_string),
                    },
                )
                .ok();

                let event: Value = match serde_json::from_str(&data) {
                    Ok(v) => v,
                    Err(e) => {
                        let msg = format!(
                            "Failed to parse Responses API SSE event: {}. Data: {}",
                            e,
                            truncate_for_log(&data, 1200)
                        );
                        emit_backend_error(
                            app,
                            "llm.providers.responses",
                            msg.clone(),
                            Some("stream.parse"),
                        );
                        return Err(msg);
                    }
                };

                let event_type = event["type"].as_str().unwrap_or("").to_owned();

                match event_type.as_str() {
                    // 新输出项开始：仅处理 function_call 类型，记录 call_id/name 并通知前端。
                    "response.output_item.added" => {
                        let output_index =
                            event["output_index"].as_u64().unwrap_or(0) as usize;
                        let item = &event["item"];
                        if item["type"].as_str() == Some("function_call") {
                            let call_id = item["call_id"].as_str().map(str::to_string);
                            let name = item["name"].as_str().map(str::to_string);
                            let entry = pending_fn_calls.entry(output_index).or_default();
                            entry.call_id = call_id.clone();
                            entry.name = name.clone();
                            if let Some(ref n) = name {
                                app.emit(
                                    "chat-stream",
                                    ChatMessageEvent {
                                        r#type: "tool-use-start".into(),
                                        text: None,
                                        tool_use_id: call_id,
                                        tool_use_name: Some(n.clone()),
                                        tool_use_input: None,
                                        tool_result: None,
                                        token_usage: None,
                                        stop_reason: None,
                                        turn_state: Some("tool_running".into()),
                                        conversation_id: conversation_id.map(str::to_string),
                                    },
                                )
                                .ok();
                            }
                        }
                    }

                    // 文本增量：推送到前端并累积。
                    "response.output_text.delta" => {
                        if let Some(delta) = event["delta"].as_str() {
                            generated_text.push_str(delta);
                            app.emit(
                                "chat-stream",
                                ChatMessageEvent {
                                    r#type: "text".into(),
                                    text: Some(delta.to_string()),
                                    tool_use_id: None,
                                    tool_use_name: None,
                                    tool_use_input: None,
                                    tool_result: None,
                                    token_usage: None,
                                    stop_reason: None,
                                    turn_state: Some("streaming_text".into()),
                                    conversation_id: conversation_id.map(str::to_string),
                                },
                            )
                            .ok();
                        }
                    }

                    // reasoning 增量（o-系列模型）：推送到前端，不写入 output_blocks。
                    "response.reasoning_summary_text.delta" => {
                        if let Some(delta) = event["delta"].as_str() {
                            if !delta.is_empty() {
                                app.emit(
                                    "chat-stream",
                                    ChatMessageEvent {
                                        r#type: "reasoning".into(),
                                        text: Some(delta.to_string()),
                                        tool_use_id: None,
                                        tool_use_name: None,
                                        tool_use_input: None,
                                        tool_result: None,
                                        token_usage: None,
                                        stop_reason: None,
                                        turn_state: Some("streaming_reasoning".into()),
                                        conversation_id: conversation_id.map(str::to_string),
                                    },
                                )
                                .ok();
                            }
                        }
                    }

                    // 函数调用参数增量：累积并推送到前端。
                    "response.function_call_arguments.delta" => {
                        let output_index =
                            event["output_index"].as_u64().unwrap_or(0) as usize;
                        if let Some(delta) = event["delta"].as_str() {
                            let entry = pending_fn_calls.entry(output_index).or_default();
                            entry.arguments.push_str(delta);
                            app.emit(
                                "chat-stream",
                                ChatMessageEvent {
                                    r#type: "tool-json-delta".into(),
                                    text: None,
                                    tool_use_id: entry.call_id.clone(),
                                    tool_use_name: None,
                                    tool_use_input: Some(delta.to_string()),
                                    tool_result: None,
                                    token_usage: None,
                                    stop_reason: None,
                                    turn_state: Some("tool_input_streaming".into()),
                                    conversation_id: conversation_id.map(str::to_string),
                                },
                            )
                            .ok();
                        }
                    }

                    // 输出项完成：function_call 类型时执行工具调用。
                    "response.output_item.done" => {
                        let output_index =
                            event["output_index"].as_u64().unwrap_or(0) as usize;
                        let item = &event["item"];

                        if item["type"].as_str() != Some("function_call") {
                            continue;
                        }

                        // 优先从 item 字段取值，兜底从 pending 状态取。
                        let call_id = item["call_id"]
                            .as_str()
                            .map(str::to_string)
                            .or_else(|| {
                                pending_fn_calls
                                    .get(&output_index)
                                    .and_then(|p| p.call_id.clone())
                            });
                        let name = item["name"]
                            .as_str()
                            .map(str::to_string)
                            .or_else(|| {
                                pending_fn_calls
                                    .get(&output_index)
                                    .and_then(|p| p.name.clone())
                            });
                        let arguments = item["arguments"]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| {
                                pending_fn_calls
                                    .get(&output_index)
                                    .map(|p| p.arguments.clone())
                                    .unwrap_or_default()
                            });
                        pending_fn_calls.remove(&output_index);

                        let (call_id, name) = match (call_id, name) {
                            (Some(id), Some(n)) => (id, n),
                            (id, name) => {
                                let msg = format!(
                                    "Responses API function_call at output_index={} missing call_id or name: has_id={}, has_name={}",
                                    output_index,
                                    id.is_some(),
                                    name.is_some()
                                );
                                emit_backend_error(
                                    app,
                                    "llm.providers.responses",
                                    msg.clone(),
                                    Some("stream.function_call.incomplete"),
                                );
                                return Err(msg);
                            }
                        };

                        let input_value: Value = match serde_json::from_str(&arguments) {
                            Ok(v) => v,
                            Err(e) => {
                                let msg = format!(
                                    "Failed to parse Responses API function call arguments for '{}': {}. Args: {}",
                                    name,
                                    e,
                                    truncate_for_log(&arguments, 800)
                                );
                                emit_backend_error(
                                    app,
                                    "llm.providers.responses",
                                    msg.clone(),
                                    Some("stream.function_call.args_parse"),
                                );
                                return Err(msg);
                            }
                        };

                        output_blocks.push(ContentBlock::ToolUse {
                            id: call_id.clone(),
                            name: name.clone(),
                            input: input_value.clone(),
                        });

                        app.emit(
                            "chat-stream",
                            ChatMessageEvent {
                                r#type: "tool-executing".into(),
                                text: None,
                                tool_use_id: Some(call_id.clone()),
                                tool_use_name: Some(name.clone()),
                                tool_use_input: None,
                                tool_result: None,
                                token_usage: None,
                                stop_reason: None,
                                turn_state: Some("tool_executing".into()),
                                conversation_id: conversation_id.map(str::to_string),
                            },
                        )
                        .ok();

                        let executed_calls = tools::execute_tool_calls_with_app(
                            app,
                            conversation_id,
                            vec![tools::ToolCallRequest {
                                id: call_id.clone(),
                                name: name.clone(),
                                input: input_value,
                            }],
                        )
                        .await;

                        for executed in executed_calls {
                            let serialized_input = serde_json::to_string_pretty(&executed.input)
                                .unwrap_or_else(|_| executed.input.to_string());

                            app.emit(
                                "chat-stream",
                                ChatMessageEvent {
                                    r#type: "tool-result".into(),
                                    text: None,
                                    tool_use_id: Some(executed.id.clone()),
                                    tool_use_name: Some(executed.name.clone()),
                                    tool_use_input: Some(serialized_input),
                                    tool_result: Some(executed.output.clone()),
                                    token_usage: None,
                                    stop_reason: None,
                                    turn_state: Some("tool_completed".into()),
                                    conversation_id: conversation_id.map(str::to_string),
                                },
                            )
                            .ok();

                            // tool_use_id 存的就是 call_id，下轮作为 function_call_output.call_id 回传。
                            tool_result_blocks.push(ContentBlock::ToolResult {
                                tool_use_id: executed.id,
                                is_error: executed.is_error,
                                content: vec![ContentBlock::Text { text: executed.output }],
                            });

                            if !executed.additional_messages.is_empty() {
                                additional_context_messages
                                    .extend(executed.additional_messages);
                            }
                            if executed.prevent_continuation {
                                prevent_continuation = true;
                                if hook_stop_reason.is_none() {
                                    hook_stop_reason = executed.stop_reason;
                                }
                            }
                        }
                    }

                    // 流结束：读取 usage 并发 stop 事件。
                    "response.completed" => {
                        let usage = &event["response"]["usage"];
                        if let Some(v) = usage["input_tokens"].as_u64() {
                            current_input_tokens = Some(v as u32);
                        }
                        if let Some(v) = usage["output_tokens"].as_u64() {
                            current_output_tokens = Some(v as u32);
                        }

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
                                stop_reason: Some("end_turn".into()),
                                turn_state: Some("intermediate".into()),
                                conversation_id: conversation_id.map(str::to_string),
                            },
                        )
                        .ok();
                    }

                    // error 事件：上报并中止。
                    "error" => {
                        let code = event["code"].as_str().unwrap_or("unknown");
                        let message = event["message"].as_str().unwrap_or("unknown error");
                        let msg =
                            format!("Responses API stream error: code={}, message={}", code, message);
                        emit_backend_error(
                            app,
                            "llm.providers.responses",
                            msg.clone(),
                            Some("stream.error_event"),
                        );
                        return Err(msg);
                    }

                    // 其他事件类型静默忽略。
                    _ => {}
                }
            }
        }

        // 将剩余文本写入输出块。
        if !generated_text.is_empty() {
            output_blocks.push(ContentBlock::Text {
                text: generated_text.clone(),
            });
        }

        // 流内未发 stop 时补发。
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
                    stop_reason: None,
                    turn_state: Some("intermediate".into()),
                    conversation_id: conversation_id.map(str::to_string),
                },
            )
            .ok();
        }

        let output_blocks_empty = output_blocks.is_empty();
        let tool_result_blocks_empty = tool_result_blocks.is_empty();

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

        if !additional_context_messages.is_empty() {
            result_messages.extend(additional_context_messages);
        }

        let final_stop_reason = if prevent_continuation {
            hook_stop_reason.or_else(|| Some("hook_stopped_continuation".to_string()))
        } else {
            Some("end_turn".to_string())
        };

        if output_blocks_empty && tool_result_blocks_empty {
            let msg = format!(
                "Responses API provider returned empty assistant message. input_tokens={:?}, output_tokens={:?}, prevent_continuation={}",
                current_input_tokens, current_output_tokens, prevent_continuation
            );
            emit_backend_error(
                app,
                "llm.providers.responses",
                msg.clone(),
                Some("stream.empty_assistant"),
            );
            return Err(msg);
        }

        Ok(ProviderTurnResult {
            messages: result_messages,
            stop_reason: final_stop_reason,
            input_tokens: current_input_tokens,
            output_tokens: current_output_tokens,
            prevent_continuation,
        })
    }
}
