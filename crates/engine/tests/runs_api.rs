//! Runs API: validation, listing, patching, samples browsing, and a 100k-sample run. Ported from
//! backend/tests/test_runs_api.py.

mod common;

use std::time::{Duration, Instant};

use axum::http::{Method, StatusCode};
use common::{dead_url, env, i, s};
use serde_json::{json, Value};
use stellarator_core::time;

#[tokio::test]
async fn create_validation() {
    let e = env().await;
    let ep = e.make_endpoint(&dead_url(), "Nowhere").await;
    let ps = e.make_prompt_set(3).await;
    let empty = e.make_prompt_set_with(0, "Empty", "ready", |_| json!([])).await;
    let importing =
        e.make_prompt_set_with(2, "Importing", "importing", |_| json!([{"role": "user", "content": "x"}])).await;
    let broken = e.make_prompt_set_with(0, "Broken", "failed", |_| json!([])).await;

    let post = |body: Value| e.post("/api/runs", body);
    let (st, r) = post(json!({"endpoint_id": 999999, "model": "m", "prompt_set_id": ps})).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(s(&r["fields"]["endpoint_id"]).contains("endpoint"));
    let (st, r) = post(json!({"endpoint_id": ep, "model": "m", "prompt_set_id": 999999})).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(s(&r["fields"]["prompt_set_id"]).contains("prompt set"));
    let (st, r) = post(json!({"endpoint_id": ep, "model": "m", "prompt_set_id": empty})).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(s(&r["fields"]["prompt_set_id"]).contains("no prompts"));
    let (st, r) = post(json!({"endpoint_id": ep, "model": "m", "prompt_set_id": importing})).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r["fields"]["prompt_set_id"], "This prompt set is still importing.");
    assert_eq!(r["detail"], "This prompt set is still importing.");
    let (st, r) = post(json!({"endpoint_id": ep, "model": "m", "prompt_set_id": broken})).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r["fields"]["prompt_set_id"], "This prompt set failed to import.");
    let (st, r) =
        post(json!({"endpoint_id": ep, "model": "m", "prompt_set_id": ps, "params": {"user_template": "no slot"}})).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(r["fields"].get("params.user_template").is_some(), "{r}");
    let (st, r) = post(json!({"endpoint_id": ep, "model": "m", "prompt_set_id": ps, "concurrency": 0})).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(r["fields"].get("concurrency").is_some(), "{r}");
    let (st, r) = post(json!({"endpoint_id": ep, "prompt_set_id": ps})).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r["fields"]["model"], "Field required.");
    // Nothing was created by the refusals.
    assert_eq!(e.get("/api/runs").await, json!([]));
}

