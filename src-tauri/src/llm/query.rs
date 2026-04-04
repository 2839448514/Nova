use tauri::{AppHandle, Emitter};

use crate::llm::providers::LlmProvider;
use crate::llm::query_engine::ChatMessageEvent;
use crate::llm::services::compact;
use crate::llm::types::{AgentMode, Content, ContentBlock, Message};
use crate::llm::utils::context_assembler::{self, AssembleOptions};
use crate::llm::utils::error_event::emit_backend_error;

mod state_machine;

use state_machine::TurnOutcome;

// 入口函数：发送用户聊天消息，驱动 LLM 请求和工具调用流程。
// 这个函数负责准备消息、循环调度 provider、处理 tool-result 掉回、最后发送 stop 事件。
// send_chat_message
//     │
//     ├─ 1. context_assembler   → 注入会话恢复上下文
//     ├─ 2. compact             → 压缩历史消息
//     │
//     └─ 3. 主循环 loop
//             ├─ 取消检查                          → cancelled → break
//             ├─ 权限决策消费
//             ├─ provider.send_request (流式)      → 错误 → emit stop(error) → return Err
//             ├─ provider 报告 cancelled           → break
//             ├─ 合并新消息到 current_messages
//             ├─ provider.prevent_continuation     → stop_hook_prevented → break
//             ├─ 工具结果检测
//             │       ├─ has_tool_result           → continue (下一轮)
//             │       └─ !has_tool_result
//             │               ├─ run_stop_hooks
//             │               │       ├─ prevent_continuation → break
//             │               │       └─ added_context → current_messages.extend → continue
//             │               └─ 正常结束           → completed → break
//             └─ needs_user_input                  → break
pub async fn send_chat_message(
	app: AppHandle,
	conversation_id: Option<String>,
	messages: Vec<Message>,
	agent_mode: AgentMode,
) -> Result<(), String> {
	// 1. 先组装上下文（会话恢复等），再执行压缩。
	let assembled_messages = context_assembler::assemble_messages_for_turn(
		&app,
		conversation_id.as_deref(),
		&messages,
		AssembleOptions::default(),
	)
	.await;
	let mut current_messages = compact::compact_messages_for_turn(
		&app,
		conversation_id.as_deref(),
		&assembled_messages,
	)
	.await;

	// 2. 根据设置选择模型提供方（Anthropic/OpenAI）。
	// Provider 实例封装了底层调用细节。
	let provider = LlmProvider::new(&app);

	// 3. 主循环：调用 provider.send_request（流式），并根据 tool 执行情况决定是否继续下一步。
	//    - 如果发生工具调用，结果会被“注入”到 current_messages 继续下一轮。
	//    - 如果 provider 返回 needs_user_input / 无工具结果，则结束。
	let final_outcome = loop {
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
		let provider_result = match provider
			.send_request(&app, &current_messages, agent_mode, conversation_id.as_deref())
			.await
		{
			// 请求成功时拿到结果对象。
			Ok(v) => v,
			Err(e) => {
				// 出错直接通知前端 stop(error) 并返回错误。
				// 同时上报后端错误事件用于统一监控。
				emit_backend_error(
					&app,
					"llm.query_engine",
					e.clone(),
					Some("provider.send_request"),
				);
				// 通知前端当前回合以错误状态结束。
				app.emit(
					"chat-stream",
					ChatMessageEvent {
						// 事件类型为 stop。
						r#type: "stop".into(),
						// 把错误文本透传给前端。
						text: Some(e.clone()),
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
					},
				)
				// 忽略 emit 错误，保证主错误路径返回。
				.ok();
				// 将 provider 错误返回给上层调用方。
				return Err(e);
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
			let stop_hook_result =
				crate::llm::services::tools::run_stop_hooks(&current_messages, conversation_id.as_deref());
			// 判断 stop hooks 是否注入了附加上下文。
			let stop_hook_added_context = !stop_hook_result.additional_messages.is_empty();
			if stop_hook_added_context {
				// 将 stop hooks 注入的上下文并入当前消息。
				current_messages.extend(stop_hook_result.additional_messages);
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
		},
	)
	// stop 事件投递失败不影响函数返回。
	.ok();

	// 全流程成功完成。
	Ok(())
}
