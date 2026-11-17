//! Ports of backend/tests/test_imports_api.py plus the v2 background-import behaviour.

mod common;

use std::time::{Duration, Instant, SystemTime};

use axum::http::StatusCode;
use common::*;
use serde_json::json;
use stellarator_core::probes::ImportsProbe;
use stellarator_core::schemas::PromptSetStatus;

#[tokio::test]
async fn upload_preview_commit_csv() {
    let app = app().await;
    let up = app.upload_ok("../my prompts.csv", &std::fs::read(fixture("imports/bom_quoted.csv")).unwrap()).await;
    assert_eq!(up["filename"], "my prompts.csv");
    assert_eq!(up["detected_format"], "csv");
    assert!(app.upload_dir().join(up["upload_id"].as_str().unwrap()).join("my prompts.csv").is_file());
    let src = json!({"kind": "file", "upload_id": up["upload_id"], "format": "auto"});
    let (s, pv) = app.post("/api/imports/preview", json!({"source": src})).await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(pv["detected_format"], "csv");
    assert_eq!(pv["prompt_field"], "prompt");
    assert_eq!((pv["total_rows"].as_i64(), pv["estimated_prompts"].as_i64()), (Some(2), Some(2)));
    assert_eq!(pv["prompts"][0], json!([{"role": "user", "content": "Line one\nline two"}]));
    let kinds: Vec<_> = pv["columns"].as_array().unwrap().iter().map(|c| c["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["number", "text", "text"]);
    assert_eq!(pv["columns"][2]["looks_like_output"], true);
    assert_eq!(pv["dropped"], json!([]));
    assert_eq!(pv["has_conversations"], false);

    let (s, ps) = app.post("/api/imports", json!({"source": src, "name": "  My set  "})).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{ps}");
    assert_eq!(ps["status"], "importing");
    assert_eq!(ps["source_kind"], "file");
    assert_eq!(ps["name"], "My set");
    let set = app.wait(ps["id"].as_i64().unwrap()).await;
    assert_eq!(set.status, PromptSetStatus::Ready);
    assert_eq!((set.prompt_count, set.rows_read, set.rows_total), (2, 2, Some(2)));
    assert_eq!(set.status_detail, None);
    assert_eq!(set.source["filename"], "my prompts.csv");
    assert_eq!(set.source["prompt_field"], "prompt");
    assert_eq!(set.source["rows"], 2);
    // The upload is removed after a successful import.
    assert!(!app.upload_dir().join(up["upload_id"].as_str().unwrap()).exists());
    let stored = app.stored(set.id).await;
    assert_eq!(stored[1].0[0]["content"], "Say \"hi\"");
    assert_eq!(stored[0].1, json!({"id": "1", "answer": "a"}));
}

#[tokio::test]
async fn paste_preview_commit() {
    let app = app().await;
    let src = json!({"kind": "paste", "text": "# A\nx\n# B\ny"});
    let (_, pv) = app.post("/api/imports/preview", json!({"source": src})).await;
    assert_eq!(pv["split_mode"], "heading");
    assert_eq!(pv["prompts"].as_array().unwrap().len(), 2);
    let set = app.import(json!({"source": src, "name": "Pasted", "mapping": {"split_mode": "line"}})).await;
    assert_eq!(set.prompt_count, 4);
    assert_eq!(set.source_kind, "paste");
    assert_eq!(set.source["split_mode"], "line");
}

#[tokio::test]
async fn errors() {
    let app = app().await;
    let (s, v) = app.post("/api/imports/preview", json!({"source": {"kind": "file", "upload_id": "0".repeat(32)}})).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{v}");
    assert_eq!(v["detail"], "That upload was not found. It may have expired; upload the file again.");
    let (s, _) = app.post("/api/imports/preview", json!({"source": {"kind": "file", "upload_id": "../x"}})).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, v) = app.post("/api/imports", json!({"source": {"kind": "paste", "text": "   "}, "name": "x"})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["detail"], "Paste some text to import.");
    let up = app.upload_ok("a.csv", b"prompt\nhi\n").await;
    let src = json!({"kind": "file", "upload_id": up["upload_id"]});
    let (s, v) = app.post("/api/imports", json!({"source": src, "mapping": {"prompt_field": "zzz"}, "name": "x"})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["detail"].as_str().unwrap().contains("zzz"));
    assert_eq!(v["fields"]["mapping.prompt_field"], v["detail"]);
    // Nothing created for a failed import; the upload stays for a retry.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM prompt_sets").fetch_one(&app.ctx.db).await.unwrap();
    assert_eq!(n, 0);
    assert!(app.upload_dir().join(up["upload_id"].as_str().unwrap()).exists());
    let (s, v) = app.upload("e.txt", b"").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["detail"], "That file is empty.");
    // Validation from core: limit 0, empty name.
    let (s, v) = app.post("/api/imports", json!({"source": src, "mapping": {"limit": 0}, "name": ""})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["fields"]["mapping.limit"].is_string() && v["fields"]["name"].is_string(), "{v}");
    // No text column at all.
    let up = app.upload_ok("n.csv", b"a,b\n1,2\n3,4\n").await;
    let (s, v) = app.post("/api/imports", json!({"source": {"kind": "file", "upload_id": up["upload_id"]}, "name": "x"})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["detail"], "Couldn't find a text column to use as the prompt. Choose the column that holds the prompts.");
}

#[tokio::test]
async fn no_prompts_fails_in_background() {
    let app = app().await;
    // Rows exist but every prompt is empty: only known after reading the whole file.
    let up = app.upload_ok("e.csv", b"prompt,n\nhello there,1\n").await;
    let set = app
        .import(json!({"source": {"kind": "file", "upload_id": up["upload_id"]}, "mapping": {"offset": 5}, "name": "x"}))
        .await;
    assert_eq!(set.status, PromptSetStatus::Failed);
    assert_eq!(set.status_detail.as_deref(), Some("No prompts were found with these settings. Check the column or split mode and try again."));
    assert!(app.upload_dir().join(up["upload_id"].as_str().unwrap()).exists());
}

