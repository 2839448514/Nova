use crate::llm::tools::shared::cron_store::{add_job, list_jobs, remove_job, CronJob};
use chrono::{DateTime, Datelike, Local, Timelike, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, Emitter};
use tokio::time::{self, Duration};
use uuid::Uuid;

const SCHEDULER_TICK_SECONDS: u64 = 15;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskTriggerEvent {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub recurring: bool,
    pub durable: bool,
    pub created_at: String,
    pub triggered_at: String,
}

fn parse_field_range(index: usize) -> (u32, u32) {
    match index {
        0 => (0, 59),
        1 => (0, 23),
        2 => (1, 31),
        3 => (1, 12),
        4 => (0, 7),
        _ => (0, 0),
    }
}

fn parse_number_in_range(raw: &str, min: u32, max: u32) -> bool {
    raw.parse::<u32>()
        .ok()
        .map(|v| v >= min && v <= max)
        .unwrap_or(false)
}

fn normalize_day_of_week(value: u32) -> u32 {
    if value == 7 {
        0
    } else {
        value
    }
}

fn parse_number_for_match(raw: &str, min: u32, max: u32, day_of_week: bool) -> Option<u32> {
    let parsed = raw.parse::<u32>().ok()?;
    let normalized = if day_of_week {
        normalize_day_of_week(parsed)
    } else {
        parsed
    };

    if normalized < min || normalized > max {
        return None;
    }

    Some(normalized)
}

fn validate_cron_segment(segment: &str, min: u32, max: u32) -> bool {
    if segment.is_empty() {
        return false;
    }

    let (base, step) = match segment.split_once('/') {
        Some((base, step)) => (base, Some(step)),
        None => (segment, None),
    };

    if let Some(step_raw) = step {
        let valid_step = step_raw
            .parse::<u32>()
            .ok()
            .map(|v| v > 0)
            .unwrap_or(false);
        if !valid_step {
            return false;
        }
    }

    if base == "*" {
        return true;
    }

    if let Some((start, end)) = base.split_once('-') {
        let valid_start = parse_number_in_range(start, min, max);
        let valid_end = parse_number_in_range(end, min, max);
        if !valid_start || !valid_end {
            return false;
        }
        let s = start.parse::<u32>().ok().unwrap_or(0);
        let e = end.parse::<u32>().ok().unwrap_or(0);
        return s <= e;
    }

    parse_number_in_range(base, min, max)
}

fn validate_cron_field(field: &str, min: u32, max: u32) -> bool {
    field
        .split(',')
        .all(|segment| validate_cron_segment(segment.trim(), min, max))
}

fn cron_segment_matches(
    segment: &str,
    value: u32,
    min: u32,
    max: u32,
    day_of_week: bool,
) -> bool {
    let (base, step) = match segment.split_once('/') {
        Some((base, step)) => (base.trim(), Some(step.trim())),
        None => (segment.trim(), None),
    };

    let step_value = step
        .map(|raw| raw.parse::<u32>().ok().unwrap_or(0))
        .unwrap_or(1);
    if step_value == 0 {
        return false;
    }

    let (start, end) = if base == "*" {
        (min, max)
    } else if let Some((raw_start, raw_end)) = base.split_once('-') {
        let Some(start) = parse_number_for_match(raw_start.trim(), min, max, day_of_week) else {
            return false;
        };
        let Some(end) = parse_number_for_match(raw_end.trim(), min, max, day_of_week) else {
            return false;
        };
        if start > end {
            return false;
        }
        (start, end)
    } else {
        let Some(exact) = parse_number_for_match(base, min, max, day_of_week) else {
            return false;
        };
        (exact, exact)
    };

    if value < start || value > end {
        return false;
    }

    if step_value == 1 {
        return true;
    }

    (value - start) % step_value == 0
}

fn cron_field_matches(field: &str, value: u32, min: u32, max: u32, day_of_week: bool) -> bool {
    field
        .split(',')
        .any(|segment| cron_segment_matches(segment.trim(), value, min, max, day_of_week))
}

