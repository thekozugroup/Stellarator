//! Runs: create, watch, steer (pause, resume, concurrency, retry failed) and browse samples.

use std::sync::Arc;

use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::Value;
use sqlx::types::Json as SqlJson;
use sqlx::Row;
use stellarator_core::schemas::{
    MetricPoint, Page, RunCreate, RunLive, RunOut, RunParams, RunPatch, RunStatus, SampleOut, SampleStatus,
    SampleSummary,
};
use stellarator_core::{time, ApiError, ApiJson, ApiQuery};

use crate::engine::{preview, Engine, PREVIEW_CHARS};
use crate::generation::{last_user_text, prepare_messages};

pub const MAX_METRIC_POINTS: usize = 240;

pub fn router(engine: Arc<Engine>) -> Router {
    Router::new()
        .route("/api/runs", get(list_runs).post(create_run))
        .route("/api/runs/{run_id}", get(get_run).patch(update_run).delete(delete_run))
        .route("/api/runs/{run_id}/pause", post(pause_run))
        .route("/api/runs/{run_id}/resume", post(resume_run))
        .route("/api/runs/{run_id}/retry-failed", post(retry_failed))
        .route("/api/runs/{run_id}/live", get(run_live))
        .route("/api/runs/{run_id}/samples", get(list_samples))
        .route("/api/samples/{sample_id}", get(get_sample))
        .with_state(engine)
}

/// An integer path parameter; anything else is a 422 like the Python API's.
pub struct Id(pub i64);

impl<S: Send + Sync> FromRequestParts<S> for Id {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Path::<i64>::from_request_parts(parts, state).await {
            Ok(Path(id)) => Ok(Id(id)),
            Err(_) => Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Input should be a valid integer, unable to parse string as an integer",
            )),
        }
    }
}

fn run_not_found() -> ApiError {
    ApiError::not_found("Run not found")
}

// ------------------------------------------------------------------------------------ run rows

const RUN_SELECT: &str = "SELECT r.id, r.name, r.endpoint_id, r.endpoint_name, r.model, r.prompt_set_id, r.params, \
    r.concurrency, r.status, r.status_reason, r.total, r.done, r.failed, r.truncated, r.prompt_tokens, \
    r.completion_tokens, r.reasoning_tokens, r.active_seconds, r.created_at, r.started_at, r.finished_at, \
    ps.name AS prompt_set_name FROM runs r LEFT JOIN prompt_sets ps ON ps.id = r.prompt_set_id";

#[derive(sqlx::FromRow)]
struct RunRow {
    id: i64,
    name: String,
    endpoint_id: Option<i64>,
    endpoint_name: String,
    model: String,
    prompt_set_id: i64,
    params: SqlJson<Value>,
    concurrency: i64,
    status: String,
    status_reason: Option<String>,
    total: i64,
    done: i64,
    failed: i64,
    truncated: i64,
    prompt_tokens: i64,
    completion_tokens: i64,
    reasoning_tokens: i64,
    active_seconds: f64,
    created_at: NaiveDateTime,
    started_at: Option<NaiveDateTime>,
    finished_at: Option<NaiveDateTime>,
    prompt_set_name: Option<String>,
}

fn params_of(v: Value) -> RunParams {
    serde_json::from_value(v).unwrap_or_default()
}

fn run_out(engine: &Engine, r: RunRow) -> RunOut {
    let in_flight = engine.in_flight(r.id);
    let active = r.active_seconds + engine.unsaved_active_seconds(r.id);
    RunOut {
        id: r.id,
        name: r.name,
        endpoint_id: r.endpoint_id,
        endpoint_name: r.endpoint_name,
        model: r.model,
        prompt_set_id: r.prompt_set_id,
        prompt_set_name: r.prompt_set_name.unwrap_or_default(),
        params: params_of(r.params.0),
        concurrency: r.concurrency,
        status: RunStatus::parse(&r.status),
        status_reason: r.status_reason,
        total: r.total,
        done: r.done,
        failed: r.failed,
        truncated: r.truncated,
        pending: (r.total - r.done - r.failed - in_flight).max(0),
        prompt_tokens: r.prompt_tokens,
        completion_tokens: r.completion_tokens,
        reasoning_tokens: r.reasoning_tokens,
        active_seconds: (active * 1000.0).round() / 1000.0,
        created_at: time::utc(r.created_at),
        started_at: time::utc_opt(r.started_at),
        finished_at: time::utc_opt(r.finished_at),
    }
}

