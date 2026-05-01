#![deny(unused_must_use)]

use anyhow::{anyhow, Context, Result};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use std::time::Duration;
use stellarator_supervisor::{
    api::{self, ApiState},
    config::Config,
    db,
    schema_check,
    supervisor::Supervisor,
    tinker::TinkerClient,
    ws::{self, Hub, WsState},
};
use tokio_util::sync::CancellationToken;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

const SHUTDOWN_DRAIN_SECS: u64 = 10;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy_safe();

    // Scrub potentially-sensitive headers from any spans tower-http creates.
    // These names are normalized lowercase in HeaderMap, so we filter at format
    // time via the `EnvFilter` plus a custom field formatter wrapper; for now
    // we just ensure the secret values themselves are never placed in spans.
    init_tracing();

    let cfg = Arc::new(Config::from_env()?);

    // Required: a non-empty shared secret. Refuse to boot otherwise — this
    // would silently disable auth on every protected route.
    let shared_secret = std::env::var("SUPERVISOR_SHARED_SECRET").unwrap_or_default();
    if shared_secret.trim().is_empty() {
        return Err(anyhow!(
            "SUPERVISOR_SHARED_SECRET is not set; refusing to start without an auth secret"
        ));
    }
    let shared_secret = Arc::new(shared_secret);

    // NOTE: do NOT log shared_secret or tinker_api_key.
    info!(bind = %cfg.bind_addr, "stellarator-supervisor starting");

    let pool = db::connect(&cfg.sqlite_url())
        .await
        .context("connecting to sqlite")?;

    schema_check::verify(&pool)
        .await
        .context("schema verification failed")?;

    let tinker = TinkerClient::new(cfg.tinker_base_url.clone(), cfg.tinker_api_key.clone())?;
    let hub = Arc::new(Hub::new());
    let shutdown = CancellationToken::new();

    let supervisor = Supervisor::new(cfg.clone(), pool.clone(), tinker, hub.clone(), shutdown.clone());

    let api_state = ApiState {
        supervisor: supervisor.clone(),
        shared_secret: shared_secret.clone(),
    };
    let ws_state = WsState {
        hub,
        shared_secret: shared_secret.clone(),
        shutdown: shutdown.clone(),
    };

    // Health endpoints are unauthenticated and live on a separate router.
    let health_router = Router::new()
        .route("/health", get(api::health))
        .route("/healthz", get(api::health));

    let protected_router = Router::new()
        .route("/supervisor/status", get(api::status))
        .route("/supervisor/track", post(api::track))
        .route("/supervisor/untrack/:run_id", post(api::untrack))
        .route_layer(axum::middleware::from_fn_with_state(
            api_state.clone(),
            api::require_auth,
        ))
        .with_state(api_state);

    let ws_router = Router::new()
        .route("/ws/runs", get(ws::ws_all))
        .route("/ws/runs/:run_id", get(ws::ws_run))
        .with_state(ws_state);

    let app = health_router
        .merge(protected_router)
        .merge(ws_router)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr)
        .await
        .with_context(|| format!("bind {}", cfg.bind_addr))?;
    info!("listening on {}", cfg.bind_addr);

    let shutdown_for_serve = shutdown.clone();
    let serve = axum::serve(listener, app).with_graceful_shutdown(async move {
        shutdown_for_serve.cancelled().await;
    });

    let signals = wait_for_shutdown_signal(shutdown.clone());
    tokio::select! {
        res = serve => {
            if let Err(e) = res {
                warn!(error=?e, "axum serve error");
            }
        }
        _ = signals => {
            info!("shutdown signal received; draining");
            shutdown.cancel();
        }
    }

    // Graceful drain: cancel each supervised job and wait up to N seconds for
    // all in-flight tasks to wind down. Then close the pool to flush WAL.
    let supervisor_for_drain = supervisor.clone();
    for j in supervisor_for_drain.tracked() {
        supervisor_for_drain.untrack(&j.run_id);
    }

    let drain_deadline = Duration::from_secs(SHUTDOWN_DRAIN_SECS);
    let drain = async {
        loop {
            if supervisor_for_drain.tracked().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    if tokio::time::timeout(drain_deadline, drain).await.is_err() {
        warn!("drain timeout exceeded; forcing exit");
    }
    pool.close().await;

    Ok(())
}

async fn wait_for_shutdown_signal(_shutdown: CancellationToken) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                warn!(error=?e, "failed to install SIGTERM handler; falling back to ctrl_c only");
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn init_tracing() {
    // The shared secret and Tinker API key are only ever read from `Config`
    // and never placed into tracing fields. As a defense-in-depth we omit the
    // headers layer entirely and only enable the request-line trace so that
    // an `Authorization` or `X-Supervisor-Token` header value cannot be
    // accidentally captured in a span.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .try_init();
}

fn dotenvy_safe() -> Result<()> {
    if let Ok(content) = std::fs::read_to_string(".env") {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                if std::env::var(k).is_err() {
                    std::env::set_var(k.trim(), v.trim().trim_matches('"'));
                }
            }
        }
    }
    Ok(())
}