fn validate_cron_expression(expr: &str) -> Result<(), String> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return Err("Cron expression must contain exactly 5 fields: M H DoM Mon DoW".to_string());
    }

    for (index, field) in fields.iter().enumerate() {
        let (min, max) = parse_field_range(index);
        if !validate_cron_field(field.trim(), min, max) {
            return Err(format!(
                "Invalid cron field {}='{}'. Expected range {}-{} with optional *, -, /, ,",
                index + 1,
                field,
                min,
                max
            ));
        }
    }

    Ok(())
}

fn cron_matches_local_now(expr: &str, now: &DateTime<Local>) -> bool {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return false;
    }

    let minute = now.minute();
    let hour = now.hour();
    let day_of_month = now.day();
    let month = now.month();
    let day_of_week = now.weekday().num_days_from_sunday();

    cron_field_matches(fields[0], minute, 0, 59, false)
        && cron_field_matches(fields[1], hour, 0, 23, false)
        && cron_field_matches(fields[2], day_of_month, 1, 31, false)
        && cron_field_matches(fields[3], month, 1, 12, false)
        && cron_field_matches(fields[4], day_of_week, 0, 6, true)
}

pub async fn run_scheduler_loop(app: AppHandle) {
    let mut ticker = time::interval(Duration::from_secs(SCHEDULER_TICK_SECONDS));
    let mut fired_minute_by_id: HashMap<String, String> = HashMap::new();

    loop {
        ticker.tick().await;

        let now_local = Local::now();
        let now_utc = Utc::now().to_rfc3339();
        let minute_key = now_local.format("%Y-%m-%d %H:%M").to_string();

        let jobs = match list_jobs(&app) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[cron.scheduler] Failed to list jobs: {}", e);
                continue;
            }
        };

        let mut existing_ids = HashSet::new();

        for job in jobs {
            existing_ids.insert(job.id.clone());

            if !cron_matches_local_now(&job.cron, &now_local) {
                continue;
            }

            if fired_minute_by_id
                .get(&job.id)
                .map(|key| key == &minute_key)
                .unwrap_or(false)
            {
                continue;
            }

            let payload = ScheduledTaskTriggerEvent {
                id: job.id.clone(),
                cron: job.cron.clone(),
                prompt: job.prompt.clone(),
                recurring: job.recurring,
                durable: job.durable,
                created_at: job.created_at.clone(),
                triggered_at: now_utc.clone(),
            };

            match app.emit("scheduled-task-trigger", &payload) {
                Ok(_) => {
                    fired_minute_by_id.insert(job.id.clone(), minute_key.clone());
                    if !job.recurring {
                        if let Err(e) = remove_job(&app, &job.id) {
                            eprintln!(
                                "[cron.scheduler] Failed to remove one-shot job {}: {}",
                                job.id, e
                            );
                        }
                        fired_minute_by_id.remove(&job.id);
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[cron.scheduler] Failed to emit scheduled-task-trigger for {}: {}",
                        job.id, e
                    );
                }
            }
        }

        fired_minute_by_id.retain(|id, _| existing_ids.contains(id));
    }
}

#[tauri::command]
pub fn list_scheduled_tasks(app: AppHandle) -> Result<Vec<CronJob>, String> {
    list_jobs(&app)
}

#[tauri::command]
pub fn create_scheduled_task(
    app: AppHandle,
    cron: String,
    prompt: String,
    recurring: Option<bool>,
    durable: Option<bool>,
) -> Result<CronJob, String> {
    let cron_value = cron.trim();
    if cron_value.is_empty() {
        return Err("cron is required".to_string());
    }
    validate_cron_expression(cron_value)?;

    let prompt_value = prompt.trim();
    if prompt_value.is_empty() {
        return Err("prompt is required".to_string());
    }

    let raw_uuid = Uuid::new_v4().simple().to_string();
    let id = format!("cron-{}", &raw_uuid[..12]);

    let job = CronJob {
        id,
        cron: cron_value.to_string(),
        prompt: prompt_value.to_string(),
        recurring: recurring.unwrap_or(true),
        durable: durable.unwrap_or(false),
        created_at: Utc::now().to_rfc3339(),
    };

    add_job(&app, job)
}

#[tauri::command]
pub fn delete_scheduled_task(app: AppHandle, id: String) -> Result<bool, String> {
    let task_id = id.trim();
    if task_id.is_empty() {
        return Err("id is required".to_string());
    }

    remove_job(&app, task_id)
}
