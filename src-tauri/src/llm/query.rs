use tauri::{AppHandle, Emitter};

use crate::llm::providers::LlmProvider;
use crate::llm::query_engine::ChatMessageEvent;
use crate::llm::services::compact;
use crate::llm::types::{Content, ContentBlock, Message, Role};
use crate::llm::utils::error_event::emit_backend_error;

mod state_machine;

use state_machine::TurnOutcome;

// 当本轮输出达到预算 90% 以上时，不再触发续跑提示。
const TOKEN_BUDGET_COMPLETION_THRESHOLD_PERCENT: i64 = 90;
// 连续续跑时，若新增 token 低于该阈值则判定收益递减。
const TOKEN_BUDGET_DIMINISHING_THRESHOLD_TOKENS: i64 = 500;
// 未配置时允许的默认最大续跑次数。
const TOKEN_BUDGET_DEFAULT_MAX_CONTINUATIONS: u32 = 6;

#[derive(Debug, Default)]
struct BudgetTracker {
	// 已触发的续跑次数。
	continuation_count: u32,
	// 最近一次检查相对上一轮的增量 token。
	last_delta_tokens: i64,
	// 最近一次检查时累计的输出 token。
	last_turn_tokens: i64,
}

fn env_positive_i64(key: &str) -> Option<i64> {
	// 从环境变量读取原始字符串。
	std::env::var(key)
		// 环境变量不存在时转为 None。
		.ok()
		// 字符串去空白后尝试解析为 i64。
		.and_then(|v| v.trim().parse::<i64>().ok())
		// 仅保留正数值。
		.filter(|v| *v > 0)
}

fn token_budget_for_turn() -> Option<i64> {
	// 读取每轮预算配置（正整数）。
	env_positive_i64("NOVA_TURN_TOKEN_BUDGET")
}

fn token_budget_max_continuations() -> u32 {
	// 读取最大续跑次数配置。
	std::env::var("NOVA_TURN_TOKEN_BUDGET_MAX_CONTINUATIONS")
		// 未配置时转为 None。
		.ok()
		// 去空白并尝试解析为 u32。
		.and_then(|v| v.trim().parse::<u32>().ok())
		// 仅接受大于 0 的配置值。
		.filter(|v| *v > 0)
		// 配置缺失或非法时回落到默认值。
		.unwrap_or(TOKEN_BUDGET_DEFAULT_MAX_CONTINUATIONS)
}

fn estimate_assistant_output_tokens(messages: &[Message]) -> i64 {
	// 从消息列表中过滤 assistant 角色消息。
	let assistant_messages = messages
		// 遍历消息切片。
		.iter()
		// 仅保留 assistant 消息。
		.filter(|m| m.role == Role::Assistant)
		// 克隆消息供后续估算使用。
		.cloned()
		// 收集为向量。
		.collect::<Vec<_>>();
	// 使用 compact 模块统一估算 token 数。
	compact::estimate_tokens_for_messages(&assistant_messages)
}

fn next_token_budget_nudge(
	tracker: &mut BudgetTracker,
	budget: i64,
	turn_output_tokens: i64,
	max_continuations: u32,
) -> Option<String> {
	// 预算或输出无效时不触发续跑。
	if budget <= 0 || turn_output_tokens <= 0 {
		return None;
	}

	// 超过最大续跑次数时不再继续。
	if tracker.continuation_count >= max_continuations {
		return None;
	}

	// 计算相对上次检查新增的 token。
	let delta_since_last_check = turn_output_tokens - tracker.last_turn_tokens;
	// 判断是否进入收益递减状态。
	let is_diminishing = tracker.continuation_count >= 3
		&& delta_since_last_check < TOKEN_BUDGET_DIMINISHING_THRESHOLD_TOKENS
		&& tracker.last_delta_tokens < TOKEN_BUDGET_DIMINISHING_THRESHOLD_TOKENS;

	// 未递减且仍低于预算完成阈值时，注入续跑提示。
	if !is_diminishing
		&& turn_output_tokens < (budget * TOKEN_BUDGET_COMPLETION_THRESHOLD_PERCENT) / 100
	{
		// 续跑计数加一。
		tracker.continuation_count += 1;
		// 记录本次增量用于下轮递减判断。
		tracker.last_delta_tokens = delta_since_last_check;
		// 记录本次累计输出。
		tracker.last_turn_tokens = turn_output_tokens;

		// 计算已用预算百分比并限制显示范围。
		let pct = ((turn_output_tokens * 100) / budget).clamp(0, 999);
		// 返回追加给模型的续跑指令文本。
		return Some(format!(
			"[TokenBudget] continuation #{} ({}% of budget, {} / {} tokens). Continue directly without recap and finish the remaining work in smaller chunks.",
			tracker.continuation_count,
			pct,
			turn_output_tokens,
			budget,
		));
	}

	None
}

