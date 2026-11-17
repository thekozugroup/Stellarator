//! v2: `GET /api/endpoint-presets`.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use stellarator_core::presets::endpoint_presets;
use stellarator_core::schemas::EndpointPreset;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/endpoint-presets", get(list))
}

async fn list(State(state): State<AppState>) -> Json<Vec<EndpointPreset>> {
    Json(endpoint_presets(&state.ctx.config))
}
