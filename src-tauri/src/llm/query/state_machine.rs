#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    Completed,
    NeedsUserInput,
    Cancelled,
    StopHookPrevented,
}

impl TurnState {
    pub fn as_event_state(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::NeedsUserInput => "needs_user_input",
            Self::Cancelled => "cancelled",
            Self::StopHookPrevented => "stop_hook_prevented",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TurnOutcome {
    pub stop_reason: String,
    pub turn_state: TurnState,
}

impl TurnOutcome {
    pub fn completed(stop_reason: impl Into<String>) -> Self {
        Self {
            stop_reason: stop_reason.into(),
            turn_state: TurnState::Completed,
        }
    }

    pub fn needs_user_input() -> Self {
        Self {
            stop_reason: "needs_user_input".to_string(),
            turn_state: TurnState::NeedsUserInput,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            stop_reason: "cancelled".to_string(),
            turn_state: TurnState::Cancelled,
        }
    }

    pub fn stop_hook_prevented(stop_reason: impl Into<String>) -> Self {
        Self {
            stop_reason: stop_reason.into(),
            turn_state: TurnState::StopHookPrevented,
        }
    }
}
