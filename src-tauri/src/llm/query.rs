use tauri::{AppHandle, Emitter};

use crate::llm::providers::LlmProvider;
use crate::llm::query_engine::ChatMessageEvent;
use crate::llm::services::compact;
use crate::llm::types::{Content, ContentBlock, Message, Role};
use crate::llm::utils::error_event::emit_backend_error;

mod state_machine;

use state_machine::TurnOutcome;

const TOKEN_BUDGET_COMPLETION_THRESHOLD_PERCENT: i64 = 90;
const TOKEN_BUDGET_DIMINISHING_THRESHOLD_TOKENS: i64 = 500;
const TOKEN_BUDGET_DEFAULT_MAX_CONTINUATIONS: u32 = 6;

#[derive(Debug, Default)]
struct BudgetTracker {
	continuation_count: u32,
	last_delta_tokens: i64,
	last_turn_tokens: i64,
}

fn env_positive_i64(key: &str) -> Option<i64> {
	std::env::var(key)
		.ok()
		.and_then(|v| v.trim().parse::<i64>().ok())
		.filter(|v| *v > 0)
}

fn token_budget_for_turn() -> Option<i64> {
	env_positive_i64("NOVA_TURN_TOKEN_BUDGET")
}

fn token_budget_max_continuations() -> u32 {
	std::env::var("NOVA_TURN_TOKEN_BUDGET_MAX_CONTINUATIONS")
		.ok()
		.and_then(|v| v.trim().parse::<u32>().ok())
		.filter(|v| *v > 0)
		.unwrap_or(TOKEN_BUDGET_DEFAULT_MAX_CONTINUATIONS)
}

fn estimate_assistant_output_tokens(messages: &[Message]) -> i64 {
	let assistant_messages = messages
		.iter()
		.filter(|m| m.role == Role::Assistant)
		.cloned()
		.collect::<Vec<_>>();
	compact::estimate_tokens_for_messages(&assistant_messages)
}