#[tokio::test]
async fn not_found() {
    let e = env().await;
    let cases = [
        (Method::GET, "/api/runs/999999"),
        (Method::PATCH, "/api/runs/999999"),
        (Method::DELETE, "/api/runs/999999"),
        (Method::POST, "/api/runs/999999/pause"),
        (Method::POST, "/api/runs/999999/resume"),
        (Method::POST, "/api/runs/999999/retry-failed"),
        (Method::GET, "/api/runs/999999/live"),
        (Method::GET, "/api/runs/999999/samples"),
        (Method::GET, "/api/samples/999999"),
    ];
    for (method, path) in cases {
        let body = (method == Method::PATCH).then(|| json!({"name": "x"}));
        let (status, r) = e.call(method.clone(), path, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
        assert!(r["detail"].is_string());
    }
    let (status, _) = e.call(Method::GET, "/api/runs/abc", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_patch_and_shape() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = e.make_prompt_set_with(4, "Four prompts", "ready", |k| json!([{"role": "user", "content": format!("Q{k}")}])).await;
    let params = json!({"samples_per_prompt": 2, "temperature": 0.2, "seed": 1, "extra_body": {"top_k": 20}});
    let (_, a) = e.post("/api/runs", json!({"endpoint_id": ep, "model": "mock-plain", "prompt_set_id": ps, "params": params})).await;
    let (_, b) = e
        .post("/api/runs", json!({"name": "Second", "endpoint_id": ep, "model": "mock-plain", "prompt_set_id": ps, "concurrency": 2}))
        .await;
    assert_eq!(b["name"], "Second");
    assert_eq!(a["name"], "mock-plain on Four prompts");
    assert_eq!(i(&b["total"]), 4);
    assert_eq!(a["params"]["extra_body"], json!({"top_k": 20}));
    assert_eq!(i(&a["params"]["samples_per_prompt"]), 2);
    assert_eq!(a["params"]["temperature"], 0.2);
    assert_eq!(a["endpoint_name"], "Mock");
    assert_eq!(a["prompt_set_name"], "Four prompts");
    assert_eq!(i(&a["total"]), 8);
    for key in ["created_at", "started_at"] {
        assert!(s(&a[key]).ends_with('Z'), "{}", a[key]);
    }

    let runs = e.get("/api/runs").await;
    let ids: Vec<i64> = runs.as_array().unwrap().iter().map(|r| i(&r["id"])).collect();
    let pos = |id: i64| ids.iter().position(|x| *x == id).unwrap();
    assert!(pos(i(&b["id"])) < pos(i(&a["id"]))); // newest first

    let aid = i(&a["id"]);
    let (st, r) = e.call(Method::PATCH, &format!("/api/runs/{aid}"), Some(json!({"name": "Renamed", "concurrency": 5}))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(r["name"], "Renamed");
    assert_eq!(i(&r["concurrency"]), 5);
    assert!(e.engine.concurrency(aid).is_none_or(|c| c == 5));
    let (st, _) = e.call(Method::PATCH, &format!("/api/runs/{aid}"), Some(json!({"concurrency": 0}))).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, r) = e.call(Method::PATCH, &format!("/api/runs/{aid}"), Some(json!({"name": "   "}))).await;
    assert_eq!(r["name"], "Renamed"); // a blank name keeps the old one

    for run in [&a, &b] {
        let done = e.wait_for(i(&run["id"]), |x| x["status"] == "completed", 60.0).await;
        assert_eq!(done["done"], run["total"]);
        assert!(done["finished_at"].is_string());
    }
    let (_, r) = e.call(Method::PATCH, &format!("/api/runs/{}", i(&b["id"])), Some(json!({"concurrency": 7}))).await;
    assert_eq!(i(&r["concurrency"]), 7);
    assert_eq!(r["status"], "completed");

    // The request carried the run's parameters.
    let sample = e.get(&format!("/api/runs/{aid}/samples?limit=1")).await["items"][0].clone();
    assert_eq!(sample["status"], "done");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn samples_filters_and_search() {
    let e = env().await;
    let long_prompt = format!("Explain {}carefully.", "very ".repeat(100));
    let texts = ["Alpha 100% sure?".to_string(), "beta_value test".into(), "Gamma ray burst".into(), long_prompt.clone(), "Café crème".into()];
    let ps = e
        .make_prompt_set_with(5, "Search", "ready", |k| {
            json!([{"role": "system", "content": "sys"}, {"role": "user", "content": texts[k as usize]}])
        })
        .await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let (_, run) = e
        .post("/api/runs", json!({"endpoint_id": ep, "model": "mock-truncating", "prompt_set_id": ps, "params": {"samples_per_prompt": 2}}))
        .await;
    let rid = i(&run["id"]);
    e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;

    let page = |q: &'static str| {
        let e = &e;
        async move {
            let (st, v) = e.call(Method::GET, &format!("/api/runs/{rid}/samples{q}"), None).await;
            assert_eq!(st, StatusCode::OK, "{q}: {v}");
            v
        }
    };
    let p = page("?limit=3&offset=2").await;
    assert_eq!((i(&p["total"]), i(&p["offset"]), i(&p["limit"])), (10, 2, 3));
    let order: Vec<(i64, i64)> =
        p["items"].as_array().unwrap().iter().map(|x| (i(&x["prompt_idx"]), i(&x["sample_idx"]))).collect();
    assert_eq!(order, [(1, 0), (1, 1), (2, 0)]);
    assert_eq!(i(&page("?status=done").await["total"]), 10);
    assert_eq!(i(&page("?status=truncated").await["total"]), 10);
    assert_eq!(i(&page("?status=failed").await["total"]), 0);
    assert_eq!(i(&page("?status=pending").await["total"]), 0);
    for bad in ["?status=bogus", "?offset=-1", "?limit=0", "?limit=501", "?limit=abc"] {
        let (st, _) = e.call(Method::GET, &format!("/api/runs/{rid}/samples{bad}"), None).await;
        assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }

    assert_eq!(i(&page("?q=gamma").await["total"]), 2); // case-insensitive, prompt text
    assert_eq!(i(&page("?q=100%25").await["total"]), 2); // a literal %
    assert_eq!(i(&page("?q=beta_value").await["total"]), 2);
    assert_eq!(i(&page("?q=_").await["total"]), 2); // a literal _, not a wildcard
    assert_eq!(i(&page("?q=caf%C3%A9").await["total"]), 2); // non-ASCII
    assert_eq!(i(&page("?q=sys").await["total"]), 10); // the system message is part of the prompt
    assert_eq!(i(&page("?q=nothing-like-this").await["total"]), 0);
    assert_eq!(i(&page("?q=gamma&status=failed").await["total"]), 0);

    let found = page("?q=carefully").await;
    let item = &found["items"][0];
    let preview = s(&item["prompt_preview"]);
    assert!(preview.chars().count() <= 160 && preview.starts_with("Explain very"), "{preview}");
    assert_eq!(item["finish_reason"], "length");
    assert_eq!(item["has_reasoning"], true);
    assert_eq!(item["response_preview"], ""); // truncated while thinking

    let full = e.get(&format!("/api/samples/{}", i(&item["id"]))).await;
    assert_eq!(full["messages"], json!([{"role": "system", "content": "sys"}, {"role": "user", "content": long_prompt}]));
    assert!(s(&full["completed_at"]).ends_with('Z'));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn search_in_response() {
    let e = env().await;
    let ps = e.make_prompt_set(3).await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let (_, run) = e.post("/api/runs", json!({"endpoint_id": ep, "model": "mock-plain", "prompt_set_id": ps})).await;
    let rid = i(&run["id"]);
    e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    let r = e.get(&format!("/api/runs/{rid}/samples?q=final%20answer")).await;
    assert_eq!(i(&r["total"]), 3); // "Final answer: N." is in every response
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn large_run_is_created_quickly() {
    let e = env().await;
    let ep = e.make_endpoint(&dead_url(), "Nowhere").await;
    let ps = e.make_prompt_set(50_000).await;
    let t0 = Instant::now();
    let (st, run) = e
        .post(
            "/api/runs",
            json!({"endpoint_id": ep, "model": "m", "prompt_set_id": ps, "concurrency": 1,
                   "params": {"samples_per_prompt": 2, "max_attempts": 1, "pause_after_failures": 0}}),
        )
        .await;
    let elapsed = t0.elapsed();
    assert_eq!(st, StatusCode::CREATED, "{run}");
    assert_eq!(i(&run["total"]), 100_000);
    assert!(elapsed < Duration::from_secs(10), "creating 100,000 samples took {elapsed:?}");
    eprintln!("created 100,000 samples in {elapsed:?}");
    let rid = i(&run["id"]);
    let (st, _) = e.post(&format!("/api/runs/{rid}/pause"), json!({})).await;
    assert_eq!(st, StatusCode::OK);
    let p = e.get(&format!("/api/runs/{rid}/samples?offset=99990&limit=20")).await;
    assert_eq!(i(&p["total"]), 100_000);
    let items = p["items"].as_array().unwrap();
    assert_eq!(items.len(), 10);
    let last = items.last().unwrap();
    assert_eq!((i(&last["prompt_idx"]), i(&last["sample_idx"])), (49_999, 1));
    // Ids follow prompt order.
    let out_of_order: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM samples a JOIN samples b ON b.id = a.id + 1 \
         WHERE a.run_id = ? AND b.run_id = a.run_id AND (b.prompt_idx, b.sample_idx) < (a.prompt_idx, a.sample_idx)",
    )
    .bind(rid)
    .fetch_one(&e.db)
    .await
    .unwrap();
    assert_eq!(out_of_order, 0);
    let (st, _) = e.call(Method::DELETE, &format!("/api/runs/{rid}"), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert_eq!(e.count("SELECT COUNT(*) FROM samples WHERE run_id = ?", rid).await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_metrics_are_downsampled() {
    let e = env().await;
    let ep = e.make_endpoint(&dead_url(), "Nowhere").await;
    let ps = e.make_prompt_set(5).await;
    let (_, run) = e
        .post("/api/runs", json!({"endpoint_id": ep, "model": "m", "prompt_set_id": ps,
                                  "params": {"max_attempts": 1, "pause_after_failures": 1}}))
        .await;
    let rid = i(&run["id"]);
    e.wait_for(rid, |x| x["status"] == "paused", 20.0).await;
    let t0 = chrono::Utc::now() + chrono::Duration::minutes(1); // after the points the engine wrote
    let mut tx = e.db.begin().await.unwrap();
    for k in 0..1000i64 {
        sqlx::query(
            "INSERT INTO run_metrics (run_id, t, tokens_per_s, samples_per_min, active, concurrency, done, failed) \
             VALUES (?, ?, ?, 1.0, 1, 16, ?, 0)",
        )
        .bind(rid)
        .bind(time::to_db(t0 + chrono::Duration::seconds(5 * k)))
        .bind(k as f64)
        .bind(k)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let lv = e.live(rid).await;
    assert_eq!(i(&lv["in_flight"]), 0);
    assert_eq!(lv["sequences"], json!([]));
    assert!(lv["eta_seconds"].is_null());
    let points = lv["metrics"].as_array().unwrap();
    assert!((200..=240).contains(&points.len()), "{}", points.len());
    let times: Vec<&str> = points.iter().map(|p| s(&p["t"])).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted);
    assert_eq!(i(&points.last().unwrap()["done"]), 999); // the latest point is always kept
    let (st, _) = e.call(Method::DELETE, &format!("/api/runs/{rid}"), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn metric_downsampling_matches_python() {
    // Python: rows[::stride]; the last row replaces the final pick at 240 points, else is appended.
    let e = env().await;
    let ep = e.make_endpoint(&dead_url(), "Nowhere").await;
    let ps = e.make_prompt_set(1).await;
    let (_, run) = e
        .post("/api/runs", json!({"endpoint_id": ep, "model": "m", "prompt_set_id": ps,
                                  "params": {"max_attempts": 1, "pause_after_failures": 1}}))
        .await;
    let rid = i(&run["id"]);
    e.wait_for(rid, |x| x["status"] != "running" && x["status"] != "pausing", 20.0).await;
    sqlx::query("DELETE FROM run_metrics WHERE run_id = ?").bind(rid).execute(&e.db).await.unwrap();
    let base = chrono::Utc::now();
    let mut inserted = 0i64;
    for n in [5i64, 240, 241, 480, 481, 1000] {
        while inserted < n {
            sqlx::query(
                "INSERT INTO run_metrics (run_id, t, tokens_per_s, samples_per_min, active, concurrency, done, failed) \
                 VALUES (?, ?, 0.0, 0.0, 0, 1, ?, 0)",
            )
            .bind(rid)
            .bind(time::to_db(base + chrono::Duration::seconds(inserted)))
            .bind(inserted)
            .execute(&e.db)
            .await
            .unwrap();
            inserted += 1;
        }
        let rows: Vec<i64> = (0..n).collect();
        let want: Vec<i64> = if rows.len() > 240 {
            let stride = rows.len().div_ceil(240);
            let mut picked: Vec<i64> = rows.iter().step_by(stride).copied().collect();
            let last = *rows.last().unwrap();
            if *picked.last().unwrap() != last {
                if picked.len() >= 240 {
                    *picked.last_mut().unwrap() = last;
                } else {
                    picked.push(last);
                }
            }
            picked
        } else {
            rows
        };
        let got: Vec<i64> = e.live(rid).await["metrics"].as_array().unwrap().iter().map(|p| i(&p["done"])).collect();
        assert_eq!(got, want, "n = {n}");
    }
}
