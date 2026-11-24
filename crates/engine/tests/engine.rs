//! Run engine against the mock server (in-process, 400 tokens/s per sequence). Ported from
//! backend/tests/test_engine.py.

mod common;

use std::time::{Duration, Instant};

use axum::http::{Method, StatusCode};
use common::{assert_counters_match, dead_url, env, i, s};
use serde_json::json;
use stellarator_core::probes::RunsProbe;
use stellarator_engine::RESTART_REASON;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_of_60_at_concurrency_8() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(30).await;
    let run = e
        .start_run(ep, ps, "mock-reasoning-content", 8, json!({"samples_per_prompt": 2, "seed": 7}))
        .await;
    let rid = i(&run["id"]);
    assert_eq!(i(&run["total"]), 60);
    assert!(i(&run["pending"]) <= 60);
    assert!(e.engine.active_runs().iter().any(|r| r.run_id == rid && r.prompt_set_id == ps));

    let lv = e.wait_live(rid, |x| i(&x["in_flight"]) > 0, 20.0).await;
    assert!(i(&lv["in_flight"]) <= 8);
    let seq = &lv["sequences"][0];
    assert!(s(&seq["prompt_preview"]).starts_with("Question "));
    assert!(["waiting", "thinking", "answering"].contains(&s(&seq["phase"])));
    assert_eq!(i(&seq["attempt"]), 1);

    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!((i(&run["done"]), i(&run["failed"]), i(&run["pending"])), (60, 0, 0));
    assert!(i(&run["completion_tokens"]) > 0 && i(&run["reasoning_tokens"]) > 0);
    assert!(run["active_seconds"].as_f64().unwrap() > 0.0);
    assert!(run["finished_at"].is_string() && run["status_reason"].is_null());
    assert_counters_match(&run, &e.truth(rid).await);
    assert!(e.engine.active_runs().is_empty());

    let page = e.get(&format!("/api/runs/{rid}/samples?limit=5")).await;
    assert_eq!(i(&page["total"]), 60);
    let order: Vec<(i64, i64)> =
        page["items"].as_array().unwrap().iter().map(|x| (i(&x["prompt_idx"]), i(&x["sample_idx"]))).collect();
    assert_eq!(order, [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)]);
    let first = &page["items"][0];
    assert_eq!(first["status"], "done");
    assert_eq!(first["has_reasoning"], true);
    assert!(s(&first["response_preview"]).starts_with("Here is"));

    let sample = e.get(&format!("/api/samples/{}", i(&first["id"]))).await;
    assert!(s(&sample["reasoning"]).starts_with("The question asks"));
    assert!(s(&sample["response"]).starts_with("Here is the answer"));
    assert_eq!(sample["finish_reason"], "stop");
    assert_eq!(i(&sample["attempts"]), 1);
    assert!(i(&sample["latency_ms"]) >= i(&sample["ttft_ms"]));
    assert_eq!(sample["messages"], json!([{"role": "user", "content": "Question 0: what is 0 + 0?"}]));

    let lv = e.live(rid).await;
    assert_eq!(i(&lv["in_flight"]), 0);
    assert_eq!(lv["sequences"], json!([]));
    assert!(lv["eta_seconds"].is_null());
    let metrics = lv["metrics"].as_array().unwrap();
    assert!(metrics.len() >= 2);
    assert_eq!(i(&metrics.last().unwrap()["done"]), 60);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_reasoning_shape() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(6).await;
    let models = ["mock-reasoning", "mock-think-tags", "mock-open-think", "mock-plain"];
    let mut runs = Vec::new();
    for m in models {
        runs.push(i(&e.start_run(ep, ps, m, 6, json!({})).await["id"]));
    }
    for (model, rid) in models.iter().zip(runs) {
        let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
        assert_eq!(i(&run["done"]), 6, "{model}");
        let items = e.get(&format!("/api/runs/{rid}/samples")).await;
        for item in items["items"].as_array().unwrap() {
            let s = e.get(&format!("/api/samples/{}", i(&item["id"]))).await;
            let response = s["response"].as_str().unwrap();
            assert!(response.starts_with("Here is the answer"), "{model}: {response}");
            if *model == "mock-plain" {
                assert!(s["reasoning"].is_null());
            } else {
                let reasoning = s["reasoning"].as_str().unwrap();
                assert!(reasoning.starts_with("The question asks"), "{model}: {reasoning}");
                assert!(!reasoning.contains("think>") && !response.contains("think>"));
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_streaming_run() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(8).await;
    let params = json!({"stream": false, "system_prompt": "Be brief.", "user_template": "Solve: {prompt}"});
    let rid = i(&e.start_run(ep, ps, "mock-reasoning", 8, params).await["id"]);
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!(i(&run["done"]), 8);
    assert!(i(&run["completion_tokens"]) > 0);
    let item = e.get(&format!("/api/runs/{rid}/samples?limit=1")).await["items"][0].clone();
    let s = e.get(&format!("/api/samples/{}", i(&item["id"]))).await;
    assert_eq!(
        s["messages"],
        json!([{"role": "system", "content": "Be brief."}, {"role": "user", "content": "Solve: Question 0: what is 0 + 0?"}])
    );
    assert!(s["reasoning"].is_string() && s["response"].as_str().unwrap().starts_with("Here is the answer"));
    assert!(s["ttft_ms"].is_null() && i(&s["latency_ms"]) > 0); // no first-token time without streaming
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flaky_server_retries() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(60).await;
    let rid = i(&e.start_run(ep, ps, "mock-flaky", 16, json!({"max_attempts": 3, "pause_after_failures": 0})).await["id"]);
    // Pause while some samples wait to retry: they go back to pending and must not be lost.
    e.wait_live(rid, |x| x["sequences"].as_array().unwrap().iter().any(|s| i(&s["attempt"]) > 1), 30.0).await;
    e.post(&format!("/api/runs/{rid}/pause"), json!({})).await;
    e.wait_for(rid, |x| x["status"] == "paused", 20.0).await;
    assert_eq!(e.truth(rid).await.running, 0);
    let (status, _) = e.post(&format!("/api/runs/{rid}/resume"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let run = e.wait_for(rid, |x| x["status"] == "completed", 90.0).await;
    let t = e.truth(rid).await;
    assert_eq!((t.pending, t.running), (0, 0));
    assert_eq!(i(&run["done"]) + i(&run["failed"]), 60);
    assert_eq!(i(&run["pending"]), 0);
    assert_counters_match(&run, &t);
    let retried = e.count("SELECT COUNT(*) FROM samples WHERE run_id = ? AND attempts > 1", rid).await;
    assert!(retried > 0); // 20% of requests fail, so some samples needed a second attempt
    let failed = e.get(&format!("/api/runs/{rid}/samples?status=failed")).await;
    for item in failed["items"].as_array().unwrap() {
        assert!(s(&item["error"]).starts_with("The server answered"), "{}", item["error"]);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_failed() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(60).await;
    let rid = i(&e.start_run(ep, ps, "mock-flaky", 12, json!({"max_attempts": 1, "pause_after_failures": 0})).await["id"]);
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert!(i(&run["failed"]) > 0 && i(&run["done"]) + i(&run["failed"]) == 60, "{run}");
    let (status, after) = e.post(&format!("/api/runs/{rid}/retry-failed"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["status"], "running");
    assert_eq!(i(&after["failed"]), 0);
    assert!(after["finished_at"].is_null());
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!(i(&run["done"]) + i(&run["failed"]), 60);
    assert_counters_match(&run, &e.truth(rid).await);
    // Nothing failed: retrying again changes nothing.
    if i(&run["failed"]) == 0 {
        let (status, again) = e.post(&format!("/api/runs/{rid}/retry-failed"), json!({})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(again["status"], "completed");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn truncating_counts_truncated() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(16).await;
    let rid = i(&e.start_run(ep, ps, "mock-truncating", 8, json!({})).await["id"]);
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!((i(&run["done"]), i(&run["truncated"])), (16, 16));
    assert_counters_match(&run, &e.truth(rid).await);
    let page = e.get(&format!("/api/runs/{rid}/samples?status=truncated")).await;
    assert_eq!(i(&page["total"]), 16);
    assert!(page["items"].as_array().unwrap().iter().all(|x| x["finish_reason"] == "length"));
    let s = e.get(&format!("/api/samples/{}", i(&page["items"][0]["id"]))).await;
    assert_eq!(s["response"], ""); // cut off while thinking
    assert!(s["reasoning"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrency_changes_live_and_delete_while_running() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(400).await;
    let rid = i(&e.start_run(ep, ps, "mock-reasoning", 3, json!({})).await["id"]);
    e.wait_live(rid, |x| i(&x["in_flight"]) == 3, 20.0).await;
    let (status, r) = e.call(Method::PATCH, &format!("/api/runs/{rid}"), Some(json!({"concurrency": 24}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(i(&r["concurrency"]), 24);
    let lv = e.wait_live(rid, |x| i(&x["in_flight"]) >= 20, 5.0).await;
    assert!(i(&lv["in_flight"]) <= 24);
    assert_eq!(lv["sequences"].as_array().unwrap().len() as i64, i(&lv["in_flight"]));
    let started: Vec<i64> = lv["sequences"].as_array().unwrap().iter().map(|s| i(&s["elapsed_ms"])).collect();
    let mut sorted = started.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(started, sorted); // oldest first
    e.call(Method::PATCH, &format!("/api/runs/{rid}"), Some(json!({"concurrency": 2}))).await;
    e.wait_live(rid, |x| i(&x["in_flight"]) <= 2, 10.0).await;
    // It stays at the lower setting.
    for _ in 0..10 {
        assert!(i(&e.live(rid).await["in_flight"]) <= 2);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (status, _) = e.call(Method::DELETE, &format!("/api/runs/{rid}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(e.engine.active_runs().is_empty());
    assert_eq!(e.call(Method::GET, &format!("/api/runs/{rid}"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(e.count("SELECT COUNT(*) FROM samples WHERE run_id = ?", rid).await, 0);
    assert_eq!(e.count("SELECT COUNT(*) FROM run_metrics WHERE run_id = ?", rid).await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_then_resume() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(80).await;
    let rid = i(&e.start_run(ep, ps, "mock-reasoning-content", 16, json!({})).await["id"]);
    e.wait_for(rid, |x| i(&x["done"]) >= 8, 60.0).await;
    let (status, r) = e.post(&format!("/api/runs/{rid}/pause"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(r["status"] == "pausing" || r["status"] == "paused", "{r}");
    let run = e.wait_for(rid, |x| x["status"] == "paused", 20.0).await;
    assert!(run["status_reason"].is_null());
    let t = e.truth(rid).await;
    assert_eq!(t.running, 0);
    assert!(t.pending > 0);
    assert_eq!(i(&run["pending"]), t.pending);
    assert_counters_match(&run, &t);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(e.run(rid).await["done"], run["done"]); // nothing moves while paused
    let (_, again) = e.post(&format!("/api/runs/{rid}/pause"), json!({})).await;
    assert_eq!(again["status"], "paused"); // idempotent

    let (status, r) = e.post(&format!("/api/runs/{rid}/resume"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(r["status"], "running");
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!((i(&run["done"]), i(&run["pending"])), (80, 0));
    assert_counters_match(&run, &e.truth(rid).await);
    let (status, r) = e.post(&format!("/api/runs/{rid}/resume"), json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(s(&r["detail"]).contains("finished"));
    let (status, _) = e.post(&format!("/api/runs/{rid}/pause"), json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_while_pausing_can_resume() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(40).await;
    let rid = i(&e.start_run(ep, ps, "mock-reasoning", 8, json!({})).await["id"]);
    e.wait_live(rid, |x| i(&x["in_flight"]) >= 4, 20.0).await;
    assert_eq!(e.post(&format!("/api/runs/{rid}/pause"), json!({})).await.1["status"], "pausing");
    let (_, r) = e.post(&format!("/api/runs/{rid}/resume"), json!({})).await; // changed our mind before it settled
    assert_eq!(r["status"], "running");
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!(i(&run["done"]), 40);
    assert_counters_match(&run, &e.truth(rid).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recover_after_crash() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(120).await;
    let rid = i(&e.start_run(ep, ps, "mock-reasoning", 16, json!({})).await["id"]);
    e.wait_for(rid, |x| i(&x["done"]) >= 10, 60.0).await;

    // Crash: the writer dies with unsaved work and the run task vanishes without cleanup.
    e.engine.crash_for_tests().await;
    // Make sure some rows are left mid-flight.
    sqlx::query(
        "UPDATE samples SET status = 'running' WHERE id IN \
         (SELECT id FROM samples WHERE run_id = ? AND status = 'pending' LIMIT 3)",
    )
    .bind(rid)
    .execute(&e.db)
    .await
    .unwrap();
    let before = e.truth(rid).await;
    assert!(before.running >= 3);
    assert_eq!(e.run(rid).await["status"], "running");

    e.engine.recover().await.unwrap();
    let run = e.run(rid).await;
    assert_eq!(run["status"], "paused");
    assert_eq!(run["status_reason"], RESTART_REASON);
    let t = e.truth(rid).await;
    assert_eq!(t.running, 0);
    assert_eq!(t.pending, 120 - t.done - t.failed);
    assert_counters_match(&run, &t);

    let (status, r) = e.post(&format!("/api/runs/{rid}/resume"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(r["status_reason"].is_null());
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!(i(&run["done"]), 120);
    assert_counters_match(&run, &e.truth(rid).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_returns_in_flight_to_pending() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(60).await;
    let rid = i(&e.start_run(ep, ps, "mock-reasoning", 8, json!({})).await["id"]);
    e.wait_live(rid, |x| i(&x["in_flight"]) == 8, 20.0).await;
    e.engine.shutdown().await;
    let t = e.truth(rid).await;
    assert_eq!(t.running, 0);
    assert_eq!(t.done + t.pending, 60);
    let (status, active): (String, f64) =
        sqlx::query_as("SELECT status, active_seconds FROM runs WHERE id = ?").bind(rid).fetch_one(&e.db).await.unwrap();
    assert_eq!(status, "running");
    assert!(active > 0.0);
    e.engine.recover().await.unwrap();
    let run = e.run(rid).await;
    assert_eq!(run["status"], "paused");
    assert_eq!(run["status_reason"], RESTART_REASON);
    assert_counters_match(&run, &e.truth(rid).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn circuit_breaker_and_deleted_endpoint() {
    let e = env().await;
    let ep = e.make_endpoint(&dead_url(), "Dead").await;
    let ps = e.make_prompt_set(50).await;
    let rid = i(&e.start_run(ep, ps, "any-model", 2, json!({"max_attempts": 1, "pause_after_failures": 3})).await["id"]);
    let run = e.wait_for(rid, |x| x["status"] == "paused", 20.0).await;
    let reason = s(&run["status_reason"]);
    assert!(reason.starts_with("Paused after 3 failures in a row. Last error: Could not connect."), "{reason}");
    assert!((3..=4).contains(&i(&run["failed"])), "{run}");
    assert_eq!(i(&run["done"]), 0);
    assert_counters_match(&run, &e.truth(rid).await);

    let (status, r) = e.post(&format!("/api/runs/{rid}/resume"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    // Resuming clears the reason. A dead endpoint refuses at once, so the run can be paused again
    // by the time the answer is read; then the reason is the new pause's.
    assert!(r["status_reason"].is_null() || r["status"] == "paused", "{r}");
    let run = e.wait_for(rid, |x| x["status"] == "paused", 20.0).await;
    assert!(s(&run["status_reason"]).starts_with("Paused after 3 failures in a row."));

    // Once its endpoint is gone the run cannot continue.
    sqlx::query("DELETE FROM endpoints WHERE id = ?").bind(ep).execute(&e.db).await.unwrap();
    let (status, r) = e.post(&format!("/api/runs/{rid}/resume"), json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(s(&r["detail"]).contains("endpoint"));
    let (status, _) = e.post(&format!("/api/runs/{rid}/retry-failed"), json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let run = e.run(rid).await;
    assert!(run["endpoint_id"].is_null());
    assert_eq!(run["endpoint_name"], "Dead");
    assert_eq!(run["status"], "paused");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn load_2000_samples_at_256() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set(1000).await;
    let t0 = Instant::now();
    let rid = i(&e.start_run(ep, ps, "mock-reasoning-content", 256, json!({"samples_per_prompt": 2})).await["id"]);
    let mut peak = 0;
    loop {
        let lv = e.live(rid).await;
        peak = peak.max(i(&lv["in_flight"]));
        if lv["run"]["status"] == "completed" {
            break;
        }
        assert!(t0.elapsed() < Duration::from_secs(120), "{}", lv["run"]);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let run = e.run(rid).await;
    let t = e.truth(rid).await;
    assert_eq!((i(&run["done"]), i(&run["failed"]), t.pending, t.running), (2000, 0, 0, 0));
    assert_counters_match(&run, &t);
    assert!(peak > 128, "peak in flight {peak}");
    eprintln!("2,000 samples at 256 seqs: {:.1} s, peak in flight {peak}", t0.elapsed().as_secs_f64());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_does_not_wait_for_retry_backoff() {
    let e = env().await;
    let ep = e.make_endpoint(&dead_url(), "Down").await;
    let ps = e.make_prompt_set(20).await;
    let rid = i(&e.start_run(ep, ps, "any-model", 4, json!({"max_attempts": 10, "pause_after_failures": 0})).await["id"]);
    // Every request fails to connect, so all four sit in backoff between attempts.
    let lv = e
        .wait_live(
            rid,
            |x| {
                let seqs = x["sequences"].as_array().unwrap();
                i(&x["in_flight"]) == 4 && seqs.iter().all(|s| i(&s["attempt"]) >= 2)
            },
            20.0,
        )
        .await;
    assert!(lv["sequences"].as_array().unwrap().iter().all(|s| s["phase"] == "waiting"));
    let t0 = Instant::now();
    e.post(&format!("/api/runs/{rid}/pause"), json!({})).await;
    let run = e.wait_for(rid, |x| x["status"] == "paused", 5.0).await;
    assert!(t0.elapsed() < Duration::from_millis(1500));
    assert_eq!((i(&run["failed"]), i(&run["pending"])), (0, 20)); // waiting samples went back to pending
    let t = e.truth(rid).await;
    assert_eq!((t.running, t.pending), (0, 20));
}
