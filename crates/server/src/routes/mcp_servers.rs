//! `/api/mcp-servers`: saved MCP servers whose tools runs can call.
//!
//! - `GET /api/mcp-servers` -> `[McpServerOut]`
//! - `POST /api/mcp-servers` (`McpServerIn`) -> 201 `McpServerOut`
//! - `POST /api/mcp-servers/import` (`McpImportIn`) -> `McpImportOut`
//! - `GET /api/mcp-servers/{id}` -> `McpServerOut`
//! - `PATCH /api/mcp-servers/{id}` (`McpServerPatch`) -> `McpServerOut`
//! - `DELETE /api/mcp-servers/{id}` -> 204 (409 while a running run uses it)
//! - `POST /api/mcp-servers/{id}/check` -> `McpServerOut`: connect, initialize, list the tools,
//!   cache them; `status` is `ok` or `error` with `error` set.
//!
//! Stdio servers in server mode without STELLARATOR_ALLOW_STDIO_MCP=1: create, import, check and
//! runs answer 403 with the reason.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use stellarator_core::schemas::{McpImportIn, McpImportOut, McpServerIn, McpServerOut, McpServerPatch};
use stellarator_core::{ApiError, ApiJson};

use super::P;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/mcp-servers", get(list).post(create))
        .route("/api/mcp-servers/import", post(import))
        .route("/api/mcp-servers/{id}", get(read).patch(update).delete(remove))
        .route("/api/mcp-servers/{id}/check", post(check))
}

fn not_yet() -> ApiError {
    ApiError::new(StatusCode::NOT_IMPLEMENTED, "MCP servers are not implemented yet.")
}

async fn list(State(_state): State<AppState>) -> Result<Json<Vec<McpServerOut>>, ApiError> {
    Err(not_yet())
}

async fn create(
    State(_state): State<AppState>,
    ApiJson(_body): ApiJson<McpServerIn>,
) -> Result<(StatusCode, Json<McpServerOut>), ApiError> {
    Err(not_yet())
}

async fn import(
    State(_state): State<AppState>,
    ApiJson(_body): ApiJson<McpImportIn>,
) -> Result<Json<McpImportOut>, ApiError> {
    Err(not_yet())
}

async fn read(State(_state): State<AppState>, P(_id): P<i64>) -> Result<Json<McpServerOut>, ApiError> {
    Err(not_yet())
}

async fn update(
    State(_state): State<AppState>,
    P(_id): P<i64>,
    ApiJson(_body): ApiJson<McpServerPatch>,
) -> Result<Json<McpServerOut>, ApiError> {
    Err(not_yet())
}

async fn remove(State(_state): State<AppState>, P(_id): P<i64>) -> Result<StatusCode, ApiError> {
    Err(not_yet())
}

async fn check(State(_state): State<AppState>, P(_id): P<i64>) -> Result<Json<McpServerOut>, ApiError> {
    Err(not_yet())
}
