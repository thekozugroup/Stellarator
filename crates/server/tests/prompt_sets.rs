mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use common::*;
use serde_json::json;
use stellarator_core::probes::{ActiveRun, Idle};

#[tokio::test]
async fn list_is_newest_first_with_run_counts_and_v2_fields() {
    let env = env().await;
    let (a, _) = env.prompt_set("Older", 2).await;
    let (b, _) = env.prompt_set("Newer", 3).await;
    sqlx::query("UPDATE prompt_sets SET created_at = '2026-01-01 00:00:00.000000' WHERE id = ?").bind(a).execute(&env.ctx.db).await.unwrap();
    sqlx::query("UPDATE prompt_sets SET created_at = '2026-02-01 00:00:00.000000', status = 'importing', status_detail = 'Reading train', rows_read = 120, rows_total = 500 WHERE id = ?")
        .bind(b).execute(&env.ctx.db).await.unwrap();
    env.run(a, "r1", "m").await;
    env.run(a, "r2", "m").await;

    let (status, body) = env.get("/api/prompt-sets").await;
    assert_eq!(status, StatusCode::OK);
    let sets = body.as_array().unwrap();
    assert_eq!(sets.iter().map(|s| s["name"].as_str().unwrap()).collect::<Vec<_>>(), ["Newer", "Older"]);
    assert_eq!(sets[0]["status"], "importing");
    assert_eq!(sets[0]["status_detail"], "Reading train");
    assert_eq!(sets[0]["rows_read"], 120);
    assert_eq!(sets[0]["rows_total"], 500);
    assert_eq!(sets[0]["run_count"], 0);
    assert_eq!(sets[0]["prompt_count"], 3);
    assert_eq!(sets[0]["created_at"], "2026-02-01T00:00:00Z");
    assert_eq!(sets[1]["status"], "ready");
    assert_eq!(sets[1]["status_detail"], json!(null));
    assert_eq!(sets[1]["rows_total"], json!(null));
    assert_eq!(sets[1]["run_count"], 2);
    assert_eq!(sets[1]["source_kind"], "paste");
    assert_eq!(sets[1]["source"], json!({}));
}

