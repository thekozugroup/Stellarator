//! The Stellarator HTTP server: the JSON API under `/api` and the web UI (embedded at build time
//! from `frontend/dist`) everywhere else. Used by the `stellarator-server` binary (Docker, native)
//! and by the desktop app, which runs it inside its own process on 127.0.0.1.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use stellarator_core::probes::{ImportsProbe, RunsProbe};
use stellarator_core::{Config, Ctx};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

mod assets;
mod guard;
pub mod routes;

/// Everything the routers need. `runs` and `imports` are the engine and the import jobs, seen
/// through the probes in core so this crate's own routes don't depend on their implementations.
#[derive(Clone)]
pub struct AppState {
    pub ctx: Ctx,
    pub runs: Arc<dyn RunsProbe>,
    pub imports: Arc<dyn ImportsProbe>,
}

/// Hooks run around the server's life: recover interrupted work on start, stop it cleanly on exit.
#[async_trait::async_trait]
pub trait Lifecycle: Send + Sync {
    async fn start(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn stop(&self) {}
}

/// The parts assembled into one app. `extra` holds the routers of the engine, import and export
/// crates (each a `Router` with its state already applied).
pub struct Parts {
    pub state: AppState,
    pub extra: Vec<Router>,
    pub lifecycle: Vec<Arc<dyn Lifecycle>>,
}

/// Build the full router: this crate's routes, the other crates' routes, the web UI, and the
/// launch-token guard when the config has one.
pub fn app(parts: &Parts) -> Router {
    let mut api = routes::router(parts.state.clone());
    for r in &parts.extra {
        api = api.merge(r.clone());
    }
    let app = api.fallback(assets::serve);
    let app = guard::wrap(app, parts.state.ctx.config.launch_token.clone());
    app.layer(tower_http::trace::TraceLayer::new_for_http())
}

pub struct RunningServer {
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    join: JoinHandle<()>,
    lifecycle: Vec<Arc<dyn Lifecycle>>,
}

impl RunningServer {
    /// Stop accepting requests, let in-flight ones finish, then stop runs and imports cleanly.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = (&mut self.join).await;
        for l in &self.lifecycle {
            l.stop().await;
        }
    }
}

/// Bind and serve. `bind` may use port 0 to pick a free port; the chosen address is returned.
pub async fn start(parts: Parts, bind: SocketAddr) -> anyhow::Result<RunningServer> {
    for l in &parts.lifecycle {
        l.start().await?;
    }
    let router = app(&parts);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let addr = listener.local_addr()?;
    let (tx, rx) = oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        let serve = axum::serve(listener, router).with_graceful_shutdown(async {
            let _ = rx.await;
        });
        if let Err(e) = serve.await {
            tracing::error!(error = %e, "server stopped with an error");
        }
    });
    tracing::info!(%addr, "Stellarator is listening");
    Ok(RunningServer { addr, shutdown: Some(tx), join, lifecycle: parts.lifecycle })
}

/// Open the database and build the parts with only this crate's routes. The binary and the
/// desktop app replace this with `full_parts` once the other crates are wired in (see main.rs).
pub async fn minimal_parts(config: Config) -> anyhow::Result<Parts> {
    let ctx = Ctx::open(config).await?;
    let idle = Arc::new(stellarator_core::probes::Idle);
    Ok(Parts {
        state: AppState { ctx, runs: idle.clone(), imports: idle },
        extra: Vec::new(),
        lifecycle: Vec::new(),
    })
}

pub fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,tower_http=warn,sqlx=warn".into());
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
