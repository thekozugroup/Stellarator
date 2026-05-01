use crate::tinker::TinkerClient;
use crate::ws::Hub;
use crate::Config;
use dashmap::DashMap;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub db: SqlitePool,
    pub tinker: TinkerClient,
    pub jobs: Arc<DashMap<String, TrackedJob>>,
    pub hub: Arc<Hub>,
    pub shutdown: CancellationToken,
}

#[derive(Clone)]
pub struct TrackedJob {
    pub run_id: String,
    pub tinker_job_id: String,
    pub gpu_type: String,
    pub gpu_count: i64,
    pub cancel: CancellationToken,
}