async fn load_run(engine: &Engine, run_id: i64) -> Result<RunOut, ApiError> {
    let row: Option<RunRow> = sqlx::query_as(&format!("{RUN_SELECT} WHERE r.id = ?"))
        .bind(run_id)
        .fetch_optional(&engine.ctx.db)
        .await?;
    row.map(|r| run_out(engine, r)).ok_or_else(run_not_found)
}

async fn require_run(engine: &Engine, run_id: i64) -> Result<(), ApiError> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT id FROM runs WHERE id = ?").bind(run_id).fetch_optional(&engine.ctx.db).await?;
    found.map(|_| ()).ok_or_else(run_not_found)
}

// ------------------------------------------------------------------------------------ runs

async fn list_runs(State(engine): State<Arc<Engine>>) -> Result<Json<Vec<RunOut>>, ApiError> {
    let rows: Vec<RunRow> = sqlx::query_as(&format!("{RUN_SELECT} ORDER BY r.created_at DESC, r.id DESC"))
        .fetch_all(&engine.ctx.db)
        .await?;
    Ok(Json(rows.into_iter().map(|r| run_out(&engine, r)).collect()))
}

async fn create_run(
    State(engine): State<Arc<Engine>>,
    ApiJson(body): ApiJson<RunCreate>,
) -> Result<(StatusCode, Json<RunOut>), ApiError> {
    let db = &engine.ctx.db;
    let endpoint: Option<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM endpoints WHERE id = ?").bind(body.endpoint_id).fetch_optional(db).await?;
    let Some((endpoint_id, endpoint_name)) = endpoint else {
        return Err(ApiError::field("endpoint_id", "This endpoint does not exist. Pick another endpoint."));
    };
    let set: Option<(i64, String, String)> = sqlx::query_as("SELECT id, name, status FROM prompt_sets WHERE id = ?")
        .bind(body.prompt_set_id)
        .fetch_optional(db)
        .await?;
    let Some((set_id, set_name, set_status)) = set else {
        return Err(ApiError::field("prompt_set_id", "This prompt set does not exist. Pick another prompt set."));
    };
    match set_status.as_str() {
        "importing" => return Err(ApiError::field("prompt_set_id", "This prompt set is still importing.")),
        "failed" => return Err(ApiError::field("prompt_set_id", "This prompt set failed to import.")),
        _ => {}
    }
    let has_prompts: Option<i64> = sqlx::query_scalar("SELECT id FROM prompts WHERE prompt_set_id = ? LIMIT 1")
        .bind(set_id)
        .fetch_optional(db)
        .await?;
    if has_prompts.is_none() {
        return Err(ApiError::field("prompt_set_id", "This prompt set has no prompts. Import prompts into it first."));
    }

    let per_prompt = body.params.samples_per_prompt;
    let name = match body.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => format!("{} on {}", body.model, set_name),
    };
    let name: String = name.chars().take(300).collect();
    let now = time::now_db();
    let params = serde_json::to_value(&body.params).map_err(anyhow::Error::from)?;

    let mut tx = db.begin().await?;
    let run_id = sqlx::query(
        "INSERT INTO runs (name, endpoint_id, endpoint_name, model, prompt_set_id, params, concurrency, status, \
         status_reason, total, done, failed, truncated, prompt_tokens, completion_tokens, reasoning_tokens, \
         active_seconds, created_at, started_at, finished_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, 'running', NULL, 0, 0, 0, 0, 0, 0, 0, 0.0, ?, ?, NULL)",
    )
    .bind(&name)
    .bind(endpoint_id)
    .bind(&endpoint_name)
    .bind(&body.model)
    .bind(set_id)
    .bind(SqlJson(&params))
    .bind(body.concurrency)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    // One statement inserts every sample in (prompt_idx, prompt_id, sample_idx) order, so sample ids
    // follow prompt order. The engine relies on this to take pending samples in prompt order
    // straight from the (run_id, status) index.
    let total = sqlx::query(
        "WITH RECURSIVE k(n) AS (SELECT 0 UNION ALL SELECT n + 1 FROM k WHERE n + 1 < ?) \
         INSERT INTO samples (run_id, prompt_id, prompt_idx, sample_idx, status, attempts) \
         SELECT ?, p.id, p.idx, k.n, 'pending', 0 FROM prompts p CROSS JOIN k \
         WHERE p.prompt_set_id = ? ORDER BY p.idx, p.id, k.n",
    )
    .bind(per_prompt)
    .bind(run_id)
    .bind(set_id)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;
    sqlx::query("UPDATE runs SET total = ? WHERE id = ?").bind(total).bind(run_id).execute(&mut *tx).await?;
    tx.commit().await?;

    // The endpoint vanished between the check and the start: 409.
    engine.start(run_id).await?;
    Ok((StatusCode::CREATED, Json(load_run(&engine, run_id).await?)))
}