#[tokio::test]
async fn get_rename_and_404() {
    let env = env().await;
    let (id, _) = env.prompt_set("Old name", 1).await;
    let (status, body) = env.get(&format!("/api/prompt-sets/{id}")).await;
    assert_eq!((status, body["name"].as_str()), (StatusCode::OK, Some("Old name")));
    let (status, body) = env.patch(&format!("/api/prompt-sets/{id}"), json!({"name": "New name"})).await;
    assert_eq!((status, body["name"].as_str()), (StatusCode::OK, Some("New name")));
    let (status, body) = env.patch(&format!("/api/prompt-sets/{id}"), json!({"name": ""})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["fields"]["name"].is_string());
    assert_eq!(env.patch(&format!("/api/prompt-sets/{id}"), json!({})).await.1["fields"]["name"], "Field required.");
    assert_eq!(env.get("/api/prompt-sets/99").await, (StatusCode::NOT_FOUND, json!({"detail": "Prompt set not found"})));
    assert_eq!(env.patch("/api/prompt-sets/99", json!({"name": "x"})).await.0, StatusCode::NOT_FOUND);
    assert_eq!(env.get("/api/prompt-sets/99/prompts").await.0, StatusCode::NOT_FOUND);
    assert_eq!(env.delete("/api/prompt-sets/99").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn prompts_page_and_search() {
    let env = env().await;
    let (id, ids) = env.prompt_set("Set", 30).await;
    sqlx::query("UPDATE prompts SET messages = ?, meta = '{\"answer\": 42}' WHERE id = ?")
        .bind(json!([{"role": "system", "content": "Be Brief"}, {"role": "user", "content": "100% sure_thing"}]).to_string())
        .bind(ids[7])
        .execute(&env.ctx.db)
        .await
        .unwrap();

    let (status, page) = env.get(&format!("/api/prompt-sets/{id}/prompts?offset=5&limit=3")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((page["total"].as_i64(), page["offset"].as_i64(), page["limit"].as_i64()), (Some(30), Some(5), Some(3)));
    let idxs: Vec<i64> = page["items"].as_array().unwrap().iter().map(|p| p["idx"].as_i64().unwrap()).collect();
    assert_eq!(idxs, [5, 6, 7]);
    assert_eq!(page["items"][2]["messages"][0]["content"], "Be Brief");
    assert_eq!(page["items"][2]["meta"], json!({"answer": 42}));
    assert_eq!(page["items"][0]["id"], ids[5]);

    // Default limit is 50.
    assert_eq!(env.get(&format!("/api/prompt-sets/{id}/prompts")).await.1["limit"], 50);

    // Case-insensitive search over the messages JSON text.
    let (_, hits) = env.get(&format!("/api/prompt-sets/{id}/prompts?q=be%20brief")).await;
    assert_eq!(hits["total"], 1);
    assert_eq!(hits["items"][0]["idx"], 7);
    let (_, hits) = env.get(&format!("/api/prompt-sets/{id}/prompts?q=QUESTION%201")).await;
    assert_eq!(hits["total"], 11); // 1, 10..19
    // % and _ are matched literally.
    assert_eq!(env.get(&format!("/api/prompt-sets/{id}/prompts?q=100%25")).await.1["total"], 1);
    assert_eq!(env.get(&format!("/api/prompt-sets/{id}/prompts?q=sure_thing")).await.1["total"], 1);
    assert_eq!(env.get(&format!("/api/prompt-sets/{id}/prompts?q=sure%25thing")).await.1["total"], 0);
    // An empty q is no search.
    assert_eq!(env.get(&format!("/api/prompt-sets/{id}/prompts?q=")).await.1["total"], 30);
    // Past the end.
    let (_, empty) = env.get(&format!("/api/prompt-sets/{id}/prompts?offset=100")).await;
    assert_eq!((empty["items"].as_array().unwrap().len(), empty["total"].as_i64()), (0, Some(30)));

    for (query, field) in [("limit=0", "limit"), ("limit=501", "limit"), ("offset=-1", "offset")] {
        let (status, body) = env.get(&format!("/api/prompt-sets/{id}/prompts?{query}")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}");
        assert!(body["fields"][field].is_string(), "{query}: {body}");
    }
    assert_eq!(env.get(&format!("/api/prompt-sets/{id}/prompts?limit=abc")).await.0, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn delete_cascades_to_runs_samples_and_preferences() {
    let env = env().await;
    let (keep, keep_prompts) = env.prompt_set("Keep", 2).await;
    let (drop, drop_prompts) = env.prompt_set("Drop", 2).await;
    let ra = env.run(drop, "a", "m").await;
    let rb = env.run(drop, "b", "m").await;
    let rk = env.run(keep, "k", "m").await;
    for p in &drop_prompts {
        env.done(ra, *p, 0).await;
        env.done(rb, *p, 0).await;
    }
    env.done(rk, keep_prompts[0], 0).await;
    let (_, pref) = env.post("/api/preference-sets", json!({"name": "P", "run_ids": [ra, rb]})).await;
    let (status, _) = env.post(&format!("/api/preference-sets/{}/auto-pair", pref["id"]), json!({"chosen_run_id": ra, "rejected_run_id": rb})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(env.count("SELECT COUNT(*) FROM preference_pairs").await, 2);

    assert_eq!(env.delete(&format!("/api/prompt-sets/{drop}")).await.0, StatusCode::NO_CONTENT);
    for table in ["runs", "samples", "preference_sets", "preference_pairs", "prompts"] {
        let remaining = env.count(&format!("SELECT COUNT(*) FROM {table}")).await;
        let expected = match table { "runs" | "preference_sets" | "preference_pairs" => (table == "runs") as i64, "samples" => 1, _ => 2 };
        assert_eq!(remaining, expected, "{table}");
    }
    assert_eq!(env.get(&format!("/api/prompt-sets/{drop}")).await.0, StatusCode::NOT_FOUND);
    assert_eq!(env.get(&format!("/api/prompt-sets/{keep}")).await.0, StatusCode::OK);
}

#[tokio::test]
async fn delete_is_refused_while_a_run_is_active() {
    let probe = Arc::new(ActiveRuns(vec![ActiveRun { run_id: 1, endpoint_id: None, prompt_set_id: 1 }]));
    let env = env_with(probe, Arc::new(Idle)).await;
    let (id, _) = env.prompt_set("Busy", 1).await;
    assert_eq!(id, 1);
    let (status, body) = env.delete("/api/prompt-sets/1").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["detail"], "A run on this prompt set is still going. Pause it first.");
    assert_eq!(env.count("SELECT COUNT(*) FROM prompt_sets").await, 1);
    // A run on another set does not block it.
    let (other, _) = env.prompt_set("Free", 1).await;
    assert_eq!(env.delete(&format!("/api/prompt-sets/{other}")).await.0, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_while_importing_cancels_the_import_first() {
    let imports = FakeImports::new(1);
    let env = env_with(Arc::new(Idle), imports.clone()).await;
    let (id, _) = env.prompt_set("Importing", 5).await;
    sqlx::query("UPDATE prompt_sets SET status = 'importing' WHERE id = ?").bind(id).execute(&env.ctx.db).await.unwrap();
    assert!(imports.is_importing_now(id));
    assert_eq!(env.delete(&format!("/api/prompt-sets/{id}")).await.0, StatusCode::NO_CONTENT);
    assert_eq!(*imports.cancelled.lock().unwrap(), vec![id]);
    assert_eq!(env.count("SELECT COUNT(*) FROM prompt_sets").await, 0);
    assert_eq!(env.count("SELECT COUNT(*) FROM prompts").await, 0);

    // A set that is not importing is deleted without a cancel.
    let (other, _) = env.prompt_set("Not importing", 1).await;
    assert_eq!(env.delete(&format!("/api/prompt-sets/{other}")).await.0, StatusCode::NO_CONTENT);
    assert_eq!(imports.cancelled.lock().unwrap().len(), 1);
}
