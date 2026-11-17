//! `GET` and `PUT /api/settings`.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;
use stellarator_core::config::Mode;
use stellarator_core::schemas::{SettingsIn, SettingsOut};
use stellarator_core::{settings, ApiError, ApiJson, Ctx};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/settings", get(read).put(write))
}

async fn out(ctx: &Ctx) -> SettingsOut {
    let token = settings::hf_token(ctx).await;
    SettingsOut {
        hf_token_set: token.is_some(),
        hf_token_hint: settings::key_hint(token.as_deref()),
        hf_token_from_env: ctx.config.env_hf_token.is_some(),
        data_dir: ctx.config.data_dir.display().to_string(),
        version: ctx.config.version.to_string(),
        mode: match ctx.config.mode {
            Mode::Docker => "docker",
            Mode::Desktop => "desktop",
            Mode::Native => "native",
        },
    }
}

async fn read(State(state): State<AppState>) -> Json<SettingsOut> {
    Json(out(&state.ctx).await)
}

async fn write(State(state): State<AppState>, ApiJson(body): ApiJson<SettingsIn>) -> Result<Json<SettingsOut>, ApiError> {
    if let Some(token) = body.hf_token {
        let token = token.trim();
        let value = (!token.is_empty()).then(|| Value::String(token.to_string()));
        settings::set(&state.ctx.db, "hf_token", value).await?;
    }
    Ok(Json(out(&state.ctx).await))
}
