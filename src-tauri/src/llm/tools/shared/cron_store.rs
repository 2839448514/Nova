use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub recurring: bool,
    pub durable: bool,
    pub created_at: String,
}

static SESSION_CRON_JOBS: OnceLock<Mutex<Vec<CronJob>>> = OnceLock::new();

fn session_store() -> &'static Mutex<Vec<CronJob>> {
    SESSION_CRON_JOBS.get_or_init(|| Mutex::new(Vec::new()))
}

fn durable_store_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("scheduled_tasks.json"))
        .map_err(|e| format!("Failed to resolve app_data_dir for scheduled tasks: {}", e))
}

fn read_durable_jobs(app: &AppHandle) -> Result<Vec<CronJob>, String> {
    let path = durable_store_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read scheduled tasks file: {}", e))?;

    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str::<Vec<CronJob>>(&raw)
        .map_err(|e| format!("Invalid JSON in scheduled tasks file: {}", e))
}

fn write_durable_jobs(app: &AppHandle, jobs: &[CronJob]) -> Result<(), String> {
    let path = durable_store_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create scheduled tasks directory: {}", e))?;
    }

    let serialized = serde_json::to_string_pretty(jobs)
        .map_err(|e| format!("Failed to serialize scheduled tasks: {}", e))?;

    fs::write(path, serialized).map_err(|e| format!("Failed to write scheduled tasks file: {}", e))
}

pub fn add_job(app: &AppHandle, job: CronJob) -> Result<CronJob, String> {
    if job.durable {
        let mut durable_jobs = read_durable_jobs(app)?;
        durable_jobs.push(job.clone());
        write_durable_jobs(app, &durable_jobs)?;
        return Ok(job);
    }

    let mut jobs = session_store()
        .lock()
        .map_err(|_| "SESSION_CRON_JOBS mutex poisoned".to_string())?;
    jobs.push(job.clone());
    Ok(job)
}

pub fn list_jobs(app: &AppHandle) -> Result<Vec<CronJob>, String> {
    let mut jobs = session_store()
        .lock()
        .map_err(|_| "SESSION_CRON_JOBS mutex poisoned".to_string())?
        .clone();

    let mut durable_jobs = read_durable_jobs(app)?;
    jobs.append(&mut durable_jobs);
    jobs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(jobs)
}

pub fn remove_job(app: &AppHandle, id: &str) -> Result<bool, String> {
    let mut removed = false;

    {
        let mut jobs = session_store()
            .lock()
            .map_err(|_| "SESSION_CRON_JOBS mutex poisoned".to_string())?;
        let before = jobs.len();
        jobs.retain(|job| job.id != id);
        removed = jobs.len() != before;
    }

    let mut durable_jobs = read_durable_jobs(app)?;
    let before = durable_jobs.len();
    durable_jobs.retain(|job| job.id != id);
    if durable_jobs.len() != before {
        write_durable_jobs(app, &durable_jobs)?;
        removed = true;
    }

    Ok(removed)
}
