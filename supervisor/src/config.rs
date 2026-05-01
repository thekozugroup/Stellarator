use anyhow::{Context, Result};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub tinker_api_key: String,
    pub tinker_base_url: String,
    pub db_url: String,
    pub bind_addr: String,
    pub poll_interval_secs: u64,
    pub cost_h100_per_hour: f64,
    pub cost_a100_per_hour: f64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            tinker_api_key: env::var("TINKER_API_KEY").unwrap_or_default(),
            tinker_base_url: env::var("TINKER_BASE_URL")
                .unwrap_or_else(|_| "https://api.tinker.thinkingmachines.ai".to_string()),
            db_url: env::var("STELLARATOR_DB_URL")
                .context("STELLARATOR_DB_URL not set (expected sqlite path or sqlite:// url)")?,
            bind_addr: env::var("SUPERVISOR_BIND").unwrap_or_else(|_| "0.0.0.0:8001".to_string()),
            poll_interval_secs: env::var("SUPERVISOR_POLL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            cost_h100_per_hour: env::var("COST_H100_PER_HOUR")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(4.50),
            cost_a100_per_hour: env::var("COST_A100_PER_HOUR")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2.20),
        })
    }

    pub fn sqlite_url(&self) -> String {
        if self.db_url.starts_with("sqlite:") {
            self.db_url.clone()
        } else {
            format!("sqlite://{}?mode=rwc", self.db_url)
        }
    }
}