fn next_token_budget_nudge(
	tracker: &mut BudgetTracker,
	budget: i64,
	turn_output_tokens: i64,
	max_continuations: u32,
) -> Option<String> {
	if budget <= 0 || turn_output_tokens <= 0 {
		return None;
	}

	if tracker.continuation_count >= max_continuations {
		return None;
	}

	let delta_since_last_check = turn_output_tokens - tracker.last_turn_tokens;
	let is_diminishing = tracker.continuation_count >= 3
		&& delta_since_last_check < TOKEN_BUDGET_DIMINISHING_THRESHOLD_TOKENS
		&& tracker.last_delta_tokens < TOKEN_BUDGET_DIMINISHING_THRESHOLD_TOKENS;

	if !is_diminishing
		&& turn_output_tokens < (budget * TOKEN_BUDGET_COMPLETION_THRESHOLD_PERCENT) / 100
	{
		tracker.continuation_count += 1;
		tracker.last_delta_tokens = delta_since_last_check;
		tracker.last_turn_tokens = turn_output_tokens;

		let pct = ((turn_output_tokens * 100) / budget).clamp(0, 999);
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
	messages.iter().any(|m| match &m.content {
		Content::Text(t) => t.contains("[Session Restore Context]"),
		Content::Blocks(blocks) => blocks.iter().any(|b| {
			if let ContentBlock::Text { text } = b {
				text.contains("[Session Restore Context]")
			} else {
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
	let mut current_messages =
		compact::prepare_messages_for_turn(&app, conversation_id.as_deref(), &messages).await;

	// 2. 如果有会话 ID，尝试插入会话恢复上下文（仅当当前内容里未标记时）。
	//    这块会返回类似: "[Session Restore Context] ..." 的 system/user 信息。
	if let Some(conversation_id) = conversation_id.as_deref() {
		if !has_session_restore_marker(&current_messages) {
			if let Some(restore_msg) =
				crate::llm::utils::session_restore::build_resume_context_message(
					&app,
					conversation_id,
				)
				.await
			{
				current_messages.insert(0, restore_msg);
			}
		}
	}

	// 3. 根据设置选择模型提供方（Anthropic/OpenAI）。
	let provider = LlmProvider::new(&app);
	let token_budget = token_budget_for_turn();
	let max_budget_continuations = token_budget_max_continuations();
	let mut budget_tracker = BudgetTracker::default();
	let mut turn_output_tokens: i64 = 0;

	// 4. 主循环：调用 provider.send_request（流式），并根据 tool 执行情况决定是否继续下一步。
	//    - 如果发生工具调用，结果会被“注入”到 current_messages 继续下一轮。
	//    - 如果 provider 返回 needs_user_input / 无工具结果，则结束。
	let final_outcome = loop {
		if crate::llm::cancellation::is_cancelled(conversation_id.as_deref()) {
			break TurnOutcome::cancelled();
		}

		let consumed =
			crate::llm::utils::permissions::consume_user_permission_decisions(
				conversation_id.as_deref(),
				&current_messages,
			);
		if consumed > 0 {
			eprintln!("[permissions] applied user approval decisions={}", consumed);
		}

		let provider_result = match provider
			.send_request(&app, &current_messages, plan_mode, conversation_id.as_deref())
			.await
		{
			Ok(v) => v,
			Err(e) => {
				// 出错直接通知前端 stop(error) 并返回错误。
				emit_backend_error(
					&app,
					"llm.query_engine",
					e.clone(),
					Some("provider.send_request"),
				);
				app.emit(
					"chat-stream",
					ChatMessageEvent {
						r#type: "stop".into(),
						text: Some(e.clone()),
						tool_use_id: None,
						tool_use_name: None,
						tool_use_input: None,
						tool_result: None,
						token_usage: None,
						stop_reason: Some("provider_error".into()),
						turn_state: Some("error".into()),
					},
				)
				.ok();
				return Err(e);
			}
		};

		if provider_result.stop_reason.as_deref() == Some("cancelled") {
			break TurnOutcome::cancelled();
		}

		// 本轮 provider 输出合并到 current_messages 以支持工具环回。
		let new_messages = provider_result.messages;
		turn_output_tokens += provider_result
			.output_tokens
			.map(|v| v as i64)
			.unwrap_or_else(|| estimate_assistant_output_tokens(&new_messages));
		current_messages.extend(new_messages.clone());

		eprintln!("[loop] new_messages count={},the new messages are: {:?}", new_messages.len(), new_messages);

		let has_tool_result = new_messages.iter().any(|m| {
			if let Content::Blocks(blocks) = &m.content {
				blocks
					.iter()
					.any(|b| matches!(b, ContentBlock::ToolResult { .. }))
			} else {
				false
			}
		});

		eprintln!("[loop] has_tool_result={}", has_tool_result);

		// 若返回需要用户输入，终止当前回合并告诉前端。
		if compact::has_needs_user_input(&new_messages) {
			break TurnOutcome::needs_user_input();
		}

		if provider_result.prevent_continuation {
			break TurnOutcome::stop_hook_prevented(
				provider_result
					.stop_reason
					.unwrap_or_else(|| "hook_stopped_continuation".to_string()),
			);
		}

		// 若本轮没有工具结果，说明回合结束。
		if !has_tool_result {
			let stop_hook_result =
				crate::llm::services::tools::run_stop_hooks(&current_messages, conversation_id.as_deref());
			let stop_hook_added_context = !stop_hook_result.additional_messages.is_empty();
			if stop_hook_added_context {
				current_messages.extend(stop_hook_result.additional_messages);
			}

			if stop_hook_result.prevent_continuation {
				break TurnOutcome::stop_hook_prevented(
					stop_hook_result
						.stop_reason
						.unwrap_or_else(|| "stop_hook_prevented".to_string()),
				);
			}

			if stop_hook_added_context {
				continue;
			}

			if let Some(budget) = token_budget {
				if let Some(nudge) = next_token_budget_nudge(
					&mut budget_tracker,
					budget,
					turn_output_tokens,
					max_budget_continuations,
				) {
					current_messages.push(Message {
						role: Role::User,
						content: Content::Text(nudge),
					});
					continue;
				}
			}

			break TurnOutcome::completed(
				provider_result
					.stop_reason
					.unwrap_or_else(|| "end_turn".to_string()),
			);
		}
	};

	// 5. 业务终止：告知前端本轮结束，并携带 stop_reason/turn_state 以区分 completed/needs_user_input/error。
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
			stop_reason: Some(final_outcome.stop_reason),
			turn_state: Some(final_outcome.turn_state.as_event_state().to_string()),
		},
	)
	.ok();

	Ok(())
}
