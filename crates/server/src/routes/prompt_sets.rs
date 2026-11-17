//! `/api/prompt-sets`: list, rename, browse prompts, delete.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::Value;
use sqlx::types::Json as Db;
use stellarator_core::schemas::{Page, PromptOut, PromptSetOut, PromptSetPatch, PromptSetStatus};
use stellarator_core::time::utc;
use stellarator_core::{ApiError, ApiJson, ApiQuery, Ctx, FieldErrors};

use super::{check_range, P};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/prompt-sets", get(list))
        .route("/api/prompt-sets/{id}", get(read).patch(rename).delete(remove))
        .route("/api/prompt-sets/{id}/prompts", get(prompts))
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    name: String,
    source_kind: String,
    source: Db<Value>,
    prompt_count: i64,
    created_at: NaiveDateTime,
    status: String,
    status_detail: Option<String>,
    rows_read: i64,
    rows_total: Option<i64>,
    run_count: i64,
}

const SELECT: &str = "SELECT s.id, s.name, s.source_kind, s.source, s.prompt_count, s.created_at, s.status, \
    s.status_detail, s.rows_read, s.rows_total, \
    (SELECT COUNT(*) FROM runs r WHERE r.prompt_set_id = s.id) AS run_count FROM prompt_sets s";

impl Row {
    fn out(self) -> PromptSetOut {
        PromptSetOut {
            id: self.id,
            name: self.name,
            source_kind: self.source_kind,
            source: self.source.0,
            prompt_count: self.prompt_count,
            created_at: utc(self.created_at),
            run_count: self.run_count,
            status: PromptSetStatus::parse(&self.status),
            status_detail: self.status_detail,
            rows_read: self.rows_read,
            rows_total: self.rows_total,
        }
    }
}

async fn fetch(ctx: &Ctx, id: i64) -> Result<Row, ApiError> {
    sqlx::query_as::<_, Row>(&format!("{SELECT} WHERE s.id = ?"))
        .bind(id)
        .fetch_optional(&ctx.db)
        .await?
        .ok_or_else(|| ApiError::not_found("Prompt set not found"))
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<PromptSetOut>>, ApiError> {
    let rows = sqlx::query_as::<_, Row>(&format!("{SELECT} ORDER BY s.created_at DESC, s.id DESC"))
        .fetch_all(&state.ctx.db)
        .await?;
    Ok(Json(rows.into_iter().map(Row::out).collect()))
}

async fn read(State(state): State<AppState>, P(id): P<i64>) -> Result<Json<PromptSetOut>, ApiError> {
    Ok(Json(fetch(&state.ctx, id).await?.out()))
}

async fn rename(
    State(state): State<AppState>,
    P(id): P<i64>,
    ApiJson(body): ApiJson<PromptSetPatch>,
) -> Result<Json<PromptSetOut>, ApiError> {
    let res = sqlx::query("UPDATE prompt_sets SET name = ? WHERE id = ?")
        .bind(&body.name)
        .bind(id)
        .execute(&state.ctx.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::not_found("Prompt set not found"));
    }
    Ok(Json(fetch(&state.ctx, id).await?.out()))
}

#[derive(Deserialize)]
struct PromptsQuery {
    #[serde(default)]
    offset: i64,
    #[serde(default = "default_limit")]
    limit: i64,
    q: Option<String>,
}

fn default_limit() -> i64 {
    50
}

#[derive(sqlx::FromRow)]
struct PromptRow {
    id: i64,
    idx: i64,
    messages: Db<Value>,
    meta: Db<Value>,
}

/// Escape LIKE wildcards so the search text is matched literally.
fn like_pattern(q: &str) -> String {
    let mut out = String::from("%");
    for c in q.to_lowercase().chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

async fn prompts(
    State(state): State<AppState>,
    P(id): P<i64>,
    ApiQuery(q): ApiQuery<PromptsQuery>,
) -> Result<Json<Page<PromptOut>>, ApiError> {
    let mut errors = FieldErrors::new();
    check_range(&mut errors, "offset", q.offset, 0, None);
    check_range(&mut errors, "limit", q.limit, 1, Some(500));
    errors.finish()?;
    let ctx = &state.ctx;
    fetch(ctx, id).await?;
    let pattern = q.q.as_deref().filter(|s| !s.is_empty()).map(like_pattern);
    let (total, rows) = match &pattern {
        None => {
            let total = sqlx::query_scalar("SELECT COUNT(*) FROM prompts WHERE prompt_set_id = ?")
                .bind(id)
                .fetch_one(&ctx.db)
                .await?;
            let rows = sqlx::query_as::<_, PromptRow>(
                "SELECT id, idx, messages, meta FROM prompts WHERE prompt_set_id = ? ORDER BY idx LIMIT ? OFFSET ?",
            )
            .bind(id)
            .bind(q.limit)
            .bind(q.offset)
            .fetch_all(&ctx.db)
            .await?;
            (total, rows)
        }
        Some(p) => {
            let total = sqlx::query_scalar(
                "SELECT COUNT(*) FROM prompts WHERE prompt_set_id = ? AND lower(messages) LIKE ? ESCAPE '\\'",
            )
            .bind(id)
            .bind(p)
            .fetch_one(&ctx.db)
            .await?;
            let rows = sqlx::query_as::<_, PromptRow>(
                "SELECT id, idx, messages, meta FROM prompts WHERE prompt_set_id = ? \
                 AND lower(messages) LIKE ? ESCAPE '\\' ORDER BY idx LIMIT ? OFFSET ?",
            )
            .bind(id)
            .bind(p)
            .bind(q.limit)
            .bind(q.offset)
            .fetch_all(&ctx.db)
            .await?;
            (total, rows)
        }
    };
    let items = rows
        .into_iter()
        .map(|r| PromptOut {
            id: r.id,
            idx: r.idx,
            messages: match r.messages.0 {
                Value::Array(a) => a,
                _ => Vec::new(),
            },
            meta: r.meta.0,
        })
        .collect();
    Ok(Json(Page { items, total, offset: q.offset, limit: q.limit }))
}

async fn remove(State(state): State<AppState>, P(id): P<i64>) -> Result<StatusCode, ApiError> {
    let ctx = &state.ctx;
    fetch(ctx, id).await?;
    if state.runs.active_runs().iter().any(|r| r.prompt_set_id == id) {
        return Err(ApiError::conflict("A run on this prompt set is still going. Pause it first."));
    }
    // Stop the import first so it cannot write rows into a set that is going away.
    if state.imports.is_importing(id) {
        state.imports.cancel(id).await;
    }
    // One statement, one transaction: the foreign keys delete the prompts, runs, samples,
    // metrics, preference sets and pairs that hang off this set.
    sqlx::query("DELETE FROM prompt_sets WHERE id = ?").bind(id).execute(&ctx.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

