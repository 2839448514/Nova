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

// 返回任务内存仓库的全局单例。
// 这里的 Vec 只保存当前进程生命周期内的任务列表。
fn tasks_store() -> &'static Mutex<Vec<TodoItem>> {
    TASKS.get_or_init(|| Mutex::new(Vec::new()))
}

// 生成下一个任务 id。
// NEXT_ID 是一个递增计数器，每创建一个任务就加一。
fn next_id() -> u64 {
    let lock = NEXT_ID.get_or_init(|| Mutex::new(1));
    let mut id = lock.lock().expect("NEXT_ID mutex poisoned");
    let out = *id;
    *id += 1;
    out
}

// 返回当前内存里的全部任务快照。
pub fn list() -> Vec<TodoItem> {
    tasks_store().lock().expect("TASKS mutex poisoned").clone()
}

// 新建一个任务并加入内存仓库。
// `title`/`status`/`notes` 就是 TodoItem 的三个业务字段。
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

// 按 id 更新一个任务的部分字段。
// `notes: Option<Option<String>>` 的双层 Option 用来区分“不改 notes”和“把 notes 清空”。
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

// 按 id 读取单个任务。
pub fn get(id: u64) -> Option<TodoItem> {
    tasks_store()
        .lock()
        .expect("TASKS mutex poisoned")
        .iter()
        .find(|t| t.id == id)
        .cloned()
}

// 清空当前进程内保存的全部任务。
pub fn clear() {
    tasks_store().lock().expect("TASKS mutex poisoned").clear();
}

// 用一组全新的任务内容替换当前仓库。
// `items` 只提供 title/status/notes，id 会在这里重新分配。
pub fn replace_all(items: Vec<(String, String, Option<String>)>) -> Vec<TodoItem> {
    clear();
    let mut out = Vec::new();
    for (title, status, notes) in items {
        out.push(create(title, status, notes));
    }
    out
}
