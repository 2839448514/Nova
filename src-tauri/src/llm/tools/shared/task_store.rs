use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: u64,
    pub title: String,
    pub status: String,
    pub notes: Option<String>,
}

static TASKS: OnceLock<Mutex<Vec<TodoItem>>> = OnceLock::new();
static NEXT_ID: OnceLock<Mutex<u64>> = OnceLock::new();

fn tasks_store() -> &'static Mutex<Vec<TodoItem>> {
    TASKS.get_or_init(|| Mutex::new(Vec::new()))
}

fn next_id() -> u64 {
    let lock = NEXT_ID.get_or_init(|| Mutex::new(1));
    let mut id = lock.lock().expect("NEXT_ID mutex poisoned");
    let out = *id;
    *id += 1;
    out
}

pub fn list() -> Vec<TodoItem> {
    tasks_store().lock().expect("TASKS mutex poisoned").clone()
}

pub fn create(title: String, status: String, notes: Option<String>) -> TodoItem {
    let mut tasks = tasks_store().lock().expect("TASKS mutex poisoned");
    let item = TodoItem {
        id: next_id(),
        title,
        status,
        notes,
    };
    tasks.push(item.clone());
    item
}

pub fn update(
    id: u64,
    title: Option<String>,
    status: Option<String>,
    notes: Option<Option<String>>,
) -> Option<TodoItem> {
    let mut tasks = tasks_store().lock().expect("TASKS mutex poisoned");
    let task = tasks.iter_mut().find(|t| t.id == id)?;

    if let Some(t) = title {
        task.title = t;
    }
    if let Some(s) = status {
        task.status = s;
    }
    if let Some(n) = notes {
        task.notes = n;
    }

    Some(task.clone())
}

pub fn get(id: u64) -> Option<TodoItem> {
    tasks_store()
        .lock()
        .expect("TASKS mutex poisoned")
        .iter()
        .find(|t| t.id == id)
        .cloned()
}

pub fn clear() {
    tasks_store().lock().expect("TASKS mutex poisoned").clear();
}

pub fn replace_all(items: Vec<(String, String, Option<String>)>) -> Vec<TodoItem> {
    clear();
    let mut out = Vec::new();
    for (title, status, notes) in items {
        out.push(create(title, status, notes));
    }
    out
}
