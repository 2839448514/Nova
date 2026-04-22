use tauri::{AppHandle, Emitter};

use crate::llm::flow_trace::FlowTracer;
use crate::llm::providers::LlmProvider;
use crate::llm::query_engine::ChatMessageEvent;
use crate::llm::services::compact;
use crate::llm::types::{AgentMode, Content, ContentBlock, Message, Role};
use crate::llm::utils::context_assembler::{self, AssembleOptions};
use crate::llm::utils::error_event::emit_backend_error;

mod state_machine;

use state_machine::TurnOutcome;

const SESSION_RAG_CONTEXT_MARKER: &str = "[Session RAG Context]";
const SESSION_RAG_SEARCH_LIMIT: usize = 5;

fn text_from_content(content: &Content) -> String {
	match content {
		Content::Text(text) => text.trim().to_string(),
		Content::Blocks(blocks) => blocks
			.iter()
			.filter_map(|block| {
				if let ContentBlock::Text { text } = block {
					let trimmed = text.trim();
					if trimmed.is_empty() {
						None
					} else {
						Some(trimmed.to_string())
					}
				} else {
					None
				}
			})
			.collect::<Vec<_>>()
			.join("\n"),
	}
}

fn latest_user_query_text(messages: &[Message]) -> Option<String> {
	messages.iter().rev().find_map(|message| {
		if message.role != Role::User {
			return None;
		}

		let text = text_from_content(&message.content);
		let trimmed = text.trim();
		if trimmed.is_empty() {
			None
		} else {
			Some(trimmed.to_string())
		}
	})
}

fn build_session_rag_context_message(
	app: &AppHandle,
	conversation_id: Option<&str>,
	query: &str,
) -> Result<Option<Message>, String> {
	let Some(scope_id) = conversation_id
		.map(|id| id.trim())
		.filter(|id| !id.is_empty())
	else {
		return Ok(None);
	};

	let query_text = query.trim();
	if query_text.chars().count() < 2 {
		return Ok(None);
	}

	let hits = crate::command::rag::rag_search_conversation_documents(
		app.clone(),
		scope_id.to_string(),
		query_text.to_string(),
		Some(SESSION_RAG_SEARCH_LIMIT),
	)?;

	if hits.is_empty() {
		return Ok(None);
	}

	let mut context_lines = vec![
		format!("{} Query: {}", SESSION_RAG_CONTEXT_MARKER, query_text),
		"Use the retrieved snippets below as supporting context. If they conflict with current repository reality or explicit user instructions, prioritize repository reality and user intent.".to_string(),
		"Retrieved snippets:".to_string(),
	];

	for (idx, hit) in hits.iter().enumerate() {
		context_lines.push(format!(
			"{}. {} (score={}, id={})",
			idx + 1,
			hit.source_name,
			hit.score,
			hit.id
		));
		context_lines.push(format!("   snippet: {}", hit.snippet));
	}

	Ok(Some(Message {
		role: Role::User,
		content: Content::Text(context_lines.join("\n")),
	}))
}

fn is_session_start_turn(messages: &[Message]) -> bool {
	let assistant_count = messages
		.iter()
		.filter(|m| m.role == Role::Assistant)
		.count();
	let user_count = messages
		.iter()
		.filter(|m| m.role == Role::User)
		.count();

	assistant_count == 0 && user_count <= 1
}

