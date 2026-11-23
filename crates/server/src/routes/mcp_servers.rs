//! `/api/mcp-servers`: saved MCP servers whose tools runs can call.
//!
//! - `GET /api/mcp-servers` -> `[McpServerOut]`
//! - `POST /api/mcp-servers` (`McpServerIn`) -> 201 `McpServerOut`
//! - `POST /api/mcp-servers/import` (`McpImportIn`) -> `McpImportOut`
//! - `GET /api/mcp-servers/{id}` -> `McpServerOut`
//! - `PATCH /api/mcp-servers/{id}` (`McpServerPatch`) -> `McpServerOut`
//! - `DELETE /api/mcp-servers/{id}` -> 204 (409 while a running run uses it)
//! - `POST /api/mcp-servers/{id}/check` -> `McpServerOut`: connect, initialize, list the tools,
//!   cache them; `status` is `ok`, or `error` with `error` set.
//!
//! Env and header values are secrets: they are stored as given and returned as hints only.
//! Stdio servers start commands, so in server mode (no sign-in, listening on the network) they are
//! refused with 403 unless the operator set STELLARATOR_ALLOW_STDIO_MCP=1.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use sqlx::types::Json as Db;
use stellarator_core::config::STDIO_MCP_BLOCKED;
use stellarator_core::error::Validate;
use stellarator_core::schemas::{
    McpImportIn, McpImportNote, McpImportOut, McpServerIn, McpServerOut, McpServerPatch, McpTransport,
};
use stellarator_core::time::now_db;
use stellarator_core::{ApiError, ApiJson, Ctx, FieldErrors};
use stellarator_mcp::store::{self, ServerRow, CHECK_TIMEOUT};

use super::P;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/mcp-servers", get(list).post(create))
        .route("/api/mcp-servers/import", post(import))
        .route("/api/mcp-servers/{id}", get(read).patch(update).delete(remove))
        .route("/api/mcp-servers/{id}/check", post(check))
}

fn not_found() -> ApiError {
    ApiError::not_found("MCP server not found")
}

fn stdio_blocked() -> ApiError {
    ApiError::new(StatusCode::FORBIDDEN, STDIO_MCP_BLOCKED)
}

async fn fetch(ctx: &Ctx, id: i64) -> Result<ServerRow, ApiError> {
    store::fetch(&ctx.db, id).await?.ok_or_else(not_found)
}

async fn out(ctx: &Ctx, id: i64) -> Result<McpServerOut, ApiError> {
    Ok(fetch(ctx, id).await?.out(ctx.config.allow_stdio_mcp))
}

/// The id of a saved server called `name` (ignoring case), other than `except`.
async fn name_taken(ctx: &Ctx, name: &str, except: Option<i64>) -> Result<bool, ApiError> {
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM mcp_servers WHERE name = ? COLLATE NOCASE AND id != ?")
        .bind(name)
        .bind(except.unwrap_or(-1))
        .fetch_optional(&ctx.db)
        .await?;
    Ok(id.is_some())
}

fn name_clash(name: &str) -> ApiError {
    ApiError::field("name", format!("A server named \"{name}\" already exists. Pick another name."))
}

async fn insert(ctx: &Ctx, s: &McpServerIn) -> Result<i64, ApiError> {
    let now = now_db();
    let id = sqlx::query(
        "INSERT INTO mcp_servers (name, transport, command, args, env, url, headers, enabled, status, created_at, \
         updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'unchecked', ?, ?)",
    )
    .bind(&s.name)
    .bind(s.transport.as_str())
    .bind(&s.command)
    .bind(Db(&s.args))
    .bind(Db(&s.env))
    .bind(&s.url)
    .bind(Db(&s.headers))
    .bind(s.enabled)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await?
    .last_insert_rowid();
    Ok(id)
}

