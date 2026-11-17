//! `stellarator-server`: serve the API and the web UI.
//!
//! Environment: STELLARATOR_DATA_DIR (default ./data), STELLARATOR_BIND (default 0.0.0.0:8080),
//! STELLARATOR_DOCKER=1 inside the Docker image, HF_TOKEN.
//!
//! `stellarator-server healthcheck` exits 0 when the server on STELLARATOR_BIND's port answers
//! /api/health (used by the Docker HEALTHCHECK, which has no curl).

use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bind: SocketAddr = std::env::var("STELLARATOR_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        std::process::exit(if healthy(bind.port()).await { 0 } else { 1 });
    }
    stellarator_server::init_tracing();
    let config = stellarator_core::Config::from_env();
    let parts = stellarator_server::full_parts(config).await?;
    let server = stellarator_server::start(parts, bind).await?;
    shutdown_signal().await;
    tracing::info!("shutting down");
    server.shutdown().await;
    Ok(())
}

async fn healthy(port: u16) -> bool {
    let client = match reqwest::Client::builder().no_proxy().timeout(std::time::Duration::from_secs(4)).build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    matches!(client.get(format!("http://127.0.0.1:{port}/api/health")).send().await, Ok(r) if r.status().is_success())
}

/// Ctrl-C, or SIGTERM from `docker stop`.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("signal handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
