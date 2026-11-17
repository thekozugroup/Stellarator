//! This crate's own routes: health, endpoints, endpoint presets, prompt sets, preferences and
//! settings. Runs, imports and exports come from their crates (see `Parts::extra`).

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde_json::json;
use stellarator_core::ApiError;

use crate::AppState;

mod endpoints;
mod preferences;
mod presets;
mod prompt_sets;
mod settings;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .merge(endpoints::router())
        .merge(presets::router())
        .merge(prompt_sets::router())
        .merge(preferences::router())
        .merge(settings::router())
        .with_state(state)
}

async fn health(axum::extract::State(state): axum::extract::State<AppState>) -> Json<serde_json::Value> {
    Json(json!({"ok": true, "version": state.ctx.config.version}))
}

/// `Path<T>` whose rejections are 422 `{detail}` like FastAPI's, not axum's plain-text 400.
pub(crate) struct P<T>(pub T);

impl<S, T> FromRequestParts<S> for P<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Path(v)) => Ok(P(v)),
            Err(_) => Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "Not a valid id.")),
        }
    }
}

/// 422 with a plain `detail` and no field map (FastAPI's `HTTPException(422, ...)`).
pub(crate) fn unprocessable(detail: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, detail)
}

/// Page parameter checks with pydantic's wording (`ge` / `le`).
pub(crate) fn check_range(
    errors: &mut stellarator_core::FieldErrors,
    field: &str,
    value: i64,
    min: i64,
    max: Option<i64>,
) {
    if value < min {
        errors.add(field, format!("Input should be greater than or equal to {min}"));
    } else if let Some(max) = max.filter(|m| value > *m) {
        errors.add(field, format!("Input should be less than or equal to {max}"));
    }
}
