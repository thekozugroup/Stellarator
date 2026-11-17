//! Ported from backend/tests/test_preferences_api.py.

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::{json, Value};

/// A prompt set of n prompts; run A and run B each answered every prompt once.
struct Two {
    ps: i64,
    prompts: Vec<i64>,
    run_a: i64,
    run_b: i64,
    a: Vec<i64>,
    b: Vec<i64>,
}

async fn two_runs(env: &Env, n: i64) -> Two {
    let (ps, prompts) = env.prompt_set("Math word problems", n).await;
    let run_a = env.run(ps, "Qwen3 on math", "Qwen/Qwen3-32B").await;
    let run_b = env.run(ps, "Llama on math", "meta-llama/Llama-3.1-8B-Instruct").await;
    let mut a = Vec::new();
    let mut b = Vec::new();
    for p in &prompts {
        a.push(env.done(run_a, *p, 0).await);
    }
    for p in &prompts {
        b.push(env.done(run_b, *p, 0).await);
    }
    Two { ps, prompts, run_a, run_b, a, b }
}

async fn create(env: &Env, name: &str, run_ids: &[i64]) -> Value {
    let (status, body) = env.post("/api/preference-sets", json!({"name": name, "run_ids": run_ids})).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

fn detail(v: &Value) -> &str {
    v["detail"].as_str().unwrap()
}

#[tokio::test]
async fn create_returns_summary() {
    let env = env().await;
    let t = two_runs(&env, 3).await;
    let out = create(&env, "  Qwen vs Llama  ", &[t.run_a, t.run_b, t.run_a]).await;
    assert_eq!(out["name"], "Qwen vs Llama");
    assert_eq!(out["prompt_set_id"], t.ps);
    assert_eq!(out["prompt_set_name"], "Math word problems");
    assert_eq!(out["run_ids"], json!([t.run_a, t.run_b]));
    assert_eq!(
        out["runs"],
        json!([
            {"id": t.run_a, "name": "Qwen3 on math", "model": "Qwen/Qwen3-32B"},
            {"id": t.run_b, "name": "Llama on math", "model": "meta-llama/Llama-3.1-8B-Instruct"},
        ])
    );
    assert_eq!(out["prompt_count"], 3);
    assert_eq!(out["decided"], 0);
    assert!(out["created_at"].as_str().unwrap().ends_with('Z'));
    let (status, got) = env.get(&format!("/api/preference-sets/{}", out["id"])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got, out);
}

#[tokio::test]
async fn create_rejects_runs_on_different_prompt_sets() {
    let env = env().await;
    let (ps1, _) = env.prompt_set("One", 1).await;
    let (ps2, _) = env.prompt_set("Two", 1).await;
    let r1 = env.run(ps1, "r1", "m1").await;
    let r2 = env.run(ps2, "r2", "m2").await;
    let (status, body) = env.post("/api/preference-sets", json!({"name": "Mixed", "run_ids": [r1, r2]})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail(&body), "All runs must use the same prompt set.");
}

#[tokio::test]
async fn create_rejects_missing_run_and_bad_name() {
    let env = env().await;
    let (ps, _) = env.prompt_set("One", 1).await;
    let r1 = env.run(ps, "r1", "m1").await;

    let (status, body) = env.post("/api/preference-sets", json!({"name": "Gone", "run_ids": [r1, 9999]})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail(&body), "One of the chosen runs no longer exists. Refresh the list and choose again.");

    let (status, body) = env.post("/api/preference-sets", json!({"name": "   ", "run_ids": [r1]})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["fields"], json!({"name": "Enter a name for the preference set."}));

    let (status, body) = env.post("/api/preference-sets", json!({"name": "", "run_ids": [r1]})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["fields"]["name"].is_string());

    let (status, body) = env.post("/api/preference-sets", json!({"name": "No runs", "run_ids": []})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["fields"]["run_ids"].is_string());
    assert_eq!(env.count("SELECT COUNT(*) FROM preference_sets").await, 0);
}

#[tokio::test]
async fn prompt_count_needs_two_finished_candidates() {
    let env = env().await;
    let (ps, p) = env.prompt_set("Set", 5).await;
    let run = env.run(ps, "Sampled twice", "m").await;
    let other = env.run(ps, "Other", "m2").await;
    // p0: two done samples from one run -> counts
    env.done(run, p[0], 0).await;
    env.done(run, p[0], 1).await;
    // p1: one done, one failed -> does not count
    env.done(run, p[1], 0).await;
    env.sample(run, p[1], 1, "failed", "stop").await;
    // p2: one done in each run, one truncated -> counts (truncated is still done)
    env.sample(run, p[2], 0, "done", "length").await;
    env.done(other, p[2], 0).await;
    // p3: pending and running -> does not count
    env.sample(run, p[3], 0, "pending", "stop").await;
    env.sample(run, p[3], 1, "running", "stop").await;
    // p4: one done in each run -> counts only when both runs are in the set
    env.done(run, p[4], 0).await;
    env.done(other, p[4], 0).await;

    assert_eq!(create(&env, "Single run", &[run]).await["prompt_count"], 1);
    assert_eq!(create(&env, "Both runs", &[run, other]).await["prompt_count"], 3);
}

#[tokio::test]
async fn list_is_newest_first_with_decided_counts() {
    let env = env().await;
    let t = two_runs(&env, 2).await;
    let first = create(&env, "First", &[t.run_a, t.run_b]).await;
    let second = create(&env, "Second", &[t.run_a, t.run_b]).await;
    let (status, _) = env
        .put(
            &format!("/api/preference-sets/{}/pairs/{}", first["id"], t.prompts[0]),
            json!({"chosen_sample_id": t.a[0], "rejected_sample_id": t.b[0]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, listed) = env.get("/api/preference-sets").await;
    let listed = listed.as_array().unwrap();
    assert_eq!(listed.iter().map(|s| s["id"].clone()).collect::<Vec<_>>(), [second["id"].clone(), first["id"].clone()]);
    assert_eq!(listed[0]["decided"], 0);
    assert_eq!(listed[1]["decided"], 1);
    assert!(listed.iter().all(|s| s["prompt_count"] == 2 && s["prompt_set_name"] == "Math word problems"));
}

#[tokio::test]
async fn unknown_set_is_404() {
    let env = env().await;
    let (status, body) = env.get("/api/preference-sets/42").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(detail(&body), "Preference set not found. It may have been deleted.");
    assert_eq!(env.get("/api/preference-sets/42/items").await.0, StatusCode::NOT_FOUND);
    assert_eq!(env.delete("/api/preference-sets/42").await.0, StatusCode::NOT_FOUND);
    assert_eq!(env.delete("/api/preference-sets/42/pairs/1").await.0, StatusCode::NOT_FOUND);
    assert_eq!(env.post("/api/preference-sets/42/auto-pair", json!({"chosen_run_id": 1, "rejected_run_id": 2})).await.0, StatusCode::NOT_FOUND);
    let (status, _) = env.put("/api/preference-sets/42/pairs/1", json!({"chosen_sample_id": 1, "rejected_sample_id": 2})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn items_page_in_prompt_order_with_candidates() {
    let env = env().await;
    let (ps, prompts) = env.prompt_set("Set", 6).await;
    let run_a = env.run(ps, "Two samples", "model-a").await;
    let run_b = env.run(ps, "One sample", "model-b").await;
    // Insert out of order to prove sorting: run B first, then run A's samples 1 then 0.
    for p in prompts.iter().rev() {
        env.done(run_b, *p, 0).await;
        env.sample(run_a, *p, 1, "done", "length").await;
        env.done(run_a, *p, 0).await;
    }
    let pset = create(&env, "Review", &[run_a, run_b]).await;

    let (status, page) = env.get(&format!("/api/preference-sets/{}/items?offset=2&limit=3", pset["id"])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((page["total"].clone(), page["offset"].clone(), page["limit"].clone()), (json!(6), json!(2), json!(3)));
    let idxs: Vec<i64> = page["items"].as_array().unwrap().iter().map(|i| i["prompt_idx"].as_i64().unwrap()).collect();
    assert_eq!(idxs, [2, 3, 4]);
    let item = &page["items"][0];
    assert_eq!(item["prompt_id"], prompts[2]);
    assert_eq!(item["messages"], json!([{"role": "user", "content": "Question 2"}]));
    assert_eq!((item["chosen_sample_id"].clone(), item["rejected_sample_id"].clone(), item["source"].clone()), (json!(null), json!(null), json!(null)));
    let cands = item["candidates"].as_array().unwrap();
    let order: Vec<(Value, Value)> = cands.iter().map(|c| (c["run_id"].clone(), c["sample_idx"].clone())).collect();
    assert_eq!(order, [(json!(run_a), json!(0)), (json!(run_a), json!(1)), (json!(run_b), json!(0))]);
    assert_eq!((cands[0]["run_name"].clone(), cands[0]["model"].clone()), (json!("Two samples"), json!("model-a")));
    assert_eq!(cands[1]["finish_reason"], "length");
    assert_eq!(cands[2]["reasoning"], format!("thinking {run_b}/2/0"));
    assert_eq!(cands[2]["response"], format!("answer {run_b}/2/0"));
    assert_eq!((cands[0]["completion_tokens"].clone(), cands[1]["completion_tokens"].clone()), (json!(100), json!(101)));
    assert_eq!((cands[0]["reasoning_tokens"].clone(), cands[1]["reasoning_tokens"].clone()), (json!(60), json!(61)));
    // Past the end keeps the total.
    let (_, past) = env.get(&format!("/api/preference-sets/{}/items?offset=50", pset["id"])).await;
    assert_eq!((past["items"].clone(), past["total"].clone()), (json!([]), json!(6)));
}

#[tokio::test]
async fn items_filter() {
    let env = env().await;
    let t = two_runs(&env, 30).await;
    let pset = create(&env, "Filters", &[t.run_a, t.run_b]).await;
    let url = format!("/api/preference-sets/{}", pset["id"]);
    for i in 0..3 {
        let (status, _) = env.put(&format!("{url}/pairs/{}", t.prompts[i]), json!({"chosen_sample_id": t.a[i], "rejected_sample_id": t.b[i]})).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (_, decided) = env.get(&format!("{url}/items?filter=decided")).await;
    assert_eq!(decided["total"], 3);
    let idxs: Vec<i64> = decided["items"].as_array().unwrap().iter().map(|i| i["prompt_idx"].as_i64().unwrap()).collect();
    assert_eq!(idxs, [0, 1, 2]);
    assert_eq!(decided["items"][0]["chosen_sample_id"], t.a[0]);
    assert_eq!(decided["items"][0]["rejected_sample_id"], t.b[0]);
    assert_eq!(decided["items"][0]["source"], "manual");

    let (_, undecided) = env.get(&format!("{url}/items?filter=undecided&limit=1")).await;
    assert_eq!(undecided["total"], 27);
    assert_eq!(undecided["items"][0]["prompt_idx"], 3);

    let (_, all) = env.get(&format!("{url}/items?filter=all&limit=200")).await;
    assert_eq!((all["total"].clone(), all["items"].as_array().unwrap().len()), (json!(30), 30));

    let (status, body) = env.get(&format!("{url}/items?filter=maybe")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["detail"].as_str().unwrap().contains("allowed values"));
    assert_eq!(env.get(&format!("{url}/items?limit=201")).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(env.get(&format!("{url}/items?limit=0")).await.0, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn put_pair_upserts_one_pair_per_prompt() {
    let env = env().await;
    let t = two_runs(&env, 2).await;
    let pset = create(&env, "Upsert", &[t.run_a, t.run_b]).await;
    let url = format!("/api/preference-sets/{}/pairs/{}", pset["id"], t.prompts[0]);

    let (status, item) = env.put(&url, json!({"chosen_sample_id": t.a[0], "rejected_sample_id": t.b[0]})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(item["prompt_id"], t.prompts[0]);
    assert_eq!((item["chosen_sample_id"].clone(), item["rejected_sample_id"].clone()), (json!(t.a[0]), json!(t.b[0])));
    assert_eq!(item["source"], "manual");
    assert_eq!(item["candidates"].as_array().unwrap().len(), 2);

    let (status, item) = env.put(&url, json!({"chosen_sample_id": t.b[0], "rejected_sample_id": t.a[0]})).await;
    assert_eq!((status, item["chosen_sample_id"].clone()), (StatusCode::OK, json!(t.b[0])));
    assert_eq!(env.count("SELECT COUNT(*) FROM preference_pairs").await, 1);
    assert_eq!(env.count("SELECT chosen_sample_id FROM preference_pairs").await, t.b[0]);
    assert_eq!(env.get(&format!("/api/preference-sets/{}", pset["id"])).await.1["decided"], 1);
}

#[tokio::test]
async fn put_pair_validation() {
    let env = env().await;
    let t = two_runs(&env, 2).await;
    let p0 = t.prompts[0];
    let outsider = env.run(t.ps, "Not in set", "m3").await;
    let outsider_sample = env.done(outsider, p0, 0).await;
    let failed = env.sample(t.run_a, p0, 1, "failed", "stop").await;
    let (_, other_prompts) = env.prompt_set("Other set", 1).await;
    let pset = create(&env, "Validation", &[t.run_a, t.run_b]).await;
    let base = format!("/api/preference-sets/{}/pairs", pset["id"]);
    let put = |prompt: i64, chosen: i64, rejected: i64| {
        let url = format!("{base}/{prompt}");
        let env = &env;
        async move { env.put(&url, json!({"chosen_sample_id": chosen, "rejected_sample_id": rejected})).await }
    };

    let (status, body) = put(p0, t.a[0], t.a[0]).await;
    assert_eq!((status, detail(&body)), (StatusCode::UNPROCESSABLE_ENTITY, "Chosen and rejected must be two different responses."));
    let (status, body) = put(p0, failed, t.b[0]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail(&body), "The chosen response has not finished, so it cannot be paired.");
    let (status, body) = put(p0, t.a[0], outsider_sample).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail(&body), "The rejected response comes from a run that is not in this preference set.");
    let (status, body) = put(p0, t.a[1], t.b[0]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail(&body), "The chosen response answers a different prompt. Reload and pick again.");
    let (status, body) = put(p0, t.a[0], 999_999).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail(&body), "The rejected response does not exist. Reload the prompt and pick again.");
    // A prompt from another prompt set.
    assert_eq!(put(other_prompts[0], t.a[0], t.b[0]).await.0, StatusCode::NOT_FOUND);
    assert_eq!(put(999_999, t.a[0], t.b[0]).await.0, StatusCode::NOT_FOUND);
    // A missing field.
    let (status, body) = env.put(&format!("{base}/{p0}"), json!({"chosen_sample_id": t.a[0]})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["fields"]["rejected_sample_id"], "Field required.");
    // Nothing was written by the failed attempts.
    assert_eq!(env.get(&format!("/api/preference-sets/{}", pset["id"])).await.1["decided"], 0);
}

#[tokio::test]
async fn delete_pair() {
    let env = env().await;
    let t = two_runs(&env, 2).await;
    let pset = create(&env, "Clear", &[t.run_a, t.run_b]).await;
    let url = format!("/api/preference-sets/{}/pairs/{}", pset["id"], t.prompts[1]);
    env.put(&url, json!({"chosen_sample_id": t.a[1], "rejected_sample_id": t.b[1]})).await;
    let set_url = format!("/api/preference-sets/{}", pset["id"]);
    assert_eq!(env.get(&set_url).await.1["decided"], 1);
    assert_eq!(env.delete(&url).await.0, StatusCode::NO_CONTENT);
    assert_eq!(env.get(&set_url).await.1["decided"], 0);
    // Clearing again is harmless.
    assert_eq!(env.delete(&url).await.0, StatusCode::NO_CONTENT);
    assert_eq!(env.get(&format!("{set_url}/items?filter=undecided")).await.1["total"], 2);
}

#[tokio::test]
async fn auto_pair_prefers_one_run_and_keeps_manual_pairs() {
    let env = env().await;
    let (ps, p) = env.prompt_set("Auto", 5).await;
    let run_a = env.run(ps, "Strong", "big").await;
    let run_b = env.run(ps, "Weak", "small").await;
    let mut first_a = std::collections::HashMap::new();
    for (n, pid) in p[..4].iter().enumerate() {
        if n == 0 {
            // Run A's sample 0 failed on p0, so its first finished sample is sample 1.
            env.sample(run_a, *pid, 0, "failed", "stop").await;
            first_a.insert(*pid, env.done(run_a, *pid, 1).await);
        } else {
            first_a.insert(*pid, env.done(run_a, *pid, 0).await);
            env.done(run_a, *pid, 1).await;
        }
    }
    let mut first_b = std::collections::HashMap::new();
    for pid in &p[..3] {
        first_b.insert(*pid, env.done(run_b, *pid, 0).await);
    }
    // p3: run B has nothing; p4: neither run answered.
    let pset = create(&env, "Auto pairs", &[run_a, run_b]).await;
    let base = format!("/api/preference-sets/{}", pset["id"]);

    // A hand-made pair on p1 (reversed preference) must survive.
    let (status, _) = env.put(&format!("{base}/pairs/{}", p[1]), json!({"chosen_sample_id": first_b[&p[1]], "rejected_sample_id": first_a[&p[1]]})).await;
    assert_eq!(status, StatusCode::OK);

    let (status, res) = env.post(&format!("{base}/auto-pair"), json!({"chosen_run_id": run_a, "rejected_run_id": run_b})).await;
    assert_eq!((status, res), (StatusCode::OK, json!({"created": 2, "skipped": 1})));

    let rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        "SELECT prompt_id, chosen_sample_id, rejected_sample_id, source FROM preference_pairs WHERE preference_set_id = ? ORDER BY prompt_id",
    )
    .bind(pset["id"].as_i64())
    .fetch_all(&env.ctx.db)
    .await
    .unwrap();
    assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), [p[0], p[1], p[2]]);
    assert_eq!(rows[0], (p[0], first_a[&p[0]], first_b[&p[0]], "rule".to_string()));
    assert_eq!(rows[1], (p[1], first_b[&p[1]], first_a[&p[1]], "manual".to_string()));

    // Running it again rewrites rule pairs and still keeps the manual one.
    let (_, res) = env.post(&format!("{base}/auto-pair"), json!({"chosen_run_id": run_a, "rejected_run_id": run_b})).await;
    assert_eq!(res, json!({"created": 2, "skipped": 1}));

    // Overwrite replaces the manual pair too, and flips the rule pairs to the new direction.
    let (_, res) = env.post(&format!("{base}/auto-pair"), json!({"chosen_run_id": run_b, "rejected_run_id": run_a, "overwrite": true})).await;
    assert_eq!(res, json!({"created": 3, "skipped": 0}));
    let rows: Vec<(i64, i64, String)> = sqlx::query_as("SELECT prompt_id, chosen_sample_id, source FROM preference_pairs WHERE preference_set_id = ? ORDER BY prompt_id")
        .bind(pset["id"].as_i64())
        .fetch_all(&env.ctx.db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|r| r.2 == "rule"));
    assert_eq!(rows.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(), p[..3].iter().map(|pid| (*pid, first_b[pid])).collect::<Vec<_>>());
    assert_eq!(env.get(&base).await.1["decided"], 3);
}

#[tokio::test]
async fn auto_pair_validation() {
    let env = env().await;
    let t = two_runs(&env, 1).await;
    let outsider = env.run(t.ps, "Outsider", "m").await;
    let pset = create(&env, "Rules", &[t.run_a, t.run_b]).await;
    let url = format!("/api/preference-sets/{}/auto-pair", pset["id"]);

    let (status, body) = env.post(&url, json!({"chosen_run_id": t.run_a, "rejected_run_id": t.run_a})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail(&body), "Pick two different runs: one for chosen and one for rejected.");
    let (status, body) = env.post(&url, json!({"chosen_run_id": outsider, "rejected_run_id": t.run_b})).await;
    assert_eq!((status, detail(&body)), (StatusCode::UNPROCESSABLE_ENTITY, "The chosen run is not in this preference set."));
    let (status, body) = env.post(&url, json!({"chosen_run_id": t.run_a, "rejected_run_id": outsider})).await;
    assert_eq!((status, detail(&body)), (StatusCode::UNPROCESSABLE_ENTITY, "The rejected run is not in this preference set."));
    let (status, body) = env.post(&url, json!({"chosen_run_id": t.run_a})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["fields"]["rejected_run_id"], "Field required.");
}

#[tokio::test]
async fn auto_pair_handles_many_prompts() {
    let env = env().await;
    let (ps, _) = env.prompt_set("Big", 0).await;
    sqlx::query(
        "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < 1199) \
         INSERT INTO prompts (prompt_set_id, idx, messages, meta) SELECT ?, i, '[{\"role\":\"user\",\"content\":\"q\"}]', '{}' FROM n",
    )
    .bind(ps)
    .execute(&env.ctx.db)
    .await
    .unwrap();
    let run_a = env.run(ps, "A", "a").await;
    let run_b = env.run(ps, "B", "b").await;
    for run in [run_a, run_b] {
        sqlx::query("INSERT INTO samples (run_id, prompt_id, prompt_idx, sample_idx, status, response, attempts) SELECT ?, id, idx, 0, 'done', 'x', 1 FROM prompts WHERE prompt_set_id = ?")
            .bind(run)
            .bind(ps)
            .execute(&env.ctx.db)
            .await
            .unwrap();
    }
    let pset = create(&env, "Big", &[run_a, run_b]).await;
    assert_eq!(pset["prompt_count"], 1200);
    let (_, res) = env.post(&format!("/api/preference-sets/{}/auto-pair", pset["id"]), json!({"chosen_run_id": run_a, "rejected_run_id": run_b})).await;
    assert_eq!(res, json!({"created": 1200, "skipped": 0}));
    let (_, items) = env.get(&format!("/api/preference-sets/{}/items?filter=undecided", pset["id"])).await;
    assert_eq!(items["total"], 0);
}

#[tokio::test]
async fn delete_set_removes_pairs() {
    let env = env().await;
    let t = two_runs(&env, 3).await;
    let keep = create(&env, "Keep", &[t.run_a, t.run_b]).await;
    let drop = create(&env, "Drop", &[t.run_a, t.run_b]).await;
    for pset in [&keep, &drop] {
        let (_, res) = env.post(&format!("/api/preference-sets/{}/auto-pair", pset["id"]), json!({"chosen_run_id": t.run_a, "rejected_run_id": t.run_b})).await;
        assert_eq!(res["created"], 3);
    }
    assert_eq!(env.delete(&format!("/api/preference-sets/{}", drop["id"])).await.0, StatusCode::NO_CONTENT);
    assert_eq!(env.get(&format!("/api/preference-sets/{}", drop["id"])).await.0, StatusCode::NOT_FOUND);
    let left: Vec<(i64, i64)> = sqlx::query_as("SELECT preference_set_id, COUNT(*) FROM preference_pairs GROUP BY preference_set_id")
        .fetch_all(&env.ctx.db)
        .await
        .unwrap();
    assert_eq!(left, [(keep["id"].as_i64().unwrap(), 3)]);
    let (_, listed) = env.get("/api/preference-sets").await;
    assert_eq!(listed.as_array().unwrap().iter().map(|s| s["id"].clone()).collect::<Vec<_>>(), [keep["id"].clone()]);
}

#[tokio::test]
async fn deleted_run_drops_out_of_set() {
    let env = env().await;
    let t = two_runs(&env, 2).await;
    let pset = create(&env, "Run deleted", &[t.run_a, t.run_b]).await;
    sqlx::query("DELETE FROM runs WHERE id = ?").bind(t.run_b).execute(&env.ctx.db).await.unwrap();
    let (_, out) = env.get(&format!("/api/preference-sets/{}", pset["id"])).await;
    assert_eq!(out["run_ids"], json!([t.run_a]));
    assert_eq!(out["prompt_count"], 0);
    let (_, items) = env.get(&format!("/api/preference-sets/{}/items", pset["id"])).await;
    assert_eq!(items, json!({"items": [], "total": 0, "offset": 0, "limit": 20}));
}

#[tokio::test]
async fn deleted_prompt_set_removes_its_preference_sets() {
    let env = env().await;
    let t = two_runs(&env, 2).await;
    create(&env, "Gone soon", &[t.run_a, t.run_b]).await;
    assert_eq!(env.delete(&format!("/api/prompt-sets/{}", t.ps)).await.0, StatusCode::NO_CONTENT);
    assert_eq!(env.get("/api/preference-sets").await.1, json!([]));
}
