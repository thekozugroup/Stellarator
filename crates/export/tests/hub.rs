mod common;

use axum::Router;
use base64::Engine;
use common::*;
use serde_json::{json, Value};
use stellarator_core::settings;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

async fn setup(hub: Option<&FakeHub>) -> (Env, Seed, i64, Router) {
    let mut e = env().await;
    if let Some(h) = hub {
        let mut c = (*e.ctx.config).clone();
        c.hf_hub = h.url.clone();
        e.ctx.config = std::sync::Arc::new(c);
    }
    let s = seed(&e.ctx.db).await;
    let pref = seed_pref(&e.ctx.db, &s).await;
    let app = e.app();
    (e, s, pref, app)
}

async fn token(e: &Env) {
    settings::set(&e.ctx.db, "hf_token", Some(json!("hf_x"))).await.unwrap();
}

fn commit_lines(r: &Seen) -> Vec<Value> {
    ndjson(&r.body)
}

#[tokio::test]
async fn hub_push_run_inline() {
    let hub = fake_hub().await;
    let (e, s, _pref, app) = setup(Some(&hub)).await;
    let body = json!({"options": {"format": "messages"}, "target": {"repo_id": "me/ds", "private": true}});
    let uri = format!("/api/runs/{}/export/hub", s.run);

    // No token: 400 before any work.
    let r = post(&app, &uri, body.clone()).await;
    assert_eq!(r.status, 400);
    assert_eq!(r.json()["detail"], "Add a Hugging Face token in Settings to push datasets.");
    assert!(hub.requests().is_empty());
    assert!(e.leftovers().is_empty());

    token(&e).await;
    let r = post(&app, &uri, body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json(), json!({"url": format!("{}/datasets/me/ds", hub.url), "count": 3}));

    let reqs = hub.requests();
    assert_eq!(
        hub.calls(),
        [
            "POST /api/repos/create",
            "POST /api/datasets/me/ds/preupload/main",
            "POST /api/datasets/me/ds/commit/main",
            "POST /api/datasets/me/ds/preupload/main",
            "POST /api/datasets/me/ds/commit/main",
        ]
    );
    assert!(reqs.iter().all(|r| r.auth.as_deref() == Some("Bearer hf_x")));
    let create: Value = serde_json::from_slice(&reqs[0].body).unwrap();
    assert_eq!(create, json!({"type": "dataset", "private": true, "name": "ds", "organization": "me"}));

    let data = commit_lines(&reqs[2]);
    assert_eq!(data[0], json!({"key": "header", "value": {"summary": "Add data from Stellarator", "description": ""}}));
    assert_eq!(data[1]["key"], "file");
    assert_eq!(data[1]["value"]["path"], "data/train-00000-of-00001.parquet");
    assert_eq!(data[1]["value"]["encoding"], "base64");
    let parquet = B64.decode(data[1]["value"]["content"].as_str().unwrap()).unwrap();
    assert_eq!(parquet_rows(&parquet), 3);

    let readme = commit_lines(&reqs[4]);
    assert_eq!(readme[0]["value"]["summary"], "Add dataset card");
    assert_eq!(readme[1]["value"]["path"], "README.md");
    let card = String::from_utf8(B64.decode(readme[1]["value"]["content"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(
        card,
        "---\nconfigs:\n- config_name: default\n  data_files:\n  - split: train\n    path: data/train-*\n---\n\n\
         # Qwen3 32B on GSM8K\n\n- Model: qwen3-32b\n- Endpoint: spark\n- Prompt set: GSM8K sample\n- Records: 3\n\
         - Format: messages (reasoning: think_tags)\n- Generation parameters: temperature=0.7, user template set\n\
         - Generated with Stellarator.\n"
    );
    assert!(e.leftovers().is_empty(), "{:?}", e.leftovers());
}

#[tokio::test]
async fn hub_push_preference_set_and_split() {
    let hub = fake_hub().await;
    let (e, _s, pref, app) = setup(Some(&hub)).await;
    token(&e).await;
    let r = post(
        &app,
        &format!("/api/preference-sets/{pref}/export/hub"),
        json!({"options": {"format": "kto"}, "target": {"repo_id": "me/p", "split": "test"}}),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["count"], 4);
    let reqs = hub.requests();
    let data = commit_lines(&reqs[2]);
    assert_eq!(data[1]["value"]["path"], "data/test-00000-of-00001.parquet");
    let parquet = B64.decode(data[1]["value"]["content"].as_str().unwrap()).unwrap();
    assert_eq!(parquet_rows(&parquet), 4);
    let readme = commit_lines(&reqs[4]);
    let card = String::from_utf8(B64.decode(readme[1]["value"]["content"].as_str().unwrap()).unwrap()).unwrap();
    assert!(card.contains("- split: test\n    path: data/test-*"));
    assert!(card.contains("- Models: qwen3-32b\n- Endpoints: spark\n- Prompt set: GSM8K sample\n- Records: 4\n"));
    assert!(card.contains("- Format: kto, conversational (reasoning: think_tags)\n"));
    assert!(card.contains("- Generation parameters: Qwen3 32B on GSM8K: temperature=0.7, user template set\n"));
}

#[tokio::test]
async fn hub_push_large_file_uses_lfs() {
    let hub = fake_hub().await;
    hub.cfg.lock().unwrap().lfs = true;
    let (e, s, _p, app) = setup(Some(&hub)).await;
    token(&e).await;
    let r = post(&app, &format!("/api/runs/{}/export/hub", s.run), json!({"target": {"repo_id": "me/big"}})).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let reqs = hub.requests();
    let calls = hub.calls();
    let batch = calls.iter().position(|c| c.ends_with("/info/lfs/objects/batch")).expect("lfs batch call");
    assert_eq!(calls[batch], "POST /datasets/me/big.git/info/lfs/objects/batch");
    let b: Value = serde_json::from_slice(&reqs[batch].body).unwrap();
    assert_eq!(b["operation"], "upload");
    assert_eq!(b["hash_algo"], "sha256");
    let oid = b["objects"][0]["oid"].as_str().unwrap().to_string();
    let put = reqs.iter().find(|r| r.method == "PUT").expect("PUT upload");
    assert_eq!(put.path, format!("/lfs-put/{oid}"));
    assert_eq!(put.auth, None); // the storage URL gets no Hub token
    assert_eq!(sha256_hex(&put.body), oid);
    assert_eq!(b["objects"][0]["size"], put.body.len());
    assert_eq!(parquet_rows(&put.body), 3);
    assert!(calls.contains(&"POST /lfs-verify".to_string()));
    let commit = reqs.iter().filter(|r| r.path.ends_with("/commit/main")).next().unwrap();
    let lines = commit_lines(commit);
    assert_eq!(
        lines[1],
        json!({"key": "lfsFile", "value": {"path": "data/train-00000-of-00001.parquet", "algo": "sha256", "oid": oid, "size": put.body.len()}})
    );
    // The card still goes inline.
    let last = reqs.iter().rev().find(|r| r.path.ends_with("/commit/main")).unwrap();
    assert_eq!(commit_lines(last)[1]["key"], "file");
}

#[tokio::test]
async fn hub_bare_repo_name_uses_the_token_owner() {
    let hub = fake_hub().await;
    let (e, s, _p, app) = setup(Some(&hub)).await;
    token(&e).await;
    let r = post(&app, &format!("/api/runs/{}/export/hub", s.run), json!({"target": {"repo_id": "ds"}})).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["url"], format!("{}/datasets/me/ds", hub.url));
    assert!(hub.calls().contains(&"GET /api/whoami-v2".to_string()));
    let create: Value = serde_json::from_slice(&hub.requests()[0].body).unwrap();
    assert!(create.get("organization").is_none());
}

#[tokio::test]
async fn hub_existing_repo_is_fine() {
    let hub = fake_hub().await;
    hub.cfg.lock().unwrap().create_status = Some(409);
    let (e, s, _p, app) = setup(Some(&hub)).await;
    token(&e).await;
    let r = post(&app, &format!("/api/runs/{}/export/hub", s.run), json!({"target": {"repo_id": "me/ds"}})).await;
    assert_eq!(r.status, 200, "{}", r.text());
}

#[tokio::test]
async fn hub_errors_map_to_sentences() {
    let hub = fake_hub().await;
    let (e, s, _p, app) = setup(Some(&hub)).await;
    token(&e).await;
    let uri = format!("/api/runs/{}/export/hub", s.run);
    let body = json!({"target": {"repo_id": "me/ds"}});
    for (status, code, text) in [
        (401, 400, "Hugging Face rejected the token. Check it in Settings."),
        (403, 400, "The token cannot write to me/ds. Use a token with write access to that account."),
        (404, 400, "Hugging Face could not find me/ds. Check the owner name in the repo id."),
        (500, 502, "Hugging Face returned an error (500). Try again in a moment."),
    ] {
        hub.cfg.lock().unwrap().create_status = Some(status);
        let r = post(&app, &uri, body.clone()).await;
        assert_eq!(r.status, code, "{status}");
        assert_eq!(r.json()["detail"], text);
    }
    hub.cfg.lock().unwrap().create_status = None;
    hub.cfg.lock().unwrap().commit_status = Some(409);
    let r = post(&app, &uri, body.clone()).await;
    assert_eq!(r.status, 409);
    assert_eq!(r.json()["detail"], "me/ds already exists in a way that conflicts. Pick another repo id.");
    assert!(e.leftovers().is_empty(), "{:?}", e.leftovers());
}

#[tokio::test]
async fn hub_unreachable_is_502() {
    let (e, s, _p, app) = {
        let dead = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", dead.local_addr().unwrap());
        drop(dead);
        let fake = FakeHub { url, seen: Default::default(), cfg: Default::default() };
        setup(Some(&fake)).await
    };
    token(&e).await;
    let r = post(&app, &format!("/api/runs/{}/export/hub", s.run), json!({"target": {"repo_id": "me/ds"}})).await;
    assert_eq!(r.status, 502);
    assert_eq!(r.json()["detail"], "Could not reach Hugging Face. Check the network connection and try again.");
    assert!(e.leftovers().is_empty());
}

#[tokio::test]
async fn hub_input_checks() {
    let hub = fake_hub().await;
    let (e, s, _p, app) = setup(Some(&hub)).await;
    token(&e).await;
    let uri = format!("/api/runs/{}/export/hub", s.run);
    let r = post(&app, &uri, json!({"target": {"repo_id": "me/ds", "split": "tr ain"}})).await;
    assert_eq!(r.status, 400);
    assert_eq!(r.json()["detail"], "The split name can only use letters, numbers, dots, dashes and underscores.");
    let r = post(&app, &uri, json!({"target": {"repo_id": "a/b/c"}})).await;
    assert_eq!(r.status, 400);
    assert_eq!(r.json()["detail"], "The repo id must look like owner/name, using letters, numbers, dashes and dots.");
    // Nothing to push: a filter that keeps no records.
    let r = post(&app, &uri, json!({"options": {"require_reasoning": true, "first_sample_only": true, "exclude_truncated": true}, "target": {"repo_id": "me/ds"}})).await;
    assert_eq!(r.status, 200); // two records still match
    sqlx::query("UPDATE samples SET status = 'failed'").execute(&e.ctx.db).await.unwrap();
    let before = hub.requests().len();
    let r = post(&app, &uri, json!({"target": {"repo_id": "me/ds"}})).await;
    assert_eq!(r.status, 400);
    assert_eq!(r.json()["detail"], "There are no records to push with these options.");
    assert_eq!(hub.requests().len(), before);
    // 422 for a too-short repo id
    let r = post(&app, &uri, json!({"target": {"repo_id": "ab"}})).await;
    assert_eq!(r.status, 422);
}