async fn get_run(State(engine): State<Arc<Engine>>, Id(run_id): Id) -> Result<Json<RunOut>, ApiError> {
    Ok(Json(load_run(&engine, run_id).await?))
}

async fn update_run(
    State(engine): State<Arc<Engine>>,
    Id(run_id): Id,
    ApiJson(body): ApiJson<RunPatch>,
) -> Result<Json<RunOut>, ApiError> {
    require_run(&engine, run_id).await?;
    let db = &engine.ctx.db;
    if let Some(name) = body.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        sqlx::query("UPDATE runs SET name = ? WHERE id = ?").bind(name).bind(run_id).execute(db).await?;
    }
    if let Some(c) = body.concurrency {
        sqlx::query("UPDATE runs SET concurrency = ? WHERE id = ?").bind(c).bind(run_id).execute(db).await?;
        engine.set_concurrency(run_id, c).await;
    }
    Ok(Json(load_run(&engine, run_id).await?))
}

async fn delete_run(State(engine): State<Arc<Engine>>, Id(run_id): Id) -> Result<StatusCode, ApiError> {
    require_run(&engine, run_id).await?;
    engine.stop(run_id).await;
    // Samples and metrics go with it (ON DELETE CASCADE).
    sqlx::query("DELETE FROM runs WHERE id = ?").bind(run_id).execute(&engine.ctx.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn pause_run(State(engine): State<Arc<Engine>>, Id(run_id): Id) -> Result<Json<RunOut>, ApiError> {
    require_run(&engine, run_id).await?;
    engine.pause(run_id).await?;
    Ok(Json(load_run(&engine, run_id).await?))
}

async fn resume_run(State(engine): State<Arc<Engine>>, Id(run_id): Id) -> Result<Json<RunOut>, ApiError> {
    require_run(&engine, run_id).await?;
    engine.resume(run_id).await?;
    Ok(Json(load_run(&engine, run_id).await?))
}

async fn retry_failed(State(engine): State<Arc<Engine>>, Id(run_id): Id) -> Result<Json<RunOut>, ApiError> {
    require_run(&engine, run_id).await?;
    engine.retry_failed(run_id).await?;
    Ok(Json(load_run(&engine, run_id).await?))
}

#[derive(sqlx::FromRow)]
struct MetricRow {
    t: NaiveDateTime,
    tokens_per_s: f64,
    samples_per_min: f64,
    active: i64,
    concurrency: i64,
    done: i64,
    failed: i64,
}

/// The run's metric points, at most 240: every `stride`-th point, and always the latest one.
async fn metric_points(engine: &Engine, run_id: i64) -> Result<Vec<MetricPoint>, ApiError> {
    let db = &engine.ctx.db;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_metrics WHERE run_id = ?").bind(run_id).fetch_one(db).await?;
    if n == 0 {
        return Ok(Vec::new());
    }
    let max = MAX_METRIC_POINTS as i64;
    let stride = if n > max { (n + max - 1) / max } else { 1 };
    let mut rows: Vec<MetricRow> = sqlx::query_as(
        "SELECT t, tokens_per_s, samples_per_min, active, concurrency, done, failed FROM ( \
           SELECT ROW_NUMBER() OVER (ORDER BY t, id) - 1 AS rn, t, tokens_per_s, samples_per_min, active, \
                  concurrency, done, failed FROM run_metrics WHERE run_id = ? \
         ) WHERE rn % ? = 0 OR rn = ? ORDER BY rn",
    )
    .bind(run_id)
    .bind(stride)
    .bind(n - 1)
    .fetch_all(db)
    .await?;
    // Python: rows[::stride], then the last row replaces the final pick when there are already 240
    // points, or is appended.
    if rows.len() >= 2 && (n - 1) % stride != 0 {
        let picks = rows.len() - 1;
        if picks >= MAX_METRIC_POINTS {
            let last = rows.len() - 2;
            rows.remove(last);
        }
    }
    Ok(rows
        .into_iter()
        .map(|r| MetricPoint {
            t: time::utc(r.t),
            tokens_per_s: r.tokens_per_s,
            samples_per_min: r.samples_per_min,
            active: r.active,
            concurrency: r.concurrency,
            done: r.done,
            failed: r.failed,
        })
        .collect())
}

async fn run_live(State(engine): State<Arc<Engine>>, Id(run_id): Id) -> Result<Json<RunLive>, ApiError> {
    let run = load_run(&engine, run_id).await?;
    let metrics = metric_points(&engine, run_id).await?;
    let live = engine.live(run_id);
    Ok(Json(match live {
        None => RunLive {
            run,
            in_flight: 0,
            sequences: Vec::new(),
            tokens_per_s: 0.0,
            samples_per_min: 0.0,
            eta_seconds: None,
            metrics,
        },
        Some(l) => RunLive {
            run,
            in_flight: l.in_flight,
            sequences: l.sequences,
            tokens_per_s: l.tokens_per_s,
            samples_per_min: l.samples_per_min,
            eta_seconds: l.eta_seconds,
            metrics,
        },
    }))
}

// ------------------------------------------------------------------------------------ samples

#[derive(Deserialize)]
pub struct SamplesQuery {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    q: Option<String>,
}

fn like_pattern(q: &str) -> String {
    let escaped = q.to_lowercase().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

async fn list_samples(
    State(engine): State<Arc<Engine>>,
    Id(run_id): Id,
    ApiQuery(query): ApiQuery<SamplesQuery>,
) -> Result<Json<Page<SampleSummary>>, ApiError> {
    let status = match query.status.as_deref() {
        None => None,
        Some(s @ ("pending" | "running" | "done" | "failed" | "truncated")) => Some(s.to_string()),
        Some(_) => {
            return Err(ApiError::field(
                "status",
                "Input should be 'pending', 'running', 'done', 'failed' or 'truncated'",
            ))
        }
    };
    let offset = query.offset.unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::field("offset", "Input should be greater than or equal to 0"));
    }
    let limit = query.limit.unwrap_or(50);
    if limit < 1 {
        return Err(ApiError::field("limit", "Input should be greater than or equal to 1"));
    }
    if limit > 500 {
        return Err(ApiError::field("limit", "Input should be less than or equal to 500"));
    }
    require_run(&engine, run_id).await?;

    let mut where_sql = String::from("s.run_id = ?");
    match status.as_deref() {
        Some("truncated") => where_sql.push_str(" AND s.status = 'done' AND s.finish_reason = 'length'"),
        Some(_) => where_sql.push_str(" AND s.status = ?"),
        None => {}
    }
    let pattern = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(like_pattern);
    if pattern.is_some() {
        where_sql.push_str(
            " AND (EXISTS (SELECT 1 FROM json_each(p.messages) AS m \
               WHERE lower(json_extract(m.value, '$.content')) LIKE ? ESCAPE '\\') \
             OR lower(s.response) LIKE ? ESCAPE '\\')",
        );
    }
    let bind_status = status.as_deref().filter(|s| *s != "truncated");

    let count_sql = if pattern.is_some() {
        format!("SELECT COUNT(*) FROM samples s JOIN prompts p ON p.id = s.prompt_id WHERE {where_sql}")
    } else {
        format!("SELECT COUNT(*) FROM samples s WHERE {where_sql}")
    };
    let mut cq = sqlx::query_scalar::<_, i64>(&count_sql).bind(run_id);
    if let Some(s) = bind_status {
        cq = cq.bind(s);
    }
    if let Some(p) = &pattern {
        cq = cq.bind(p).bind(p);
    }
    let total = cq.fetch_one(&engine.ctx.db).await?;

    let list_sql = format!(
        "SELECT s.id, s.prompt_idx, s.sample_idx, s.status, substr(s.response, 1, 400) AS response, \
         (s.reasoning IS NOT NULL AND length(s.reasoning) > 0) AS has_reasoning, s.finish_reason, \
         s.completion_tokens, s.latency_ms, s.error, p.messages \
         FROM samples s JOIN prompts p ON p.id = s.prompt_id WHERE {where_sql} \
         ORDER BY s.prompt_idx, s.sample_idx, s.id LIMIT ? OFFSET ?"
    );
    let mut lq = sqlx::query(&list_sql).bind(run_id);
    if let Some(s) = bind_status {
        lq = lq.bind(s);
    }
    if let Some(p) = &pattern {
        lq = lq.bind(p).bind(p);
    }
    let rows = lq.bind(limit).bind(offset).fetch_all(&engine.ctx.db).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let messages: Option<String> = row.try_get("messages")?;
        let messages: Vec<Value> = match messages.as_deref().map(serde_json::from_str::<Value>) {
            Some(Ok(Value::Array(list))) => list,
            _ => Vec::new(),
        };
        let response: Option<String> = row.try_get("response")?;
        let status: String = row.try_get("status")?;
        items.push(SampleSummary {
            id: row.try_get("id")?,
            prompt_idx: row.try_get("prompt_idx")?,
            sample_idx: row.try_get("sample_idx")?,
            status: SampleStatus::parse(&status),
            prompt_preview: preview(&last_user_text(&messages), PREVIEW_CHARS),
            response_preview: response.map(|r| preview(&r, PREVIEW_CHARS)),
            has_reasoning: row.try_get::<Option<bool>, _>("has_reasoning")?.unwrap_or(false),
            finish_reason: row.try_get("finish_reason")?,
            completion_tokens: row.try_get("completion_tokens")?,
            latency_ms: row.try_get("latency_ms")?,
            error: row.try_get("error")?,
        });
    }
    Ok(Json(Page { items, total, offset, limit }))
}

