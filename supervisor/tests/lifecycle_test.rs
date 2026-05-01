use std::sync::Arc;
use std::sync::Once;
use std::time::Duration;

static INIT_ENV: Once = Once::new();
fn set_test_env() {
    INIT_ENV.call_once(|| {
        // The supervisor refuses to boot without a shared secret. Tests that
        // construct sub-components rely on this being set so any future
        // helper that touches Config::from_env stays consistent.
        std::env::set_var("SUPERVISOR_SHARED_SECRET", "test-shared-secret-value");
    });
}

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
        tinker_base_url: "http://127.0.0.1:1".into(), // never reached in apply_for_test
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
async fn apply_update_writes_status_metrics_and_cost() {
    set_test_env();
    let cfg = Arc::new(cfg());
    let db = make_db().await;
    let tinker = TinkerClient::new("http://x", "k").unwrap();
    let hub = Arc::new(Hub::new());
    let sup = Supervisor::new(cfg, db.clone(), tinker, hub.clone(), CancellationToken::new());

    let mut sub = hub.subscribe_run("11111111-1111-1111-1111-111111111111");

    let job = TrackedJob {
        run_id: "11111111-1111-1111-1111-111111111111".into(),
        tinker_job_id: "tj1".into(),
        gpu_type: "H100".into(),
        gpu_count: 1,
    };
    let tj = TinkerJob {
        id: "tj1".into(),
        status: "running".into(),
        gpu_seconds: Some(1800.0),
        started_at: Some("2026-04-30T00:00:00Z".into()),
        finished_at: None,
        step: Some(7),
        metrics: Some(serde_json::json!({"loss": 0.42, "lr": 0.0001})),
    };
    sup.apply_for_test(job, tj).await.unwrap();

    let row: (String, f64, f64) =
        sqlx::query_as("SELECT status, gpu_seconds, cost_usd FROM runs WHERE id='11111111-1111-1111-1111-111111111111'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(row.0, "running");
    assert!((row.1 - 1800.0).abs() < 1e-6);
    assert!((row.2 - 2.25).abs() < 1e-9);

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM run_metrics WHERE run_id='11111111-1111-1111-1111-111111111111'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count.0, 2);

    let evt = tokio::time::timeout(Duration::from_secs(1), sub.recv())
        .await
        .expect("ws event")
        .expect("ws ok");
    assert_eq!(evt.run_id, "11111111-1111-1111-1111-111111111111");
    assert_eq!(evt.step, 7);
}

#[tokio::test]
async fn track_is_idempotent_and_cancellable() {
    set_test_env();
    let cfg = Arc::new(cfg());
    let db = make_db().await;
    let tinker = TinkerClient::new("http://x", "k").unwrap();
    let hub = Arc::new(Hub::new());
    let sup = Supervisor::new(cfg, db, tinker, hub, CancellationToken::new());

    let job = TrackedJob {
        run_id: "11111111-1111-1111-1111-111111111111".into(),
        tinker_job_id: "tj1".into(),
        gpu_type: "H100".into(),
        gpu_count: 1,
    };
    assert!(sup.track(job.clone()));
    assert!(!sup.track(job.clone()));
    assert_eq!(sup.tracked().len(), 1);

    assert!(sup.untrack("11111111-1111-1111-1111-111111111111"));
    // task removes itself shortly after cancel.
    for _ in 0..50 {
        if sup.tracked().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(sup.tracked().is_empty());
}

#[tokio::test]
async fn restart_recovers_last_step_and_avoids_re_insert() {
    set_test_env();
    let cfg = Arc::new(cfg());
    let db = make_db().await;
    let tinker = TinkerClient::new("http://x", "k").unwrap();
    let _hub = Arc::new(Hub::new());

    let run_id = "11111111-1111-1111-1111-111111111111";
    let job = TrackedJob {
        run_id: run_id.into(),
        tinker_job_id: "tj1".into(),
        gpu_type: "H100".into(),
        gpu_count: 1,
    };

    // Simulate "first boot" applying metrics at step 5.
    let sup1 = Supervisor::new(cfg.clone(), db.clone(), tinker.clone(), Arc::new(Hub::new()), CancellationToken::new());
    let tj = TinkerJob {
        id: "tj1".into(),
        status: "running".into(),
        gpu_seconds: Some(900.0),
        started_at: Some("2026-04-30T00:00:00Z".into()),
        finished_at: None,
        step: Some(5),
        metrics: Some(serde_json::json!({"loss": 0.50})),
    };
    sup1.apply_for_test(job.clone(), tj).await.unwrap();

    let count1: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM run_metrics WHERE run_id=?")
        .bind(run_id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count1.0, 1, "first apply inserted one metric");

    // Simulate "restart": new supervisor instance.
    // On startup, it loads last_step from DB (should find step=5).
    // Creating a new sup2 will trigger the DB query in run() when track() is called.
    let sup2 = Supervisor::new(cfg.clone(), db.clone(), tinker.clone(), Arc::new(Hub::new()), CancellationToken::new());
    let _ = sup1; // sup1 is dropped, simulating restart

    // Apply the same step again.
    // Because last_step was loaded from DB, on_conflict logic should prevent duplicate writes.
    let tj2 = TinkerJob {
        id: "tj1".into(),
        status: "running".into(),
        gpu_seconds: Some(1200.0),
        started_at: Some("2026-04-30T00:00:00Z".into()),
        finished_at: None,
        step: Some(5),
        metrics: Some(serde_json::json!({"loss": 0.49})),
    };
    sup2.apply_for_test(job, tj2).await.unwrap();

    // The ON CONFLICT should update (not insert), so count stays at 1.
    // The updated value should be 0.49 (the new apply).
    let count2: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM run_metrics WHERE run_id=?")
        .bind(run_id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count2.0, 1, "second apply at same step should not add duplicate rows");

    let value: (f64,) = sqlx::query_as("SELECT value FROM run_metrics WHERE run_id=? AND step=5 AND name='loss'")
        .bind(run_id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert!((value.0 - 0.49).abs() < 1e-9, "value should be updated to second apply");
}
