//! Verify that protected routes require the shared-secret header and that
//! `/health` is exempt.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use sqlx::sqlite::SqlitePoolOptions;
use stellarator_supervisor::api::{self, ApiState};
use stellarator_supervisor::config::Config;
use stellarator_supervisor::db::init_test_schema;
use stellarator_supervisor::supervisor::Supervisor;
use stellarator_supervisor::tinker::TinkerClient;
use stellarator_supervisor::ws::Hub;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

const SECRET: &str = "test-shared-secret-value";

fn cfg() -> Config {
    Config {
        tinker_api_key: "x".into(),
        tinker_base_url: "http://127.0.0.1:1".into(),
        db_url: ":memory:".into(),
        bind_addr: "0".into(),
        poll_interval_secs: 1,
        cost_h100_per_hour: 4.50,
        cost_a100_per_hour: 2.20,
    }
}

async fn make_app() -> Router {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    init_test_schema(&pool).await.unwrap();
    let tinker = TinkerClient::new("http://x", "k").unwrap();
    let hub = Arc::new(Hub::new());
    let sup = Supervisor::new(Arc::new(cfg()), pool, tinker, hub, CancellationToken::new());
    let state = ApiState {
        supervisor: sup,
        shared_secret: Arc::new(SECRET.to_string()),
    };

    Router::new()
        .route("/health", get(api::health))
        .route("/healthz", get(api::health))
        .merge(
            Router::new()
                .route("/supervisor/status", get(api::status))
                .route("/supervisor/track", post(api::track))
                .route("/supervisor/untrack/:run_id", post(api::untrack))
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    api::require_auth,
                ))
                .with_state(state),
        )
}

#[tokio::test]
async fn health_is_unauthenticated() {
    let app = make_app().await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn missing_token_is_unauthorized() {
    let app = make_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/supervisor/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_token_is_unauthorized() {
    let app = make_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/supervisor/status")
                .header("X-Supervisor-Token", "wrong-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn correct_token_is_ok() {
    let app = make_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/supervisor/status")
                .header("X-Supervisor-Token", SECRET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn track_rejects_non_uuid_run_id() {
    let app = make_app().await;
    let body = serde_json::json!({
        "run_id": "not-a-uuid",
        "tinker_job_id": "tj1",
    })
    .to_string();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/supervisor/track")
                .header("X-Supervisor-Token", SECRET)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
