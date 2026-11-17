//! `stellarator-server`: serve the API and the web UI.
//!
//! Environment: STELLARATOR_DATA_DIR (default ./data), STELLARATOR_BIND (default 0.0.0.0:8080),
//! STELLARATOR_DOCKER=1 inside the Docker image, HF_TOKEN.

use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    stellarator_server::init_tracing();
    let config = stellarator_core::Config::from_env();
    let bind: SocketAddr = std::env::var("STELLARATOR_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    let parts = stellarator_server::minimal_parts(config).await?;
    let server = stellarator_server::start(parts, bind).await?;
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");
    server.shutdown().await;
    Ok(())
}
