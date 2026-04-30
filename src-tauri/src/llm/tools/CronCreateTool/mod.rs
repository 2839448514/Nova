use crate::llm::tools::{app_tool, AppExecuteFuture, ToolRegistration};
use crate::llm::types::Tool;
use serde_json::{json, Value};
use tauri::AppHandle;

// 把 async 的调度创建逻辑包装成统一 future。
// `input` 里会携带 cron、prompt、recurring、durable 这些创建任务所需参数。
fn execute_with_app_boxed(
    app: AppHandle,
    _conversation_id: Option<String>,
    input: Value,
) -> AppExecuteFuture {
    Box::pin(async move { execute_with_app(&app, input).await })
}

// 返回 CronCreate 的注册信息。
// 这里声明 `read_only=false`，因为它会创建新的计划任务记录。
pub(crate) fn registration() -> ToolRegistration {
    app_tool(tool, execute, execute_with_app_boxed, false, None)
}

// 返回暴露给模型的 CronCreate 元数据。
// 模型通过 schema 知道要提供 cron 表达式和 prompt 内容。
pub fn tool() -> Tool {
    Tool {
        name: "CronCreate".into(),
        description: "Schedule a recurring or one-shot prompt task.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "cron": { "type": "string", "description": "5-field cron expression: M H DoM Mon DoW" },
                "prompt": { "type": "string", "description": "Prompt payload to run on schedule" },
                "recurring": { "description": "true (default) for recurring schedule, false for one-shot" },
                "durable": { "description": "true to persist in app_data_dir across restarts" }
            },
            "required": ["cron", "prompt"]
        }),
    }
}

// 同步入口只给出固定错误，避免在没有 AppHandle 的上下文里误调用。
pub fn execute(_input: Value) -> String {
    json!({
        "ok": false,
        "message": "CronCreate requires AppHandle-aware execution and should be routed via execute_tool_with_app."
    })
    .to_string()
}

// 读取 input[key] 并把布尔语义统一成 true/false。
// `default_value` 表示字段缺失或无法解析时应该落到哪个默认值。
fn parse_semantic_bool(input: &Value, key: &str, default_value: bool) -> bool {
    let Some(value) = input.get(key) else {
        return default_value;
    };

    if let Some(v) = value.as_bool() {
        return v;
    }

    if let Some(v) = value.as_i64() {
        return v != 0;
    }

    if let Some(v) = value.as_u64() {
        return v != 0;
    }

    if let Some(v) = value.as_str() {
        let lower = v.trim().to_ascii_lowercase();
        return matches!(lower.as_str(), "1" | "true" | "yes" | "on");
    }

    default_value
}

// 根据 cron 字段位置返回合法数字范围。
// `index` 对应五段 cron 中的第几段：分钟、小时、日、月、周。
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

// 判断一个纯数字片段是否落在当前字段允许的范围内。
fn parse_number_in_range(raw: &str, min: u32, max: u32) -> bool {
    raw.parse::<u32>()
        .ok()
        .map(|v| v >= min && v <= max)
        .unwrap_or(false)
}

// 校验 cron 单个片段是否合法。
// `segment` 可能是 `*`、`1-5`、`*/10`、`1,2,3` 这些形式，这里逐种拆开检查。
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

// 校验一个完整 cron 字段。
// 字段里允许用逗号组合多个片段，所以这里会把字段拆成多个 segment 逐个复用上面的校验。
fn validate_cron_field(field: &str, min: u32, max: u32) -> bool {
    field
        .split(',')
        .all(|segment| validate_cron_segment(segment.trim(), min, max))
}

// 校验 5 段 cron 表达式整体是否合法。
// `expr` 是模型传来的原始字符串，出错时直接返回给模型可读的错误信息。
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

// 创建计划任务并返回保存结果。
// `cron` 是触发表达式，`prompt` 是到时要执行的提示词，`recurring/durable` 控制重复和持久化行为。
pub async fn execute_with_app(app: &AppHandle, input: Value) -> String {
    let cron = match input.get("cron").and_then(|v| v.as_str()).map(str::trim) {
        Some(v) if !v.is_empty() => v,
        _ => return json!({ "ok": false, "error": "CronCreate requires non-empty 'cron'" }).to_string(),
    };

    if let Err(e) = validate_cron_expression(cron) {
        return json!({ "ok": false, "error": e }).to_string();
    }

    let prompt = match input.get("prompt").and_then(|v| v.as_str()).map(str::trim) {
        Some(v) if !v.is_empty() => v,
        _ => return json!({ "ok": false, "error": "CronCreate requires non-empty 'prompt'" }).to_string(),
    };

    // recurring: true 表示反复执行；false 表示一次性任务。
    let recurring = parse_semantic_bool(&input, "recurring", true);
    // durable: true 表示写入持久化存储，应用重启后仍然保留。
    let durable = parse_semantic_bool(&input, "durable", false);

    match crate::command::cron::create_scheduled_task(
        app.clone(),
        cron.to_string(),
        prompt.to_string(),
        Some(recurring),
        Some(durable),
    )
    .await
    {
        Ok(saved) => json!({
            "ok": true,
            "id": saved.id,
            "cron": saved.cron,
            "humanSchedule": saved.cron,
            "prompt": saved.prompt,
            "conversationId": saved.conversation_id,
            "recurring": saved.recurring,
            "durable": saved.durable,
            "createdAt": saved.created_at
        })
        .to_string(),
        Err(e) => json!({ "ok": false, "error": e }).to_string(),
    }
}
