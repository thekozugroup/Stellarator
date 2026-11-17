//! Timing checks for big prompt sets. Run with `cargo test -p stellarator-server --test perf -- --ignored --nocapture`.

mod common;

use std::time::Instant;

#[tokio::test]
#[ignore]
async fn delete_big_prompt_set() {
    let n: i64 = std::env::var("PERF_ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(200_000);
    let env = common::env_on_disk().await;
    let db = &env.ctx.db;
    let now = stellarator_core::time::now_db();
    // Two prompt sets: the big one is deleted, the other keeps pairs that must not slow it down.
    for name in ["big", "other"] {
        let rows = if name == "big" { n } else { 20_000 };
        sqlx::query("INSERT INTO prompt_sets (name, source_kind, source, prompt_count, created_at) VALUES (?, 'paste', '{}', ?, ?)")
            .bind(name).bind(rows).bind(&now).execute(db).await.unwrap();
        let set_id: i64 = sqlx::query_scalar("SELECT id FROM prompt_sets WHERE name = ?").bind(name).fetch_one(db).await.unwrap();
        sqlx::query(
            "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < ? - 1) \
             INSERT INTO prompts (prompt_set_id, idx, messages, meta) \
             SELECT ?, i, '[{\"role\":\"user\",\"content\":\"a question\"}]', '{}' FROM n",
        ).bind(rows).bind(set_id).execute(db).await.unwrap();
        let mut run_ids = Vec::new();
        for r in 0..2 {
            let run = env.run(set_id, &format!("{name} run {r}"), "m").await;
            sqlx::query(
                "INSERT INTO samples (run_id, prompt_id, prompt_idx, sample_idx, status, response, attempts) \
                 SELECT ?, id, idx, 0, 'done', 'x', 1 FROM prompts WHERE prompt_set_id = ?",
            ).bind(run).bind(set_id).execute(db).await.unwrap();
            run_ids.push(run);
        }
        let (status, pset) = env.post("/api/preference-sets", serde_json::json!({"name": "p", "run_ids": run_ids})).await;
        assert_eq!(status, 201);
        let t = Instant::now();
        let (status, res) = env.post(
            &format!("/api/preference-sets/{}/auto-pair", pset["id"]),
            serde_json::json!({"chosen_run_id": run_ids[0], "rejected_run_id": run_ids[1]}),
        ).await;
        println!("auto-pair {name}: {res} in {:?}", t.elapsed());
        assert_eq!(status, 200);
    }
    let big: i64 = env.count("SELECT id FROM prompt_sets WHERE name = 'big'").await;
    let t = Instant::now();
    let (status, _) = env.delete(&format!("/api/prompt-sets/{big}")).await;
    println!("delete of {n} prompts: {:?}", t.elapsed());
    assert_eq!(status, 204);
    assert_eq!(env.count("SELECT COUNT(*) FROM prompts WHERE prompt_set_id = 1").await, 0);
}
