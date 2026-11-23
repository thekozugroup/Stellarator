//! Shared pieces of the Stellarator backend: configuration, the SQLite database, the API contract
//! (request and response types, from `stellarator-api`), errors, and helpers for
//! talking to OpenAI-compatible servers.
//!
//! Conventions every crate follows:
//! - Timestamps are UTC. They are stored as `YYYY-MM-DD HH:MM:SS.ffffff` text (see [`time`]) and
//!   sent to the browser as RFC 3339 with a `Z`.
//! - JSON columns are stored as text and read with `sqlx::types::Json`.
//! - Handlers return `Result<_, ApiError>`; errors reach the browser as
//!   `{"detail": "One sentence.", "fields": {"field.path": "message"}}`.

pub mod config;
pub mod datalock;
pub mod db;
pub mod error;
pub mod openai;
pub mod presets;
pub mod probes;
pub mod schemas;
pub mod settings;
pub mod time;

pub use config::Config;
pub use error::{ApiError, ApiJson, ApiQuery, FieldErrors};

use std::sync::Arc;

/// What every router needs: the database, the configuration and one shared HTTP client.
#[derive(Clone)]
pub struct Ctx {
    pub db: sqlx::SqlitePool,
    pub config: Arc<Config>,
    /// For short calls (model lists, Hugging Face metadata). Long streaming generation requests
    /// build their own client with per-run timeouts.
    pub http: reqwest::Client,
}

impl Ctx {
    /// Opens the data folder: takes its lock ([`datalock`]: one process per folder), then the
    /// database.
    pub async fn open(config: Config) -> anyhow::Result<Self> {
        datalock::acquire(&config.data_dir)?;
        let db = db::connect(&config).await?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("stellarator/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Self { db, config: Arc::new(config), http })
    }
}
