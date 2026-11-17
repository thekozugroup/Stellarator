//! This crate's own routes: health, endpoints, endpoint presets, prompt sets, preferences and
//! settings. Runs, imports and exports come from their crates (see `Parts::extra`).

use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .with_state(state)
}

async fn health(axum::extract::State(state): axum::extract::State<AppState>) -> Json<serde_json::Value> {
    Json(json!({"ok": true, "version": state.ctx.config.version}))
}
