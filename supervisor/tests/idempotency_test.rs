//! Re-applying the same Tinker payload must not double-write metric rows.
//! True idempotency comes from the UNIQUE INDEX on (run_id, step, name) plus
//! ON CONFLICT in `apply_update`.

use std::sync::Arc;

use sqlx::sqlite::SqlitePoolOptions;
use stellarator_supervisor::config::Config;
use stellarator_supervisor::db::init_test_schema;
use stellarator_supervisor::supervisor::{Supervisor, TrackedJob};
use stellarator_supervisor::tinker::{TinkerClient, TinkerJob};
use stellarator_supervisor::ws::Hub;
use tokio_util::sync::CancellationToken;

fn cfg() -> Config {
    Config {
        tinker_api_key: "x".into(),
        tinker_base_url: "http://127.0.0.1:1".into(),
        db_url: ":memory:".into(),
        bind_addr: "0".into(),
        poll_interval_secs: 1,
        cost_h100_per_hour: 4.50,
        cost_a100_per_hour: 2.20,
    }
}

async fn make_db() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    init_test_schema(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO runs (id, owner_agent, name, status, base_model, method, tinker_job_id, gpu_type, gpu_count) \
         VALUES ('11111111-1111-1111-1111-111111111111','claude','t','queued','m','sft','tj1','H100',1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

#[tokio::test]
async fn replaying_same_step_produces_one_row_per_metric() {
    let cfg = Arc::new(cfg());
    let db = make_db().await;
    let tinker = TinkerClient::new("http://x", "k").unwrap();
    let hub = Arc::new(Hub::new());
    let sup = Supervisor::new(cfg, db.clone(), tinker, hub, CancellationToken::new());

    let job = TrackedJob {
        run_id: "11111111-1111-1111-1111-111111111111".into(),
        tinker_job_id: "tj1".into(),
        gpu_type: "H100".into(),
        gpu_count: 1,
    };
    let tj = TinkerJob {
        id: "tj1".into(),
        status: "running".into(),
        gpu_seconds: Some(60.0),
        started_at: None,
        finished_at: None,
        step: Some(3),
        metrics: Some(serde_json::json!({"loss": 0.42, "lr": 0.0001})),
    };

    // Apply the same payload three times.
    for _ in 0..3 {
        sup.apply_for_test(job.clone(), tj.clone()).await.unwrap();
    }

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM run_metrics WHERE run_id='11111111-1111-1111-1111-111111111111'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(count.0, 2, "expected one row per (step, name)");
}

#[tokio::test]
async fn updated_value_for_same_step_is_overwritten() {
    let cfg = Arc::new(cfg());
    let db = make_db().await;
    let tinker = TinkerClient::new("http://x", "k").unwrap();
    let hub = Arc::new(Hub::new());
    let sup = Supervisor::new(cfg, db.clone(), tinker, hub, CancellationToken::new());

    let job = TrackedJob {
        run_id: "11111111-1111-1111-1111-111111111111".into(),
        tinker_job_id: "tj1".into(),
        gpu_type: "H100".into(),
        gpu_count: 1,
    };

    // First payload at step=3.
    let tj1 = TinkerJob {
        id: "tj1".into(),
        status: "running".into(),
        gpu_seconds: Some(60.0),
        started_at: None,
        finished_at: None,
        step: Some(3),
        metrics: Some(serde_json::json!({"loss": 0.9})),
    };
    sup.apply_for_test(job.clone(), tj1).await.unwrap();

    // Second payload at the SAME step with a corrected value.
    // (apply_for_test resets last_step internally so this exercises the
    //  ON CONFLICT path, not the in-memory dedupe.)
    let tj2 = TinkerJob {
        id: "tj1".into(),
        status: "running".into(),
        gpu_seconds: Some(60.0),
        started_at: None,
        finished_at: None,
        step: Some(3),
        metrics: Some(serde_json::json!({"loss": 0.5})),
    };
    sup.apply_for_test(job, tj2).await.unwrap();

    let row: (f64,) = sqlx::query_as(
        "SELECT value FROM run_metrics WHERE run_id='11111111-1111-1111-1111-111111111111' AND name='loss'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!((row.0 - 0.5).abs() < 1e-9, "value should be overwritten to 0.5, got {}", row.0);
}