#[tokio::test]
async fn upload_too_large() {
    let app = app_with(None, Some(10)).await;
    let (s, v) = app.upload("big.txt", &[b'x'; 100]).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE, "{v}");
    assert!(v["detail"].as_str().unwrap().contains("1 GB"));
    let leftovers = std::fs::read_dir(app.upload_dir()).unwrap().count();
    assert_eq!(leftovers, 0);
}

#[tokio::test]
async fn upload_cleans_old() {
    let app = app().await;
    let old = app.upload_dir().join("a".repeat(32));
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("f.txt"), "x").unwrap();
    std::fs::File::open(&old).unwrap().set_modified(SystemTime::now() - Duration::from_secs(30 * 3600)).unwrap();
    app.upload_ok("n.txt", b"hello").await;
    assert!(!old.exists());
}

#[tokio::test]
async fn commit_100k_rows_fast() {
    let app = app().await;
    let mut data = String::from("prompt,n\n");
    for i in 0..100_000 {
        data.push_str(&format!("Question {i} here,{i}\n"));
    }
    let up = app.upload_ok("big.csv", data.as_bytes()).await;
    let t0 = Instant::now();
    let set = app.import(json!({"source": {"kind": "file", "upload_id": up["upload_id"]}, "name": "big"})).await;
    let elapsed = t0.elapsed();
    assert_eq!(set.status, PromptSetStatus::Ready, "{:?}", set.status_detail);
    assert_eq!(set.prompt_count, 100_000);
    assert!(elapsed < Duration::from_secs(20), "{elapsed:?}");
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM prompts WHERE prompt_set_id = ?").bind(set.id).fetch_one(&app.ctx.db).await.unwrap();
    assert_eq!(n, 100_000);
    let (min, max): (i64, i64) =
        sqlx::query_as("SELECT MIN(idx), MAX(idx) FROM prompts WHERE prompt_set_id = ?").bind(set.id).fetch_one(&app.ctx.db).await.unwrap();
    assert_eq!((min, max), (0, 99_999));
}

#[tokio::test]
async fn cancel_stops_promptly_and_recover_marks_failed() {
    let app = app().await;
    let mut data = String::from("prompt\n");
    for i in 0..400_000 {
        data.push_str(&format!("Prompt number {i}\n"));
    }
    let up = app.upload_ok("big.csv", data.as_bytes()).await;
    let (s, v) = app.post("/api/imports", json!({"source": {"kind": "file", "upload_id": up["upload_id"]}, "name": "big"})).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let id = v["id"].as_i64().unwrap();
    assert!(app.imports.is_importing(id));
    tokio::time::sleep(Duration::from_millis(30)).await;
    let t0 = Instant::now();
    app.imports.cancel(id).await;
    assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
    assert!(!app.imports.is_importing(id));
    let set = stellarator_import::load_set(&app.ctx.db, id).await.unwrap();
    assert_eq!(set.status, PromptSetStatus::Importing, "left for the caller to delete");
    let written: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM prompts WHERE prompt_set_id = ?").bind(id).fetch_one(&app.ctx.db).await.unwrap();
    assert!(written < 400_000);
    // Nothing more is written after cancel resolved.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let again: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM prompts WHERE prompt_set_id = ?").bind(id).fetch_one(&app.ctx.db).await.unwrap();
    assert_eq!(written, again);
    // Cancelling an unknown set is a no-op.
    app.imports.cancel(9999).await;
    // A restart marks sets left importing as failed.
    app.imports.recover().await.unwrap();
    let set = stellarator_import::load_set(&app.ctx.db, id).await.unwrap();
    assert_eq!(set.status, PromptSetStatus::Failed);
    assert_eq!(set.status_detail.as_deref(), Some(stellarator_import::RESTARTED));
}

#[tokio::test]
async fn progress_is_reported_while_importing() {
    let app = app().await;
    let mut data = String::new();
    for i in 0..300_000 {
        data.push_str(&format!("{{\"prompt\": \"Prompt {i} with some words\", \"n\": {i}}}\n"));
    }
    let up = app.upload_ok("p.jsonl", data.as_bytes()).await;
    let (s, v) = app.post("/api/imports", json!({"source": {"kind": "file", "upload_id": up["upload_id"]}, "name": "p"})).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    assert_eq!(v["status_detail"], "Reading p.jsonl");
    let id = v["id"].as_i64().unwrap();
    let mut saw_partial = false;
    loop {
        let set = stellarator_import::load_set(&app.ctx.db, id).await.unwrap();
        if set.status != PromptSetStatus::Importing {
            assert_eq!(set.prompt_count, 300_000);
            break;
        }
        if set.prompt_count > 0 && set.prompt_count < 300_000 {
            saw_partial = true;
            assert!(set.rows_read >= set.prompt_count);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(saw_partial, "progress was never visible");
}

#[tokio::test]
async fn shutdown_waits_for_jobs() {
    let app = app().await;
    let mut data = String::from("prompt\n");
    for i in 0..300_000 {
        data.push_str(&format!("P {i}\n"));
    }
    let up = app.upload_ok("s.csv", data.as_bytes()).await;
    let (_, v) = app.post("/api/imports", json!({"source": {"kind": "file", "upload_id": up["upload_id"]}, "name": "s"})).await;
    let id = v["id"].as_i64().unwrap();
    app.imports.shutdown().await;
    assert!(!app.imports.is_importing(id));
}
