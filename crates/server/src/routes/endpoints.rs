//! `/api/endpoints`: saved model servers, with a live check of the base URL and API key.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use sqlx::types::Json as Db;
use stellarator_core::openai::{check_key, list_models};
use stellarator_core::schemas::{EndpointIn, EndpointOut, EndpointPatch, EndpointTestIn, EndpointTestOut};
use stellarator_core::settings::key_hint;
use stellarator_core::time::{now_db, utc, utc_opt};
use stellarator_core::{ApiError, ApiJson, Ctx};

use super::P;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/endpoints", get(list).post(create))
        .route("/api/endpoints/test", post(test))
        .route("/api/endpoints/{id}", get(read).patch(update).delete(remove))
        .route("/api/endpoints/{id}/check", post(check))
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    name: String,
    base_url: String,
    api_key: Option<String>,
    location: String,
    default_concurrency: i64,
    headers: Db<BTreeMap<String, String>>,
    models: Db<Vec<String>>,
    last_checked_at: Option<NaiveDateTime>,
    last_ok: Option<bool>,
    last_error: Option<String>,
    created_at: NaiveDateTime,
    run_count: i64,
}

const SELECT: &str = "SELECT e.*, (SELECT COUNT(*) FROM runs r WHERE r.endpoint_id = e.id) AS run_count FROM endpoints e";

impl Row {
    fn out(self) -> EndpointOut {
        EndpointOut {
            id: self.id,
            name: self.name,
            base_url: self.base_url,
            has_api_key: self.api_key.as_deref().is_some_and(|k| !k.is_empty()),
            api_key_hint: key_hint(self.api_key.as_deref()),
            location: self.location,
            default_concurrency: self.default_concurrency,
            headers: self.headers.0,
            models: self.models.0,
            last_checked_at: utc_opt(self.last_checked_at),
            last_ok: self.last_ok,
            last_error: self.last_error,
            created_at: utc(self.created_at),
            run_count: self.run_count,
        }
    }
}

async fn fetch(ctx: &Ctx, id: i64) -> Result<Row, ApiError> {
    sqlx::query_as::<_, Row>(&format!("{SELECT} WHERE e.id = ?"))
        .bind(id)
        .fetch_optional(&ctx.db)
        .await?
        .ok_or_else(|| ApiError::not_found("Endpoint not found"))
}

/// What a check learned: the model list when it could be read, and whether the key is accepted.
struct Checked {
    models: Option<Vec<String>>,
    latency_ms: Option<i64>,
    error: Option<String>,
}

