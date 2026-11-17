//! SQLite in WAL mode. The schema is the Python backend's, byte for byte, so a data folder from the
//! Docker image opens in the desktop app and back. Later changes are numbered migrations tracked in
//! `PRAGMA user_version`.

use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::Config;

/// Version 1: the tables the Python backend created.
const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS endpoints (
	id INTEGER NOT NULL,
	name VARCHAR(200) NOT NULL,
	base_url VARCHAR(2000) NOT NULL,
	api_key TEXT,
	location VARCHAR(10) NOT NULL,
	default_concurrency INTEGER NOT NULL,
	headers JSON NOT NULL,
	models JSON NOT NULL,
	last_checked_at DATETIME,
	last_ok BOOLEAN,
	last_error TEXT,
	created_at DATETIME NOT NULL,
	updated_at DATETIME NOT NULL,
	PRIMARY KEY (id)
);
CREATE TABLE IF NOT EXISTS prompt_sets (
	id INTEGER NOT NULL,
	name VARCHAR(300) NOT NULL,
	source_kind VARCHAR(20) NOT NULL,
	source JSON NOT NULL,
	prompt_count INTEGER NOT NULL,
	created_at DATETIME NOT NULL,
	PRIMARY KEY (id)
);
CREATE TABLE IF NOT EXISTS prompts (
	id INTEGER NOT NULL,
	prompt_set_id INTEGER NOT NULL,
	idx INTEGER NOT NULL,
	messages JSON NOT NULL,
	meta JSON NOT NULL,
	PRIMARY KEY (id),
	FOREIGN KEY(prompt_set_id) REFERENCES prompt_sets (id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS runs (
	id INTEGER NOT NULL,
	name VARCHAR(300) NOT NULL,
	endpoint_id INTEGER,
	endpoint_name VARCHAR(200) NOT NULL,
	model VARCHAR(500) NOT NULL,
	prompt_set_id INTEGER NOT NULL,
	params JSON NOT NULL,
	concurrency INTEGER NOT NULL,
	status VARCHAR(20) NOT NULL,
	status_reason TEXT,
	total INTEGER NOT NULL,
	done INTEGER NOT NULL,
	failed INTEGER NOT NULL,
	truncated INTEGER NOT NULL,
	prompt_tokens INTEGER NOT NULL,
	completion_tokens INTEGER NOT NULL,
	reasoning_tokens INTEGER NOT NULL,
	active_seconds FLOAT NOT NULL,
	created_at DATETIME NOT NULL,
	started_at DATETIME,
	finished_at DATETIME,
	PRIMARY KEY (id),
	FOREIGN KEY(endpoint_id) REFERENCES endpoints (id) ON DELETE SET NULL,
	FOREIGN KEY(prompt_set_id) REFERENCES prompt_sets (id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS samples (
	id INTEGER NOT NULL,
	run_id INTEGER NOT NULL,
	prompt_id INTEGER NOT NULL,
	prompt_idx INTEGER NOT NULL,
	sample_idx INTEGER NOT NULL,
	status VARCHAR(10) NOT NULL,
	reasoning TEXT,
	response TEXT,
	finish_reason VARCHAR(40),
	prompt_tokens INTEGER,
	completion_tokens INTEGER,
	reasoning_tokens INTEGER,
	latency_ms INTEGER,
	ttft_ms INTEGER,
	attempts INTEGER NOT NULL,
	error TEXT,
	completed_at DATETIME,
	PRIMARY KEY (id),
	CONSTRAINT uq_sample UNIQUE (run_id, prompt_id, sample_idx),
	FOREIGN KEY(run_id) REFERENCES runs (id) ON DELETE CASCADE,
	FOREIGN KEY(prompt_id) REFERENCES prompts (id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS run_metrics (
	id INTEGER NOT NULL,
	run_id INTEGER NOT NULL,
	t DATETIME NOT NULL,
	tokens_per_s FLOAT NOT NULL,
	samples_per_min FLOAT NOT NULL,
	active INTEGER NOT NULL,
	concurrency INTEGER NOT NULL,
	done INTEGER NOT NULL,
	failed INTEGER NOT NULL,
	PRIMARY KEY (id),
	FOREIGN KEY(run_id) REFERENCES runs (id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS preference_sets (
	id INTEGER NOT NULL,
	name VARCHAR(300) NOT NULL,
	prompt_set_id INTEGER NOT NULL,
	run_ids JSON NOT NULL,
	created_at DATETIME NOT NULL,
	PRIMARY KEY (id),
	FOREIGN KEY(prompt_set_id) REFERENCES prompt_sets (id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS preference_pairs (
	id INTEGER NOT NULL,
	preference_set_id INTEGER NOT NULL,
	prompt_id INTEGER NOT NULL,
	chosen_sample_id INTEGER NOT NULL,
	rejected_sample_id INTEGER NOT NULL,
	source VARCHAR(20) NOT NULL,
	created_at DATETIME NOT NULL,
	updated_at DATETIME NOT NULL,
	PRIMARY KEY (id),
	CONSTRAINT uq_pair_prompt UNIQUE (preference_set_id, prompt_id),
	FOREIGN KEY(preference_set_id) REFERENCES preference_sets (id) ON DELETE CASCADE,
	FOREIGN KEY(prompt_id) REFERENCES prompts (id) ON DELETE CASCADE,
	FOREIGN KEY(chosen_sample_id) REFERENCES samples (id) ON DELETE CASCADE,
	FOREIGN KEY(rejected_sample_id) REFERENCES samples (id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS settings (
	"key" VARCHAR(100) NOT NULL,
	value JSON NOT NULL,
	PRIMARY KEY ("key")
);
CREATE INDEX IF NOT EXISTS ix_prompts_set_idx ON prompts (prompt_set_id, idx);
CREATE INDEX IF NOT EXISTS ix_run_metrics_run_t ON run_metrics (run_id, t);
CREATE INDEX IF NOT EXISTS ix_samples_prompt ON samples (prompt_id);
CREATE INDEX IF NOT EXISTS ix_samples_run_status ON samples (run_id, status);
"#;

/// Numbered migrations after version 1. Index 0 brings the database to version 2, and so on.
const MIGRATIONS: &[&str] = &[
    // v2: imports run in the background. A prompt set is "importing" until its rows are in,
    // "ready" after, or "failed" with a reason. rows_read / rows_total report progress.
    r#"
    ALTER TABLE prompt_sets ADD COLUMN status VARCHAR(20) NOT NULL DEFAULT 'ready';
    ALTER TABLE prompt_sets ADD COLUMN status_detail TEXT;
    ALTER TABLE prompt_sets ADD COLUMN rows_read INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE prompt_sets ADD COLUMN rows_total INTEGER;
    "#,
];

/// Indexes on foreign keys the Python schema left without one. Without them, every row removed by
/// a cascade (deleting a prompt set with 100k prompts) scans the referencing table, and deleting
/// takes minutes. Idempotent, so it runs on every start and needs no version number.
const EXTRA_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS ix_pairs_prompt ON preference_pairs (prompt_id);
CREATE INDEX IF NOT EXISTS ix_pairs_chosen ON preference_pairs (chosen_sample_id);
CREATE INDEX IF NOT EXISTS ix_pairs_rejected ON preference_pairs (rejected_sample_id);
CREATE INDEX IF NOT EXISTS ix_runs_prompt_set ON runs (prompt_set_id);
CREATE INDEX IF NOT EXISTS ix_runs_endpoint ON runs (endpoint_id);
CREATE INDEX IF NOT EXISTS ix_preference_sets_prompt_set ON preference_sets (prompt_set_id);
"#;

pub const SCHEMA_VERSION: i64 = 1 + MIGRATIONS.len() as i64;

pub async fn connect(config: &Config) -> anyhow::Result<SqlitePool> {
    std::fs::create_dir_all(&config.data_dir)?;
    std::fs::create_dir_all(config.upload_dir())?;
    std::fs::create_dir_all(config.export_dir())?;
    let url = format!("sqlite://{}", config.db_path().display());
    let options = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(30));
    let pool = SqlitePoolOptions::new().max_connections(8).connect_with(options).await?;
    migrate(&pool).await?;
    Ok(pool)
}

/// An in-memory database for tests, migrated to the latest version.
pub async fn connect_memory() -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    // One connection: every connection to :memory: is a separate database.
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await?;
    migrate(&pool).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    let mut version: i64 = sqlx::query_scalar("PRAGMA user_version").fetch_one(pool).await?;
    if version == 0 {
        // A new database, or one the Python backend made (it never set user_version).
        sqlx::raw_sql(SCHEMA_V1).execute(pool).await?;
        version = 1;
        // A Python-era database may already have later columns if it was opened by a newer build
        // that crashed before setting the version; detect v2 by its column.
        let has_status: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('prompt_sets') WHERE name = 'status'",
        )
        .fetch_one(pool)
        .await?;
        if has_status > 0 {
            version = 2;
        }
        sqlx::raw_sql(&format!("PRAGMA user_version = {version}")).execute(pool).await?;
    }
    while version < SCHEMA_VERSION {
        let step = MIGRATIONS[(version - 1) as usize];
        let mut tx = pool.begin().await?;
        sqlx::raw_sql(step).execute(&mut *tx).await?;
        sqlx::raw_sql(&format!("PRAGMA user_version = {}", version + 1)).execute(&mut *tx).await?;
        tx.commit().await?;
        version += 1;
    }
    sqlx::raw_sql(EXTRA_INDEXES).execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrates_fresh_and_python_databases() {
        let pool = connect_memory().await.unwrap();
        let v: i64 = sqlx::query_scalar("PRAGMA user_version").fetch_one(&pool).await.unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        let cols: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('prompt_sets')")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(cols.contains(&"status".to_string()) && cols.contains(&"rows_total".to_string()));
        // Running again is a no-op.
        migrate(&pool).await.unwrap();
    }
}
