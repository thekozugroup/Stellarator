use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::str::FromStr;
use std::time::Duration;
use tracing::warn;

pub type Db = Pool<Sqlite>;

/// Query the maximum step ever recorded for a run, or -1 if none.
/// Used to initialize last_step on supervisor restart, avoiding re-application
/// of historical metrics and reducing ON CONFLICT churn.
pub async fn get_last_step(pool: &Db, run_id: &str) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COALESCE(MAX(step), -1) FROM run_metrics WHERE run_id = ?")
        .bind(run_id)
        .fetch_one(pool)
        .await
        .context("fetch max step")?;
    Ok(row.0)
}

pub async fn connect(url: &str) -> Result<Db> {
    let opts = SqliteConnectOptions::from_str(url)
        .with_context(|| format!("invalid sqlite url: {url}"))?
        .busy_timeout(Duration::from_secs(10))
        .pragma("journal_mode", "WAL")
        .pragma("synchronous", "NORMAL")
        .create_if_missing(false);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(15))
        .connect_with(opts)
        .await
        .context("connecting to sqlite")?;
    Ok(pool)
}

/// Run an async DB op, retrying a few times on SQLITE_BUSY / locked errors.
pub async fn with_retry<F, Fut, T>(mut op: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
{
    let mut delay_ms = 25u64;
    for attempt in 0..6 {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let msg = format!("{e}");
                let busy = msg.contains("database is locked")
                    || msg.contains("SQLITE_BUSY")
                    || msg.contains("locked");
                if !busy || attempt == 5 {
                    return Err(e.into());
                }
                warn!(attempt, delay_ms, "db busy, retrying");
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                delay_ms = (delay_ms * 2).min(800);
            }
        }
    }
    unreachable!()
}

/// Schema used by tests ONLY. In production the Python backend (Alembic) owns
/// the canonical schema and the supervisor only verifies it (see schema_check).
/// This mirror is intentionally minimal — keep columns in sync with
/// backend/app/models/run.py and the corresponding Alembic migration.
pub async fn init_test_schema(pool: &Db) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS runs (
            id TEXT PRIMARY KEY,
            owner_agent TEXT,
            name TEXT,
            status TEXT,
            base_model TEXT,
            method TEXT,
            hyperparams TEXT,
            dataset_mixture TEXT,
            user_goal TEXT,
            user_context TEXT,
            agent_plan TEXT,
            citations TEXT,
            tinker_job_id TEXT,
            gpu_type TEXT DEFAULT 'H100',
            gpu_count INTEGER DEFAULT 1,
            gpu_seconds REAL DEFAULT 0.0,
            cost_usd REAL DEFAULT 0.0,
            created_at TEXT,
            started_at TEXT,
            finished_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS run_metrics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id TEXT NOT NULL,
            step INTEGER DEFAULT 0,
            name TEXT NOT NULL,
            value REAL NOT NULL,
            created_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"CREATE UNIQUE INDEX IF NOT EXISTS ix_run_metrics_run_step_name
           ON run_metrics(run_id, step, name);"#,
    )
    .execute(pool)
    .await?;
    Ok(())
}