/// Check a saved server and store what the check found.
async fn run_check(ctx: &Ctx, row: &ServerRow) -> Result<(), ApiError> {
    let outcome = store::check(&row.config(), CHECK_TIMEOUT).await;
    store::save_check(&ctx.db, row.id, &outcome).await?;
    Ok(())
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<McpServerOut>>, ApiError> {
    let allow = state.ctx.config.allow_stdio_mcp;
    Ok(Json(store::fetch_all(&state.ctx.db).await?.into_iter().map(|r| r.out(allow)).collect()))
}

async fn create(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<McpServerIn>,
) -> Result<(StatusCode, Json<McpServerOut>), ApiError> {
    let ctx = &state.ctx;
    if body.transport == McpTransport::Stdio && !ctx.config.allow_stdio_mcp {
        return Err(stdio_blocked());
    }
    if name_taken(ctx, &body.name, None).await? {
        return Err(name_clash(&body.name));
    }
    let id = insert(ctx, &body).await?;
    Ok((StatusCode::CREATED, Json(out(ctx, id).await?)))
}

async fn import(State(state): State<AppState>, ApiJson(body): ApiJson<McpImportIn>) -> Result<Json<McpImportOut>, ApiError> {
    let ctx = &state.ctx;
    let taken = store::names(&ctx.db).await?;
    let parsed = stellarator_mcp::import::parse(&body.text, &taken).map_err(|e| ApiError::field("text", e))?;
    let mut errors: Vec<McpImportNote> =
        parsed.errors.into_iter().map(|(entry, message)| McpImportNote { entry, message }).collect();
    let mut notes = Vec::new();
    let mut ids = Vec::new();
    for mut e in parsed.entries {
        let mut fe = FieldErrors::new();
        e.server.validate(&mut fe);
        if let Err(err) = fe.finish() {
            let detail = match err.fields.iter().next() {
                Some((field, msg)) => format!("{field}: {msg}"),
                None => err.detail,
            };
            errors.push(McpImportNote { entry: e.entry, message: detail });
            continue;
        }
        if e.server.transport == McpTransport::Stdio && !ctx.config.allow_stdio_mcp {
            errors.push(McpImportNote { entry: e.entry, message: STDIO_MCP_BLOCKED.into() });
            continue;
        }
        ids.push(insert(ctx, &e.server).await?);
        notes.extend(e.notes.into_iter().map(|message| McpImportNote { entry: e.entry.clone(), message }));
    }
    if body.check {
        let mut rows = Vec::new();
        for id in &ids {
            rows.push(fetch(ctx, *id).await?);
        }
        let checks = rows.iter().map(|r| run_check(ctx, r));
        for r in futures::future::join_all(checks).await {
            r?;
        }
    }
    let mut created = Vec::new();
    for id in ids {
        created.push(out(ctx, id).await?);
    }
    Ok(Json(McpImportOut { created, errors, notes }))
}

async fn read(State(state): State<AppState>, P(id): P<i64>) -> Result<Json<McpServerOut>, ApiError> {
    Ok(Json(out(&state.ctx, id).await?))
}

/// A secret map after a patch: a string replaces, `null` keeps the saved value, a saved name left
/// out goes.
fn merge_secrets(
    field: &str,
    saved: &BTreeMap<String, String>,
    patch: Option<BTreeMap<String, Option<String>>>,
) -> Result<BTreeMap<String, String>, ApiError> {
    let Some(patch) = patch else { return Ok(saved.clone()) };
    let mut out = BTreeMap::new();
    for (k, v) in patch {
        let value = match v {
            Some(v) => v,
            None => match saved.get(&k) {
                Some(old) => old.clone(),
                None => return Err(ApiError::field(field, format!("{k} has no saved value to keep. Enter a value."))),
            },
        };
        out.insert(k, value);
    }
    Ok(out)
}

async fn update(
    State(state): State<AppState>,
    P(id): P<i64>,
    ApiJson(body): ApiJson<McpServerPatch>,
) -> Result<Json<McpServerOut>, ApiError> {
    let ctx = &state.ctx;
    let row = fetch(ctx, id).await?;
    let name = body.name.clone().unwrap_or_else(|| row.name.clone());
    if name != row.name && name_taken(ctx, &name, Some(id)).await? {
        return Err(name_clash(&name));
    }
    let transport = body.transport.unwrap_or_else(|| row.transport());
    let command = match &body.command {
        Some(c) => Some(c.clone()).filter(|c| !c.is_empty()),
        None => row.command.clone(),
    };
    let args = body.args.clone().unwrap_or_else(|| row.args.0.clone());
    let url = match &body.url {
        Some(u) => Some(u.trim().to_string()).filter(|u| !u.is_empty()),
        None => row.url.clone(),
    };
    let env = merge_secrets("env", &row.env.0, body.env.clone())?;
    let headers = merge_secrets("headers", &row.headers.0, body.headers.clone())?;
    let enabled = body.enabled.unwrap_or(row.enabled);
    match transport {
        McpTransport::Stdio if command.is_none() => {
            return Err(ApiError::field("command", "Enter the command that starts the server, for example npx."))
        }
        McpTransport::Http if url.is_none() => {
            return Err(ApiError::field("url", "Enter the server's URL, for example https://example.com/mcp."))
        }
        _ => {}
    }
    let reach_changed = transport != row.transport()
        || command != row.command
        || args != row.args.0
        || env != row.env.0
        || url != row.url
        || headers != row.headers.0;
    if transport == McpTransport::Stdio && reach_changed && !ctx.config.allow_stdio_mcp {
        return Err(stdio_blocked());
    }
    let res = sqlx::query(
        "UPDATE mcp_servers SET name = ?, transport = ?, command = ?, args = ?, env = ?, url = ?, headers = ?, \
         enabled = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&name)
    .bind(transport.as_str())
    .bind(&command)
    .bind(Db(&args))
    .bind(Db(&env))
    .bind(&url)
    .bind(Db(&headers))
    .bind(enabled)
    .bind(now_db())
    .bind(id)
    .execute(&ctx.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(not_found());
    }
    if reach_changed {
        // The last check was of the old settings. Its tools stay listed until the next check.
        sqlx::query("UPDATE mcp_servers SET status = 'unchecked', last_error = NULL WHERE id = ?")
            .bind(id)
            .execute(&ctx.db)
            .await?;
    }
    Ok(Json(out(ctx, id).await?))
}

async fn remove(State(state): State<AppState>, P(id): P<i64>) -> Result<StatusCode, ApiError> {
    let ctx = &state.ctx;
    fetch(ctx, id).await?;
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM runs r WHERE r.status IN ('running', 'pausing') AND r.tools IS NOT NULL AND EXISTS \
         (SELECT 1 FROM json_each(r.tools, '$.servers') s WHERE json_extract(s.value, '$.server_id') = ?)",
    )
    .bind(id)
    .fetch_one(&ctx.db)
    .await?;
    if active > 0 || state.runs.active_runs().iter().any(|r| r.mcp_server_ids.contains(&id)) {
        return Err(ApiError::conflict("A run is using this MCP server. Pause it before deleting the server."));
    }
    // Runs keep their tool snapshot (names and definitions); they cannot resume without the server.
    sqlx::query("DELETE FROM mcp_servers WHERE id = ?").bind(id).execute(&ctx.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn check(State(state): State<AppState>, P(id): P<i64>) -> Result<Json<McpServerOut>, ApiError> {
    let ctx = &state.ctx;
    let row = fetch(ctx, id).await?;
    if row.transport() == McpTransport::Stdio && !ctx.config.allow_stdio_mcp {
        return Err(stdio_blocked());
    }
    run_check(ctx, &row).await?;
    Ok(Json(out(ctx, id).await?))
}
