//! Endpoint routes, with a tiny fake OpenAI server for the model list and key check.

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;
use stellarator_core::probes::ActiveRun;

fn rejecting() -> serde_json::Value {
    json!({"error": {"message": "User not found."}})
}

#[tokio::test]
async fn rejected_key_is_reported_on_create_and_test() {
    let fake = fake_openai(&["m2", "M1"], 200, 401, rejecting()).await;
    let env = env().await;
    let (status, body) = env
        .post("/api/endpoints", json!({"name": "Hosted", "base_url": fake.base_url, "api_key": "sk-bad-key-0123456789"}))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["last_ok"], false);
    assert_eq!(body["models"], json!(["M1", "m2"]));
    assert_eq!(body["last_error"], "The server rejected the API key (401): User not found.");
    assert_eq!(body["has_api_key"], true);
    assert_eq!(body["api_key_hint"], "…6789");
    assert_eq!(body["run_count"], 0);
    assert!(body["last_checked_at"].as_str().unwrap().ends_with('Z'));

    // The key probe sends the key, an empty chat and max_tokens 1.
    let chat = fake.seen.chat.lock().unwrap().clone();
    assert_eq!(chat[0].0.as_deref(), Some("Bearer sk-bad-key-0123456789"));
    assert_eq!(chat[0].1["max_tokens"], 1);

    let (status, t) = env.post("/api/endpoints/test", json!({"base_url": fake.base_url, "api_key": "sk-bad"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(t["ok"], false);
    assert_eq!(t["models"], json!(["M1", "m2"]));
    assert!(t["error"].as_str().unwrap().contains("401"));
    assert!(t["latency_ms"].is_i64());
}

#[tokio::test]
async fn accepted_key_passes_on_bad_request() {
    let fake = fake_openai(&["m1"], 200, 400, json!({"error": {"message": "model is required"}})).await;
    let env = env().await;
    let (_, body) = env.post("/api/endpoints", json!({"name": "Good", "base_url": fake.base_url, "api_key": "sk-good"})).await;
    assert_eq!(body["last_ok"], true);
    assert_eq!(body["last_error"], json!(null));
    let (_, t) = env.post("/api/endpoints/test", json!({"base_url": fake.base_url})).await;
    assert_eq!(t["ok"], true);
    assert_eq!(t["error"], json!(null));
}

#[tokio::test]
async fn unreachable_server_is_a_stored_error_not_a_failure() {
    let env = env().await;
    let (status, body) = env.post("/api/endpoints", json!({"name": "Down", "base_url": "http://127.0.0.1:1/v1/"})).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["base_url"], "http://127.0.0.1:1/v1");
    assert_eq!(body["last_ok"], false);
    assert_eq!(body["last_error"], "Could not connect. Check the URL and that the server is up.");
    assert_eq!(body["models"], json!([]));
    assert_eq!(body["has_api_key"], false);
    assert_eq!(body["api_key_hint"], json!(null));
    let (_, t) = env.post("/api/endpoints/test", json!({"base_url": "http://127.0.0.1:1"})).await;
    assert_eq!(t, json!({"ok": false, "models": [], "latency_ms": null, "error": "Could not connect. Check the URL and that the server is up."}));
}

#[tokio::test]
async fn test_uses_the_saved_key_when_the_form_leaves_it_blank() {
    let fake = fake_openai(&["m"], 200, 400, json!({})).await;
    let env = env().await;
    let (_, ep) = env.post("/api/endpoints", json!({"name": "E", "base_url": fake.base_url, "api_key": "saved-key-value"})).await;
    let (_, t) = env.post("/api/endpoints/test", json!({"base_url": fake.base_url, "api_key": "", "endpoint_id": ep["id"]})).await;
    assert_eq!(t["ok"], true);
    let auth = fake.seen.models_auth.lock().unwrap().clone();
    assert_eq!(auth.last().unwrap().as_deref(), Some("Bearer saved-key-value"));
}

#[tokio::test]
async fn validation_errors_name_the_field() {
    let env = env().await;
    let (status, body) = env.post("/api/endpoints", json!({"name": "x", "base_url": "localhost:8000"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["fields"]["base_url"], "Base URL must start with http:// or https://");
    assert_eq!(body["detail"], "Base URL must start with http:// or https://");
    let (status, body) = env.post("/api/endpoints", json!({"base_url": "http://x"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["fields"]["name"], "Field required.");
    let (status, body) = env.post("/api/endpoints", json!({"name": "x", "base_url": "http://x", "default_concurrency": 0})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["fields"]["default_concurrency"].is_string());
    assert_eq!(env.get("/api/endpoints/abc").await.0, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn crud_check_and_run_count() {
    let fake = fake_openai(&["a"], 200, 400, json!({})).await;
    let env = env().await;
    let (_, first) = env.post("/api/endpoints", json!({"name": "First", "base_url": fake.base_url, "location": "cloud", "default_concurrency": 8, "headers": {"X-Title": "S"}})).await;
    let (_, second) = env.post("/api/endpoints", json!({"name": "Second", "base_url": fake.base_url})).await;
    assert_eq!(first["location"], "cloud");
    assert_eq!(first["default_concurrency"], 8);
    assert_eq!(first["headers"], json!({"X-Title": "S"}));

    let (_, listed) = env.get("/api/endpoints").await;
    assert_eq!(listed.as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect::<Vec<_>>(), ["Second", "First"]);

    let (ps, _) = env.prompt_set("P", 1).await;
    env.run(ps, "r1", "m").await;
    env.run_on(ps, "r2", "m", first["id"].as_i64(), "completed").await;
    env.run_on(ps, "r3", "m", first["id"].as_i64(), "completed").await;
    let (_, got) = env.get(&format!("/api/endpoints/{}", first["id"])).await;
    assert_eq!(got["run_count"], 2);
    let (_, listed) = env.get("/api/endpoints").await;
    assert_eq!(listed[1]["run_count"], 2);
    assert_eq!(listed[0]["run_count"], 0);

    // PATCH: rename, set a key, check runs again.
    let (status, patched) = env.patch(&format!("/api/endpoints/{}", second["id"]), json!({"name": "Renamed", "api_key": "abcdefghijkl"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["name"], "Renamed");
    assert_eq!(patched["api_key_hint"], "…ijkl");
    assert_eq!(patched["models"], json!(["a"]));
    // Leaving the key out keeps it; "" removes it.
    let (_, kept) = env.patch(&format!("/api/endpoints/{}", second["id"]), json!({"default_concurrency": 2})).await;
    assert_eq!(kept["has_api_key"], true);
    assert_eq!(kept["default_concurrency"], 2);
    let (_, removed) = env.patch(&format!("/api/endpoints/{}", second["id"]), json!({"api_key": ""})).await;
    assert_eq!(removed["has_api_key"], false);

    let (status, checked) = env.post(&format!("/api/endpoints/{}/check", second["id"]), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(checked["last_ok"], true);

    assert_eq!(env.get("/api/endpoints/999").await, (StatusCode::NOT_FOUND, json!({"detail": "Endpoint not found"})));
    assert_eq!(env.patch("/api/endpoints/999", json!({"name": "x"})).await.0, StatusCode::NOT_FOUND);
    assert_eq!(env.post("/api/endpoints/999/check", json!({})).await.0, StatusCode::NOT_FOUND);
    assert_eq!(env.delete("/api/endpoints/999").await.0, StatusCode::NOT_FOUND);

    // Deleting keeps the runs, with their endpoint cleared.
    assert_eq!(env.delete(&format!("/api/endpoints/{}", first["id"])).await.0, StatusCode::NO_CONTENT);
    assert_eq!(env.count("SELECT COUNT(*) FROM runs WHERE endpoint_id IS NULL").await, 3);
    assert_eq!(env.get(&format!("/api/endpoints/{}", first["id"])).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn failed_recheck_keeps_the_last_model_list() {
    let fake = fake_openai(&["a", "b"], 200, 400, json!({})).await;
    let env = env().await;
    let (_, ep) = env.post("/api/endpoints", json!({"name": "E", "base_url": fake.base_url})).await;
    let (_, moved) = env.patch(&format!("/api/endpoints/{}", ep["id"]), json!({"base_url": "http://127.0.0.1:1/v1"})).await;
    assert_eq!(moved["last_ok"], false);
    assert_eq!(moved["models"], json!(["a", "b"]));
}

#[tokio::test]
async fn delete_is_refused_while_a_run_uses_the_endpoint() {
    let env = env().await;
    let ep = env.endpoint("Busy").await;
    let (ps, _) = env.prompt_set("P", 1).await;
    let run = env.run_on(ps, "going", "m", Some(ep), "running").await;
    let (status, body) = env.delete(&format!("/api/endpoints/{ep}")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["detail"], "A run is using this endpoint. Pause it before deleting the endpoint.");

    // Once the run is paused the endpoint can go.
    sqlx::query("UPDATE runs SET status = 'paused' WHERE id = ?").bind(run).execute(&env.ctx.db).await.unwrap();
    assert_eq!(env.delete(&format!("/api/endpoints/{ep}")).await.0, StatusCode::NO_CONTENT);

    // A run the engine reports as active counts too (the database row may lag behind).
    let probe = std::sync::Arc::new(ActiveRuns(vec![ActiveRun { run_id: 1, endpoint_id: Some(1), prompt_set_id: 1 }]));
    let env2 = env_with(probe, std::sync::Arc::new(stellarator_core::probes::Idle)).await;
    assert_eq!(env2.endpoint("Busy 2").await, 1);
    assert_eq!(env2.delete("/api/endpoints/1").await.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn presets_follow_the_mode() {
    let env = env().await;
    let (status, body) = env.get("/api/endpoint-presets").await;
    assert_eq!(status, StatusCode::OK);
    let presets = body.as_array().unwrap();
    assert_eq!(presets[0]["id"], "openrouter");
    assert_eq!(presets[0]["needs_api_key"], true);
    assert_eq!(presets[0]["location"], "cloud");
    assert_eq!(presets[0]["headers"], json!({"X-Title": "Stellarator"}));
    let ollama = presets.iter().find(|p| p["id"] == "ollama").unwrap();
    assert_eq!(ollama["base_url"], "http://localhost:11434/v1");
    assert_eq!(ollama["api_key_url"], json!(null));
    assert_eq!(ollama["default_concurrency"], 4);
}