// 入口函数：发送用户聊天消息，驱动整轮 LLM 编排。
// 它负责：
// 1) 在真正请求模型前准备上下文（hooks / session restore / compact / session RAG）
// 2) 循环调用 provider，并把 provider 返回的新消息与 tool_result 回灌到 current_messages
// 3) 根据 needs_user_input / cancelled / prevent_continuation / has_tool_result 决定是否续跑
// 4) 在正常结束路径统一执行 session_end_hooks 并发送 stop 事件
// 5) 在 provider 错误路径执行 error_hooks，发送 stop(error) 后直接返回 Err
//
// send_chat_message
//     │
//     ├─ 1. 回合前准备
//     │       ├─ run_user_prompt_submit_hooks      → 追加提示提交上下文
//     │       └─ (首轮) run_session_start_hooks    → 追加会话开始上下文
//     │
//     ├─ 2. 上下文构建
//     │       ├─ context_assembler                 → 注入会话恢复上下文
//     │       ├─ run_pre_compact_hooks             → 压缩前上下文扩展
//     │       ├─ compact                           → 压缩历史消息 / 大型 tool_result
//     │       └─ session rag retrieval             → 仅按当前会话文档检索并注入上下文
//     │
//     ├─ 3. 主循环 loop
//     │       ├─ 取消检查                          → cancelled → break
//     │       ├─ 应用已提交的权限决策 / 维持审批状态
//     │       ├─ provider.send_request (流式)
//     │       │       └─ 错误: run_error_hooks + emit stop(error) + return Err
//     │       ├─ provider 报告 cancelled           → break
//     │       ├─ 合并新消息到 current_messages
//     │       ├─ needs_user_input                  → break
//     │       ├─ provider_result.prevent_continuation
//     │       │       └─ stop_hook_prevented       → break
//     │       ├─ has_tool_result                   → continue (下一轮，等待模型消费 tool_result)
//     │       └─ !has_tool_result
//     │               ├─ run_stop_hooks
//     │               │       ├─ prevent_continuation → break
//     │               │       └─ added_context → current_messages.extend → continue
//     │               └─ 正常结束                 → completed → break
//     │
//     └─ 4. 回合收尾（正常路径）
//             ├─ run_session_end_hooks             → 可覆盖 stop_reason
//             └─ emit stop                         → return Ok
pub async fn send_chat_message(
	app: AppHandle,
	conversation_id: Option<String>,
	messages: Vec<Message>,
	agent_mode: AgentMode,
) -> Result<(), String> {
	let rag_query = latest_user_query_text(&messages);
	let session_start_turn = is_session_start_turn(&messages);
	let mut turn_messages = messages;

	let tracer = FlowTracer::new(&app, conversation_id.as_deref());

	// ── 阶段 1: 提示提交 hook ────────────────────────────────────────────────
	tracer.emit("hook_prompt_submit", "提示提交 Hook", "running", None);
	let prompt_submit_hook = crate::llm::services::hooks::run_user_prompt_submit_hooks(
		&app,
		conversation_id.as_deref(),
	);
	let hook_detail = if prompt_submit_hook.additional_messages.is_empty() {
		Some("无附加消息".into())
	} else {
		Some(format!("注入 {} 条消息", prompt_submit_hook.additional_messages.len()))
	};
	tracer.emit("hook_prompt_submit", "提示提交 Hook", "completed", hook_detail);
	if !prompt_submit_hook.additional_messages.is_empty() {
		turn_messages.extend(prompt_submit_hook.additional_messages);
	}

	if session_start_turn {
		// ── 阶段 2: 会话启动 hook (首轮) ──────────────────────────────────────
		tracer.emit("hook_session_start", "会话启动 Hook", "running", None);
		let session_start_hook = crate::llm::services::hooks::run_session_start_hooks(
			&app,
			conversation_id.as_deref(),
		);
		let hook_detail = if session_start_hook.additional_messages.is_empty() {
			Some("无附加消息".into())
		} else {
			Some(format!("注入 {} 条消息", session_start_hook.additional_messages.len()))
		};
		tracer.emit("hook_session_start", "会话启动 Hook", "completed", hook_detail);
		if !session_start_hook.additional_messages.is_empty() {
			turn_messages.extend(session_start_hook.additional_messages);
		}
	} else {
		tracer.emit("hook_session_start", "会话启动 Hook", "skipped", Some("非首轮".into()));
	}

	// ── 阶段 3: 上下文组装 ────────────────────────────────────────────────────
	tracer.emit("context_assemble", "上下文组装", "running", None);
	let mut assembled_messages = context_assembler::assemble_messages_for_turn(
		&app,
		conversation_id.as_deref(),
		&turn_messages,
		AssembleOptions::default(),
	)
	.await;
	tracer.emit("context_assemble", "上下文组装", "completed", Some(FlowTracer::context_assemble_detail(&assembled_messages)));

	// ── 阶段 4: 压缩前 hook ───────────────────────────────────────────────────
	tracer.emit("hook_pre_compact", "压缩前 Hook", "running", None);
	let pre_compact_hook = crate::llm::services::hooks::run_pre_compact_hooks(
		&app,
		conversation_id.as_deref(),
	);
	let hook_detail = if pre_compact_hook.additional_messages.is_empty() {
		Some("无附加消息".into())
	} else {
		Some(format!("注入 {} 条消息", pre_compact_hook.additional_messages.len()))
	};
	tracer.emit("hook_pre_compact", "压缩前 Hook", "completed", hook_detail);
	if !pre_compact_hook.additional_messages.is_empty() {
		assembled_messages.extend(pre_compact_hook.additional_messages);
	}

	// ── 阶段 5: 历史压缩 ─────────────────────────────────────────────────────
	tracer.emit("compact", "历史压缩", "running", None);
	let before_tokens = compact::estimate_tokens_for_messages(&assembled_messages);
	let compact_plan = compact::describe_compact_plan(&assembled_messages);
	let mut current_messages = compact::compact_messages_for_turn(
		&app,
		conversation_id.as_deref(),
		&assembled_messages,
	)
	.await;
	let after_tokens = compact::estimate_tokens_for_messages(&current_messages);
	let diff_report = compact::describe_compact_diff(&assembled_messages, &current_messages);
	tracer.emit("compact", "历史压缩", "completed", Some(FlowTracer::compact_detail(
		&assembled_messages, &current_messages, before_tokens, after_tokens, &compact_plan, &diff_report,
	)));

	// ── 阶段 6: Session RAG 检索 ─────────────────────────────────────────────
	if let Some(query_text) = rag_query.as_deref() {
		tracer.emit("rag", "Session RAG 检索", "running", Some(format!("query: {}", &query_text[..query_text.len().min(60)])));
		match build_session_rag_context_message(&app, conversation_id.as_deref(), query_text) {
			Ok(Some(rag_context)) => {
				tracer.emit("rag", "Session RAG 检索", "completed", Some("已注入检索结果".into()));
				current_messages.push(rag_context);
			}
			Ok(None) => {
				tracer.emit("rag", "Session RAG 检索", "skipped", Some("无匹配文档".into()));
			}
			Err(e) => {
				tracer.emit("rag", "Session RAG 检索", "error", Some(e.clone()));
				emit_backend_error(
					&app,
					"llm.query.session_rag_context",
					e,
					Some("build_session_rag_context_message"),
				);
			}
		}
	} else {
		tracer.emit("rag", "Session RAG 检索", "skipped", Some("无用户文本".into()));
	}

	// 2. 根据设置选择模型提供方（Anthropic/OpenAI）。
	// Provider 实例封装了底层调用细节。
	let provider = LlmProvider::new(&app);

	// 3. 主循环：调用 provider.send_request（流式），并根据 tool 执行情况决定是否继续下一步。
	//    - 如果发生工具调用，结果会被“注入”到 current_messages 继续下一轮。
	//    - 如果 provider 返回 needs_user_input / 无工具结果，则结束。
	let mut final_outcome = loop {
		// 若收到取消请求，则立即以 cancelled 结束。
		if crate::llm::cancellation::is_cancelled(conversation_id.as_deref()) {
			break TurnOutcome::cancelled();
		}

		// 消费用户在前端对权限问题做出的审批决策。
		let consumed =
			crate::llm::utils::permissions::consume_user_permission_decisions(
				conversation_id.as_deref(),
				&current_messages,
			);
		// 若消费到决策，输出调试日志用于排查。
		if consumed > 0 {
			eprintln!("[permissions] applied user approval decisions={}", consumed);
		}

		// 发起 provider 请求并等待结果。
		// ── 最终上下文节点：展示发送给 AI 的每条完整消息 ────────────────────
		tracer.emit("context_final", "最终上下文", "completed", Some(FlowTracer::context_final_detail(&current_messages)));
		// ── LLM 请求节点：在调用前 emit 含 system prompt 完整内容 ─────────
		tracer.emit("llm", "Nova 推理", "running", Some(tracer.llm_detail(&current_messages, agent_mode)));
		let provider_result = match provider
			.send_request(&app, &current_messages, agent_mode, conversation_id.as_deref())
			.await
		{
			// 请求成功时拿到结果对象。
			Ok(v) => v,
			Err(e) => {
				let error_hook = crate::llm::services::hooks::run_error_hooks(
					&app,
					&e,
					conversation_id.as_deref(),
				);
				let error_text = error_hook.override_error.unwrap_or_else(|| e.clone());
				// 出错直接通知前端 stop(error) 并返回错误。
				// 同时上报后端错误事件用于统一监控。
				emit_backend_error(
					&app,
					"llm.query_engine",
					error_text.clone(),
					Some("provider.send_request"),
				);
				// 通知前端当前回合以错误状态结束。
				app.emit(
					"chat-stream",
					ChatMessageEvent {
						// 事件类型为 stop。
						r#type: "stop".into(),
						// 把错误文本透传给前端。
						text: Some(error_text.clone()),
						// 以下字段在 stop 事件中均为空。
						tool_use_id: None,
						tool_use_name: None,
						tool_use_input: None,
						tool_result: None,
						token_usage: None,
						// 停止原因标记为 provider_error。
						stop_reason: Some("provider_error".into()),
						// 回合状态标记为 error。
						turn_state: Some("error".into()),
						// 透传会话 ID，便于前端路由到正确会话。
						conversation_id: conversation_id.clone(),
					},
				)
				// 忽略 emit 错误，保证主错误路径返回。
				.ok();
				// 将 provider 错误返回给上层调用方。
				return Err(error_text);
			}
		};

		// provider 主动报告取消时，统一收敛为 cancelled。
		if provider_result.stop_reason.as_deref() == Some("cancelled") {
			break TurnOutcome::cancelled();
		}

		// 本轮 provider 输出合并到 current_messages 以支持工具环回。
		// 取出本轮新增消息。
		let new_messages = provider_result.messages;
		// 将新增消息并入上下文，供后续轮继续使用。
		current_messages.extend(new_messages.clone());

		// 输出调试日志，便于观察每轮消息增长。
		eprintln!("[loop] new_messages count={},the new messages are: {:?}", new_messages.len(), new_messages);

		// 判断新增消息中是否包含 tool_result 块。
		let has_tool_result = new_messages.iter().any(|m| {
			// 仅 blocks 结构里可能包含 tool_result。
			if let Content::Blocks(blocks) = &m.content {
				blocks
					.iter()
					// 只要有任意 ToolResult 块就判定为 true。
					.any(|b| matches!(b, ContentBlock::ToolResult { .. }))
			} else {
				// 非 blocks 内容不可能包含 tool_result。
				false
			}
		});

		// 输出工具结果检测日志。
		eprintln!("[loop] has_tool_result={}", has_tool_result);

		// 若返回需要用户输入，终止当前回合并告诉前端。
		if compact::has_needs_user_input(&new_messages) {
			break TurnOutcome::needs_user_input();
		}

		// 若 hook/provider 明确要求停止续跑，则按 stop_hook_prevented 结束。
		if provider_result.prevent_continuation {
			break TurnOutcome::stop_hook_prevented(
				provider_result
					.stop_reason
					// 未给停止原因时提供默认值。
					.unwrap_or_else(|| "hook_stopped_continuation".to_string()),
			);
		}

		// 若本轮没有工具结果，说明回合结束。
		if !has_tool_result {
			// 在回合结束前执行 stop hooks。
			tracer.emit("hook_stop", "Stop Hook", "running", None);
			let stop_hook_result =
				crate::llm::services::hooks::run_stop_hooks(&app, &current_messages, conversation_id.as_deref());
			// 判断 stop hooks 是否注入了附加上下文。
			let stop_hook_added_context = !stop_hook_result.additional_messages.is_empty();
			if stop_hook_added_context {
				tracer.emit("hook_stop", "Stop Hook", "completed", Some(format!("注入 {} 条消息", stop_hook_result.additional_messages.len())));
				// 将 stop hooks 注入的上下文并入当前消息。
				current_messages.extend(stop_hook_result.additional_messages);
			} else {
				tracer.emit("hook_stop", "Stop Hook", "completed", Some("无附加消息".into()));
			}

			// stop hooks 要求阻断续跑时立即结束。
			if stop_hook_result.prevent_continuation {
				break TurnOutcome::stop_hook_prevented(
					stop_hook_result
						.stop_reason
						// 缺省停止原因兜底。
						.unwrap_or_else(|| "stop_hook_prevented".to_string()),
				);
			}

			// 仅追加了上下文但未阻断时，继续下一轮请求。
			if stop_hook_added_context {
				continue;
			}

			// 正常结束本轮，若 provider 未给 stop_reason 则使用 end_turn。
			break TurnOutcome::completed(
				provider_result
					.stop_reason
					.unwrap_or_else(|| "end_turn".to_string()),
			);
		}
	};

	let session_end_hook = crate::llm::services::hooks::run_session_end_hooks(
		&app,
		&final_outcome.stop_reason,
		conversation_id.as_deref(),
	);
	tracer.emit("hook_session_end", "会话结束 Hook", "completed", Some(format!("stop_reason={}", &final_outcome.stop_reason)));
	if let Some(hooked_reason) = session_end_hook.stop_reason {
		final_outcome.stop_reason = hooked_reason;
	}

	// 4. 业务终止：告知前端本轮结束，并携带 stop_reason/turn_state 以区分 completed/needs_user_input/error。
	// 统一发送 stop 事件，前端据此收口渲染状态。
	app.emit(
		"chat-stream",
		ChatMessageEvent {
			// stop 事件类型。
			r#type: "stop".into(),
			// 正常 stop 不携带 text 内容。
			text: None,
			// stop 事件不绑定具体工具调用。
			tool_use_id: None,
			tool_use_name: None,
			tool_use_input: None,
			tool_result: None,
			// 本事件不附加 token_usage。
			token_usage: None,
			// 透传最终停止原因。
			stop_reason: Some(final_outcome.stop_reason),
			// 透传最终回合状态字符串。
			turn_state: Some(final_outcome.turn_state.as_event_state().to_string()),
			// 透传会话 ID，便于前端路由到正确会话。
			conversation_id: conversation_id.clone(),
		},
	)
	// stop 事件投递失败不影响函数返回。
	.ok();

	// 全流程成功完成。
	Ok(())
}