// 检查一轮消息里是否已经包含过会话恢复标记，避免重复叠加恢复上下文。
fn has_session_restore_marker(messages: &[Message]) -> bool {
	// 任意消息命中恢复标记即返回 true。
	messages.iter().any(|m| match &m.content {
		// 纯文本消息直接检查标记子串。
		Content::Text(t) => t.contains("[Session Restore Context]"),
		// 多块内容消息需要逐块检查文本块。
		Content::Blocks(blocks) => blocks.iter().any(|b| {
			// 仅文本块参与标记匹配。
			if let ContentBlock::Text { text } = b {
				text.contains("[Session Restore Context]")
			} else {
				// 非文本块不视为命中。
				false
			}
		}),
	})
}

// 入口函数：发送用户聊天消息，驱动 LLM 请求和工具调用流程。
// 这个函数负责准备消息、循环调度 provider、处理 tool-result 掉回、最后发送 stop 事件。
pub async fn send_chat_message(
	app: AppHandle,
	conversation_id: Option<String>,
	messages: Vec<Message>,
	plan_mode: bool,
) -> Result<(), String> {
	// 1. 预处理消息：把用户本轮输入和历史消息压缩为本次模型请求的 current_messages。
	// 这里会做上下文裁剪和必要格式整理。
	let mut current_messages =
		compact::prepare_messages_for_turn(&app, conversation_id.as_deref(), &messages).await;

	// 2. 如果有会话 ID，尝试插入会话恢复上下文（仅当当前内容里未标记时）。
	//    这块会返回类似: "[Session Restore Context] ..." 的 system/user 信息。
	if let Some(conversation_id) = conversation_id.as_deref() {
		// 只有当前消息尚未包含恢复标记时才补充恢复上下文。
		if !has_session_restore_marker(&current_messages) {
			// 从会话历史中构建恢复消息。
			if let Some(restore_msg) =
				crate::llm::utils::session_restore::build_resume_context_message(
					&app,
					conversation_id,
				)
				.await
			{
				// 将恢复消息插入消息头部，让模型优先看到。
				current_messages.insert(0, restore_msg);
			}
		}
	}

	// 3. 根据设置选择模型提供方（Anthropic/OpenAI）。
	// Provider 实例封装了底层调用细节。
	let provider = LlmProvider::new(&app);
	// 读取本轮 token 预算（可选）。
	let token_budget = token_budget_for_turn();
	// 读取本轮最大续跑次数。
	let max_budget_continuations = token_budget_max_continuations();
	// 初始化预算跟踪器。
	let mut budget_tracker = BudgetTracker::default();
	// 统计本轮累计输出 token。
	let mut turn_output_tokens: i64 = 0;

	// 4. 主循环：调用 provider.send_request（流式），并根据 tool 执行情况决定是否继续下一步。
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
			.send_request(&app, &current_messages, plan_mode, conversation_id.as_deref())
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
		// 先累加 provider 显式上报的 output token；缺失时走估算。
		turn_output_tokens += provider_result
			.output_tokens
			.map(|v| v as i64)
			.unwrap_or_else(|| estimate_assistant_output_tokens(&new_messages));
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

			// 当启用 token_budget 时尝试生成续跑 nudge。
			if let Some(budget) = token_budget {
				if let Some(nudge) = next_token_budget_nudge(
					&mut budget_tracker,
					budget,
					turn_output_tokens,
					max_budget_continuations,
				) {
					// 将续跑提示以用户消息形式压入上下文。
					current_messages.push(Message {
						// 续跑提示使用 user 角色，以驱动下一轮继续输出。
						role: Role::User,
						// 提示内容为文本块。
						content: Content::Text(nudge),
					});
					// 继续下一轮循环。
					continue;
				}
			}

			// 正常结束本轮，若 provider 未给 stop_reason 则使用 end_turn。
			break TurnOutcome::completed(
				provider_result
					.stop_reason
					.unwrap_or_else(|| "end_turn".to_string()),
			);
		}
	};

	// 5. 业务终止：告知前端本轮结束，并携带 stop_reason/turn_state 以区分 completed/needs_user_input/error。
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
