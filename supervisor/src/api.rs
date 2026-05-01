use crate::supervisor::{Supervisor, TrackedJob};
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use subtle::ConstantTimeEq;

/// Header carrying the shared secret for all non-health HTTP routes.
pub const AUTH_HEADER: &str = "X-Supervisor-Token";
/// Query parameter carrying the shared secret for WebSocket routes.
pub const AUTH_QUERY: &str = "token";

#[derive(Clone)]
pub struct ApiState {
    pub supervisor: Supervisor,
    pub shared_secret: Arc<String>,
}

/// Constant-time secret comparison. Empty configured secret means "no auth
/// configured" — startup refuses to run in that state, so an empty secret here
/// would only ever come from a misuse in tests; treat it as a non-match.
pub fn secrets_match(provided: &str, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    let a = provided.as_bytes();
    let b = expected.as_bytes();
    if a.len() != b.len() {
        // Still do a constant-time compare against a same-length buffer to
        // avoid trivially leaking length, then return false.
        let pad = vec![0u8; b.len()];
        let _ = pad.ct_eq(b);
        return false;
    }
    a.ct_eq(b).into()
}

/// Reject requests missing or with the wrong shared-secret header.
/// Health endpoints are mounted on a separate router and bypass this layer.
pub async fn require_auth(
    State(state): State<ApiState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = req
        .headers()
        .get(AUTH_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !secrets_match(provided, &state.shared_secret) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(req).await)
}

/// True iff the input looks like a UUID — 32 hex chars, optionally with the
/// canonical 8-4-4-4-12 dash layout. Centralized so every route validates the
/// same way.
pub fn is_uuid_shaped(s: &str) -> bool {
    let bytes = s.as_bytes();
    match bytes.len() {
        32 => bytes.iter().all(|b| b.is_ascii_hexdigit()),
        36 => {
            for (i, b) in bytes.iter().enumerate() {
                let dash = matches!(i, 8 | 13 | 18 | 23);
                if dash {
                    if *b != b'-' {
                        return false;
                    }
                } else if !b.is_ascii_hexdigit() {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

fn bad_run_id() -> Response {
    (StatusCode::BAD_REQUEST, "invalid run_id (must be UUID)").into_response()
}

#[derive(Debug, Deserialize)]
pub struct TrackRequest {
    pub run_id: String,
    pub tinker_job_id: String,
    #[serde(default = "default_gpu_type")]
    pub gpu_type: String,
    #[serde(default = "default_gpu_count")]
    pub gpu_count: i64,
}

fn default_gpu_type() -> String {
    "H100".into()
}
fn default_gpu_count() -> i64 {
    1
}

#[derive(Debug, Serialize)]
pub struct TrackResponse {
    pub run_id: String,
    pub started: bool,
    pub already_tracked: bool,
}

pub async fn track(State(state): State<ApiState>, Json(req): Json<TrackRequest>) -> Response {
    if !is_uuid_shaped(&req.run_id) {
        return bad_run_id();
    }
    let started = state.supervisor.track(TrackedJob {
        run_id: req.run_id.clone(),
        tinker_job_id: req.tinker_job_id,
        gpu_type: req.gpu_type,
        gpu_count: req.gpu_count,
    });
    (
        StatusCode::OK,
        Json(TrackResponse {
            run_id: req.run_id,
            started,
            already_tracked: !started,
        }),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
pub struct UntrackResponse {
    pub run_id: String,
    pub stopped: bool,
}

pub async fn untrack(State(state): State<ApiState>, Path(run_id): Path<String>) -> Response {
    if !is_uuid_shaped(&run_id) {
        return bad_run_id();
    }
    let stopped = state.supervisor.untrack(&run_id);
    (StatusCode::OK, Json(UntrackResponse { run_id, stopped })).into_response()
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub tracked: Vec<TrackedJob>,
}

pub async fn status(State(state): State<ApiState>) -> impl IntoResponse {
    Json(StatusResponse {
        tracked: state.supervisor.tracked(),
    })
}

pub async fn health() -> impl IntoResponse {
    let mut resp = (StatusCode::OK, "ok").into_response();
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp
}

// Body type re-export (kept so tests can construct requests easily).
pub type AxumBody = Body;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_shapes() {
        assert!(is_uuid_shaped("550e8400e29b41d4a716446655440000"));
        assert!(is_uuid_shaped("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!is_uuid_shaped(""));
        assert!(!is_uuid_shaped("notauuid"));
        assert!(!is_uuid_shaped("550e8400-e29b-41d4-a716-44665544000Z"));
        assert!(!is_uuid_shaped("'; DROP TABLE runs;--"));
    }

    #[test]
    fn secret_compare() {
        assert!(secrets_match("hunter2", "hunter2"));
        assert!(!secrets_match("hunter2", "hunter3"));
        assert!(!secrets_match("", "hunter2"));
        assert!(!secrets_match("hunter2", ""));
        assert!(!secrets_match("short", "longer-secret"));
    }
}