/// List the models, then probe the key. A failed model list is the error; a rejected key is the
/// error (and the models are still returned).
async fn probe(ctx: &Ctx, base_url: &str, api_key: Option<&str>, headers: &BTreeMap<String, String>) -> Checked {
    match list_models(&ctx.http, base_url, api_key, headers).await {
        Err(e) => Checked { models: None, latency_ms: None, error: Some(e.sentence()) },
        Ok((models, latency)) => Checked {
            models: Some(models),
            latency_ms: Some(latency),
            error: check_key(&ctx.http, base_url, api_key, headers).await,
        },
    }
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<EndpointOut>>, ApiError> {
    let rows = sqlx::query_as::<_, Row>(&format!("{SELECT} ORDER BY e.created_at DESC, e.id DESC"))
        .fetch_all(&state.ctx.db)
        .await?;
    Ok(Json(rows.into_iter().map(Row::out).collect()))
}

async fn create(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<EndpointIn>,
) -> Result<(StatusCode, Json<EndpointOut>), ApiError> {
    let ctx = &state.ctx;
    let api_key = body.api_key.filter(|k| !k.is_empty());
    let checked = probe(ctx, &body.base_url, api_key.as_deref(), &body.headers).await;
    let now = now_db();
    let id = sqlx::query(
        "INSERT INTO endpoints (name, base_url, api_key, location, default_concurrency, headers, models, \
         last_checked_at, last_ok, last_error, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&body.name)
    .bind(&body.base_url)
    .bind(&api_key)
    .bind(body.location.as_str())
    .bind(body.default_concurrency)
    .bind(Db(&body.headers))
    .bind(Db(checked.models.unwrap_or_default()))
    .bind(&now)
    .bind(checked.error.is_none())
    .bind(&checked.error)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await?
    .last_insert_rowid();
    Ok((StatusCode::CREATED, Json(fetch(ctx, id).await?.out())))
}

async fn test(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<EndpointTestIn>,
) -> Result<Json<EndpointTestOut>, ApiError> {
    let ctx = &state.ctx;
    let mut api_key = body.api_key.filter(|k| !k.is_empty());
    if api_key.is_none() {
        if let Some(id) = body.endpoint_id {
            api_key = sqlx::query_scalar::<_, Option<String>>("SELECT api_key FROM endpoints WHERE id = ?")
                .bind(id)
                .fetch_optional(&ctx.db)
                .await?
                .flatten();
        }
    }
    let c = probe(ctx, &body.base_url, api_key.as_deref(), &body.headers).await;
    Ok(Json(EndpointTestOut {
        ok: c.error.is_none(),
        models: c.models.unwrap_or_default(),
        latency_ms: c.latency_ms,
        error: c.error,
    }))
}

async fn read(State(state): State<AppState>, P(id): P<i64>) -> Result<Json<EndpointOut>, ApiError> {
    Ok(Json(fetch(&state.ctx, id).await?.out()))
}

/// Re-check a saved endpoint and store the outcome.
async fn recheck(ctx: &Ctx, row: &Row, api_key: Option<&str>, headers: &BTreeMap<String, String>, base_url: &str) -> Result<Checked, ApiError> {
    let checked = probe(ctx, base_url, api_key, headers).await;
    let models = checked.models.clone().unwrap_or_else(|| row.models.0.clone());
    let res = sqlx::query(
        "UPDATE endpoints SET models = ?, last_checked_at = ?, last_ok = ?, last_error = ?, updated_at = ? WHERE id = ?",
    )
    .bind(Db(models))
    .bind(now_db())
    .bind(checked.error.is_none())
    .bind(&checked.error)
    .bind(now_db())
    .bind(row.id)
    .execute(&ctx.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::not_found("Endpoint not found"));
    }
    Ok(checked)
}

async fn update(
    State(state): State<AppState>,
    P(id): P<i64>,
    ApiJson(body): ApiJson<EndpointPatch>,
) -> Result<Json<EndpointOut>, ApiError> {
    let ctx = &state.ctx;
    let row = fetch(ctx, id).await?;
    let name = body.name.unwrap_or_else(|| row.name.clone());
    let base_url = body.base_url.unwrap_or_else(|| row.base_url.clone());
    let location = body.location.map(|l| l.as_str().to_string()).unwrap_or_else(|| row.location.clone());
    let concurrency = body.default_concurrency.unwrap_or(row.default_concurrency);
    let headers = body.headers.unwrap_or_else(|| row.headers.0.clone());
    // None leaves the key alone; "" removes it.
    let api_key = match body.api_key {
        Some(k) => Some(k).filter(|k| !k.is_empty()),
        None => row.api_key.clone(),
    };
    let res = sqlx::query(
        "UPDATE endpoints SET name = ?, base_url = ?, api_key = ?, location = ?, default_concurrency = ?, \
         headers = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&name)
    .bind(&base_url)
    .bind(&api_key)
    .bind(&location)
    .bind(concurrency)
    .bind(Db(&headers))
    .bind(now_db())
    .bind(id)
    .execute(&ctx.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::not_found("Endpoint not found"));
    }
    recheck(ctx, &row, api_key.as_deref(), &headers, &base_url).await?;
    Ok(Json(fetch(ctx, id).await?.out()))
}

async fn check(State(state): State<AppState>, P(id): P<i64>) -> Result<Json<EndpointOut>, ApiError> {
    let ctx = &state.ctx;
    let row = fetch(ctx, id).await?;
    recheck(ctx, &row, row.api_key.as_deref(), &row.headers.0, &row.base_url).await?;
    Ok(Json(fetch(ctx, id).await?.out()))
}

async fn remove(State(state): State<AppState>, P(id): P<i64>) -> Result<StatusCode, ApiError> {
    let ctx = &state.ctx;
    fetch(ctx, id).await?;
    let active: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE endpoint_id = ? AND status IN ('running', 'pausing')")
            .bind(id)
            .fetch_one(&ctx.db)
            .await?;
    if active > 0 || state.runs.active_runs().iter().any(|r| r.endpoint_id == Some(id)) {
        return Err(ApiError::conflict("A run is using this endpoint. Pause it before deleting the endpoint."));
    }
    // Runs keep their endpoint_name snapshot; their endpoint_id becomes NULL (ON DELETE SET NULL).
    sqlx::query("DELETE FROM endpoints WHERE id = ?").bind(id).execute(&ctx.db).await?;
    Ok(StatusCode::NO_CONTENT)
}
