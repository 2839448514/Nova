use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn cancel_state() -> &'static Mutex<HashMap<String, bool>> {
    static STATE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn scope_key(conversation_id: Option<&str>) -> String {
    conversation_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or("__default__")
        .to_string()
}

pub fn begin_turn(conversation_id: Option<&str>) {
    let key = scope_key(conversation_id);
    let mut state = cancel_state().lock().unwrap_or_else(|e| e.into_inner());
    state.insert(key, false);
}

pub fn finish_turn(conversation_id: Option<&str>) {
    let key = scope_key(conversation_id);
    let mut state = cancel_state().lock().unwrap_or_else(|e| e.into_inner());
    state.remove(&key);
}

pub fn request_cancel(conversation_id: Option<&str>) -> bool {
    let key = scope_key(conversation_id);
    let mut state = cancel_state().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(flag) = state.get_mut(&key) {
        *flag = true;
        true
    } else {
        false
    }
}

pub fn is_cancelled(conversation_id: Option<&str>) -> bool {
    let key = scope_key(conversation_id);
    let state = cancel_state().lock().unwrap_or_else(|e| e.into_inner());
    state.get(&key).copied().unwrap_or(false)
}