#[derive(sqlx::FromRow)]
struct SampleRow {
    id: i64,
    run_id: i64,
    prompt_id: i64,
    prompt_idx: i64,
    sample_idx: i64,
    status: String,
    reasoning: Option<String>,
    response: Option<String>,
    finish_reason: Option<String>,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
    latency_ms: Option<i64>,
    ttft_ms: Option<i64>,
    attempts: i64,
    error: Option<String>,
    completed_at: Option<NaiveDateTime>,
    messages: Option<String>,
    params: Option<String>,
}

async fn get_sample(State(engine): State<Arc<Engine>>, Id(sample_id): Id) -> Result<Json<SampleOut>, ApiError> {
    let row: Option<SampleRow> = sqlx::query_as(
        "SELECT s.id, s.run_id, s.prompt_id, s.prompt_idx, s.sample_idx, s.status, s.reasoning, s.response, \
         s.finish_reason, s.prompt_tokens, s.completion_tokens, s.reasoning_tokens, s.latency_ms, s.ttft_ms, \
         s.attempts, s.error, s.completed_at, p.messages, r.params \
         FROM samples s LEFT JOIN prompts p ON p.id = s.prompt_id LEFT JOIN runs r ON r.id = s.run_id WHERE s.id = ?",
    )
    .bind(sample_id)
    .fetch_optional(&engine.ctx.db)
    .await?;
    let s = row.ok_or_else(|| ApiError::not_found("Sample not found"))?;
    let params = s.params.as_deref().and_then(|p| serde_json::from_str::<Value>(p).ok()).map(params_of).unwrap_or_default();
    let messages: Vec<Value> = match s.messages.as_deref().map(serde_json::from_str::<Value>) {
        Some(Ok(Value::Array(list))) => list,
        _ => Vec::new(),
    };
    Ok(Json(SampleOut {
        id: s.id,
        run_id: s.run_id,
        prompt_id: s.prompt_id,
        prompt_idx: s.prompt_idx,
        sample_idx: s.sample_idx,
        status: SampleStatus::parse(&s.status),
        reasoning: s.reasoning,
        response: s.response,
        finish_reason: s.finish_reason,
        prompt_tokens: s.prompt_tokens,
        completion_tokens: s.completion_tokens,
        reasoning_tokens: s.reasoning_tokens,
        latency_ms: s.latency_ms,
        ttft_ms: s.ttft_ms,
        attempts: s.attempts,
        error: s.error,
        completed_at: time::utc_opt(s.completed_at),
        messages: prepare_messages(&messages, &params),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_patterns_escape_wildcards() {
        assert_eq!(like_pattern("100%"), "%100\\%%");
        assert_eq!(like_pattern("a_B"), "%a\\_b%");
        assert_eq!(like_pattern("x\\y"), "%x\\\\y%");
    }
}
