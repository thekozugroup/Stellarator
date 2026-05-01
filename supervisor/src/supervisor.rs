use crate::config::Config;
use crate::cost::cost_for;
use crate::db::{with_retry, Db};
use crate::tinker::{TinkerClient, TinkerJob};
use crate::ws::Hub;
use anyhow::{Context, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

const TERMINAL: &[&str] = &[
    "succeeded",
    "failed",
    "cancelled",
    "completed",
    "errored",
    "error",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricUpdate {
    pub run_id: String,
    pub status: String,
    pub step: i64,
    pub name: String,
    pub value: f64,
    pub gpu_seconds: f64,
    pub cost_usd: f64,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedJob {
    pub run_id: String,
    pub tinker_job_id: String,
    pub gpu_type: String,
    pub gpu_count: i64,
}

pub struct JobHandle {
    pub job: TrackedJob,
    pub cancel: CancellationToken,
}

#[derive(Clone)]
pub struct Supervisor {
    cfg: Arc<Config>,
    db: Db,
    tinker: TinkerClient,
    hub: Arc<Hub>,
    jobs: Arc<DashMap<String, Arc<JobHandle>>>,
    shutdown: CancellationToken,
}

impl Supervisor {
    pub fn new(
        cfg: Arc<Config>,
        db: Db,
        tinker: TinkerClient,
        hub: Arc<Hub>,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            cfg,
            db,
            tinker,
            hub,
            jobs: Arc::new(DashMap::new()),
            shutdown,
        }
    }

    pub fn hub(&self) -> Arc<Hub> {
        self.hub.clone()
    }

    pub fn tracked(&self) -> Vec<TrackedJob> {
        self.jobs.iter().map(|kv| kv.value().job.clone()).collect()
    }

    /// Idempotent: returns true if a new task was spawned.
    pub fn track(&self, job: TrackedJob) -> bool {
        if self.jobs.contains_key(&job.run_id) {
            return false;
        }
        let cancel = CancellationToken::new();
        let handle = Arc::new(JobHandle {
            job: job.clone(),
            cancel: cancel.clone(),
        });
        self.jobs.insert(job.run_id.clone(), handle);

        let this = self.clone();
        tokio::spawn(async move {
            if let Err(e) = this.run(job.clone(), cancel).await {
                error!(run_id=%job.run_id, error=?e, "supervisor task exited");
            }
            this.jobs.remove(&job.run_id);
        });
        true
    }

    pub fn untrack(&self, run_id: &str) -> bool {
        if let Some((_, h)) = self.jobs.remove(run_id) {
            h.cancel.cancel();
            true
        } else {
            false
        }
    }

    async fn run(&self, job: TrackedJob, cancel: CancellationToken) -> Result<()> {
        info!(run_id=%job.run_id, tinker_job_id=%job.tinker_job_id, "supervising");
        let interval = Duration::from_secs(self.cfg.poll_interval_secs.max(1));

        // Recover last_step from DB to avoid re-applying historical metrics after restart.
        // On first boot, run_metrics is empty so COALESCE returns -1 naturally.
        let mut last_step: i64 = match crate::db::get_last_step(&self.db, &job.run_id).await {
            Ok(step) => {
                info!(run_id=%job.run_id, last_step=step, "recovered last_step from DB");
                step
            }
            Err(e) => {
                warn!(run_id=%job.run_id, error=?e, "failed to load last_step, assuming -1");
                -1
            }
        };

        loop {
            tokio::select! {
                _ = cancel.cancelled() => { info!(run_id=%job.run_id, "cancelled"); return Ok(()); }
                _ = self.shutdown.cancelled() => { info!(run_id=%job.run_id, "global shutdown"); return Ok(()); }
                _ = tokio::time::sleep(interval) => {}
            }

            let tj = match self.tinker.get_job(&job.tinker_job_id).await {
                Ok(Some(j)) => j,
                Ok(None) => {
                    warn!(run_id=%job.run_id, "tinker job not found; stopping");
                    return Ok(());
                }
                Err(e) => {
                    warn!(run_id=%job.run_id, error=?e, "poll error; will retry");
                    continue;
                }
            };

            if let Err(e) = self.apply_update(&job, &tj, &mut last_step).await {
                warn!(run_id=%job.run_id, error=?e, "apply_update failed");
            }

            if TERMINAL.iter().any(|t| t.eq_ignore_ascii_case(&tj.status)) {
                info!(run_id=%job.run_id, status=%tj.status, "terminal; exiting");
                return Ok(());
            }
        }
    }

    async fn apply_update(
        &self,
        job: &TrackedJob,
        tj: &TinkerJob,
        last_step: &mut i64,
    ) -> Result<()> {
        let gpu_seconds = tj.gpu_seconds.unwrap_or(0.0);
        let cost = cost_for(&self.cfg, &job.gpu_type, job.gpu_count, gpu_seconds);
        let status = tj.status.clone();
        let started = tj.started_at.clone();
        let finished = tj.finished_at.clone();

        // In-memory dedupe: only forward-progressing steps trigger writes.
        // Real idempotency comes from the UNIQUE INDEX on (run_id, step, name)
        // + ON CONFLICT below; this is just a fast path to skip re-work.
        let metric_pairs: Vec<(String, f64)> = tj
            .metrics
            .as_ref()
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
                    .collect()
            })
            .unwrap_or_default();
        let candidate_step = tj.step.unwrap_or(*last_step + 1);
        let should_write_metrics = !metric_pairs.is_empty() && candidate_step > *last_step;

        // Single transaction per apply: UPDATE runs + N upserts into run_metrics.
        // Wrapped in with_retry so SQLITE_BUSY rolls back and retries the whole tx.
        let pool = self.db.clone();
        let run_id = job.run_id.clone();
        let metric_pairs_for_tx = metric_pairs.clone();
        with_retry(|| {
            let pool = pool.clone();
            let run_id = run_id.clone();
            let s = status.clone();
            let started = started.clone();
            let finished = finished.clone();
            let pairs = metric_pairs_for_tx.clone();
            async move {
                let mut tx = pool.begin().await?;
                sqlx::query(
                    r#"UPDATE runs
                       SET status = ?1,
                           gpu_seconds = ?2,
                           cost_usd = ?3,
                           started_at = COALESCE(started_at, ?4),
                           finished_at = COALESCE(?5, finished_at)
                       WHERE id = ?6"#,
                )
                .bind(&s)
                .bind(gpu_seconds)
                .bind(cost)
                .bind(&started)
                .bind(&finished)
                .bind(&run_id)
                .execute(&mut *tx)
                .await?;

                if should_write_metrics {
                    for (name, v) in &pairs {
                        sqlx::query(
                            r#"INSERT INTO run_metrics (run_id, step, name, value, created_at)
                               VALUES (?1, ?2, ?3, ?4, datetime('now'))
                               ON CONFLICT(run_id, step, name)
                               DO UPDATE SET value = excluded.value"#,
                        )
                        .bind(&run_id)
                        .bind(candidate_step)
                        .bind(name)
                        .bind(*v)
                        .execute(&mut *tx)
                        .await?;
                    }
                }
                tx.commit().await?;
                Ok(())
            }
        })
        .await
        .context("apply_update tx")?;

        if should_write_metrics {
            for (name, v) in &metric_pairs {
                self.hub.publish(
                    &job.run_id,
                    MetricUpdate {
                        run_id: job.run_id.clone(),
                        status: status.clone(),
                        step: candidate_step,
                        name: name.clone(),
                        value: *v,
                        gpu_seconds,
                        cost_usd: cost,
                        ts: chrono::Utc::now().timestamp(),
                    },
                );
            }
            *last_step = candidate_step;
        } else if metric_pairs.is_empty() {
            // No metrics block — emit a heartbeat so subscribers see status/cost.
            self.hub.publish(
                &job.run_id,
                MetricUpdate {
                    run_id: job.run_id.clone(),
                    status,
                    step: *last_step,
                    name: "_heartbeat".into(),
                    value: 0.0,
                    gpu_seconds,
                    cost_usd: cost,
                    ts: chrono::Utc::now().timestamp(),
                },
            );
        }
        debug!(run_id=%job.run_id, gpu_seconds, cost, "applied update");
        Ok(())
    }

    /// Test-only convenience: synchronously process a single TinkerJob payload.
    pub async fn apply_for_test(&self, job: TrackedJob, tj: TinkerJob) -> Result<()> {
        let mut last = -1i64;
        self.apply_update(&job, &tj, &mut last).await
    }
}
