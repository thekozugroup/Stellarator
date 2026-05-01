//! Read-only verification that the canonical Python (Alembic) schema is present.
//!
//! The supervisor never creates or migrates tables — that's owned by the Python
//! backend. On startup we confirm that the tables and columns we depend on are
//! there, and that the unique index used for metric idempotency exists. If any
//! check fails we abort with a clear, actionable error.

use anyhow::{anyhow, bail, Context, Result};
use sqlx::Row;

use crate::db::Db;

const REQUIRED_RUN_COLUMNS: &[&str] = &[
    "id",
    "status",
    "gpu_seconds",
    "cost_usd",
    "started_at",
    "finished_at",
];

const REQUIRED_METRIC_COLUMNS: &[&str] = &["id", "run_id", "step", "name", "value", "created_at"];

const MIGRATE_HINT: &str =
    "schema is missing or incomplete — run `alembic upgrade head` in the Python backend before starting the supervisor";

pub async fn verify(pool: &Db) -> Result<()> {
    verify_table(pool, "runs", REQUIRED_RUN_COLUMNS).await?;
    verify_table(pool, "run_metrics", REQUIRED_METRIC_COLUMNS).await?;
    verify_unique_index(pool).await?;
    Ok(())
}

async fn verify_table(pool: &Db, table: &str, required: &[&str]) -> Result<()> {
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name = ?1")
            .bind(table)
            .fetch_optional(pool)
            .await
            .with_context(|| format!("probing for table {table}"))?;

    if exists.is_none() {
        bail!("required table `{table}` does not exist; {MIGRATE_HINT}");
    }

    let pragma = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(&pragma)
        .fetch_all(pool)
        .await
        .with_context(|| format!("reading pragma for {table}"))?;

    let cols: Vec<String> = rows
        .iter()
        .map(|r| r.try_get::<String, _>("name").unwrap_or_default())
        .collect();

    for col in required {
        if !cols.iter().any(|c| c == col) {
            bail!(
                "table `{table}` is missing required column `{col}` (have: {:?}); {MIGRATE_HINT}",
                cols
            );
        }
    }
    Ok(())
}

/// We rely on a UNIQUE INDEX on (run_id, step, name) for ON CONFLICT idempotency.
/// Accept either a UNIQUE INDEX or a UNIQUE table-level constraint that creates one.
async fn verify_unique_index(pool: &Db) -> Result<()> {
    let idx_rows = sqlx::query("PRAGMA index_list(run_metrics)")
        .fetch_all(pool)
        .await
        .context("reading index list for run_metrics")?;

    for row in idx_rows {
        let unique: i64 = row.try_get("unique").unwrap_or(0);
        if unique == 0 {
            continue;
        }
        let name: String = row.try_get("name").unwrap_or_default();
        let info = sqlx::query(&format!("PRAGMA index_info({name})"))
            .fetch_all(pool)
            .await
            .map_err(|e| anyhow!("reading index_info({name}): {e}"))?;
        let mut cols: Vec<String> = info
            .iter()
            .map(|r| r.try_get::<String, _>("name").unwrap_or_default())
            .collect();
        cols.sort();
        let mut want = ["run_id", "step", "name"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        want.sort();
        if cols == want {
            return Ok(());
        }
    }

    bail!(
        "run_metrics is missing the UNIQUE index on (run_id, step, name) required for idempotent metric writes; {MIGRATE_HINT}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn mem() -> Db {
        SqlitePoolOptions::new()
            .max_connections(2)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn missing_runs_table_fails() {
        let pool = mem().await;
        let err = verify(&pool).await.unwrap_err().to_string();
        assert!(err.contains("runs"), "{err}");
        assert!(err.contains("alembic upgrade head"), "{err}");
    }

    #[tokio::test]
    async fn happy_path_passes() {
        let pool = mem().await;
        crate::db::init_test_schema(&pool).await.unwrap();
        verify(&pool).await.unwrap();
    }
}
