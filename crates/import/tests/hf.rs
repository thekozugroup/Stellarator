//! Ports of backend/tests/test_hf.py and the Hugging Face parts of test_imports_api.py, against a
//! mock Hugging Face on 127.0.0.1:0, plus v2 whole-dataset and all-splits imports.

mod common;

use std::sync::Arc;

use arrow_array::builder::{ListBuilder, StringBuilder, StructBuilder};
use arrow_array::{ArrayRef, Int64Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Fields};
use axum::http::StatusCode;
use common::*;
use serde_json::{json, Value};
use stellarator_core::schemas::PromptSetStatus;
use stellarator_import::hf::{choose_split, normalize_dataset};

fn question_features() -> Value {
    json!([
        {"feature_idx": 0, "name": "question", "type": {"dtype": "string", "_type": "Value"}},
        {"feature_idx": 1, "name": "img", "type": {"_type": "Image"}},
    ])
}

fn question_rows(n: usize) -> Vec<Value> {
    (0..n).map(|i| json!({"question": format!("Q{i}"), "img": {"src": "x"}})).collect()
}

/// question/response shards: rows `start..start + n`.
fn qa_parquet(start: usize, n: usize, row_group: usize) -> Vec<u8> {
    qa_parquet_sized(start, n, row_group, 2000)
}

/// `answer_len`: bytes of model response per row (between the question and n columns).
fn qa_parquet_sized(start: usize, n: usize, row_group: usize, answer_len: usize) -> Vec<u8> {
    let batch = RecordBatch::try_from_iter(vec![
        ("question", Arc::new(StringArray::from_iter_values((start..start + n).map(|i| format!("Q{i}")))) as ArrayRef),
        ("response", Arc::new(StringArray::from_iter_values((start..start + n).map(|i| format!("A{i} {}", "y".repeat(answer_len))))) as ArrayRef),
        ("n", Arc::new(Int64Array::from_iter_values((start..start + n).map(|i| i as i64))) as ArrayRef),
    ])
    .unwrap();
    parquet_bytes(&[batch], row_group)
}

#[test]
fn normalize_names() {
    assert_eq!(normalize_dataset("openai/gsm8k").unwrap(), "openai/gsm8k");
    assert_eq!(normalize_dataset("https://huggingface.co/datasets/openai/gsm8k").unwrap(), "openai/gsm8k");
    assert_eq!(normalize_dataset("https://huggingface.co/datasets/openai/gsm8k/viewer/main/train?x=1").unwrap(), "openai/gsm8k");
    assert_eq!(normalize_dataset("huggingface.co/datasets/squad").unwrap(), "squad");
    assert_eq!(normalize_dataset(" squad ").unwrap(), "squad");
    let e = normalize_dataset("not a name!").unwrap_err();
    assert_eq!(e.status, 422);
    assert_eq!(e.field, Some("source.dataset"));
}

#[test]
fn split_choice() {
    let s = |c: &str, sp: &str| stellarator_core::schemas::HfSplit { config: c.into(), split: sp.into(), num_rows: None };
    let splits = vec![s("main", "test"), s("main", "train"), s("socratic", "train")];
    assert_eq!(choose_split("o/d", &splits, None, None, false).unwrap(), ("main".into(), vec!["train".into()], "train".into()));
    assert_eq!(
        choose_split("o/d", &splits, Some("main"), None, true).unwrap(),
        ("main".into(), vec!["test".into(), "train".into()], "train".into())
    );
    let e = choose_split("o/d", &splits, Some("x"), None, false).unwrap_err();
    assert_eq!(e.message, "o/d has no config called 'x'. Configs are: main, socratic.");
    let e = choose_split("o/d", &splits, Some("socratic"), Some("test"), false).unwrap_err();
    assert_eq!(e.message, "o/d (socratic) has no split called 'test'. Splits are: train.");
}

#[tokio::test]
async fn preview_defaults_and_sizes() {
    let hf = MockHf::start().await;
    hf.dataset(&[("train", 100), ("test", 10)], question_features(), question_rows(5));
    let app = app_with(Some(&hf.base), None).await;
    let (s, pv) = app
        .post("/api/imports/preview", json!({"source": {"kind": "huggingface", "dataset": "https://huggingface.co/datasets/o/d"}, "mapping": {"limit": 3}}))
        .await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!((pv["hf_config"].as_str(), pv["hf_split"].as_str()), (Some("main"), Some("train")));
    assert_eq!(pv["hf_splits"], json!([{"config": "main", "split": "train", "num_rows": 100}, {"config": "main", "split": "test", "num_rows": 10}]));
    assert_eq!(pv["prompt_field"], "question");
    assert_eq!((pv["total_rows"].as_i64(), pv["estimated_prompts"].as_i64()), (Some(100), Some(3)));
    let names: Vec<_> = pv["columns"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["question"]);
    // No token: no Authorization header.
    assert!(hf.hits().iter().all(|h| h.auth.is_none()));
    // All splits: rows of every split count.
    let (_, pv) = app.post("/api/imports/preview", json!({"source": {"kind": "huggingface", "dataset": "o/d", "all_splits": true}})).await;
    assert_eq!(pv["total_rows"], 110);
    assert_eq!(pv["estimated_prompts"], 110);
}

#[tokio::test]
async fn size_failure_ignored_and_token_header() {
    let hf = MockHf::start().await;
    hf.dataset(&[("train", 100), ("test", 10)], question_features(), question_rows(5));
    hf.json("/size", status(500, json!({"error": "boom"})));
    let app = app_with(Some(&hf.base), None).await;
    stellarator_core::settings::set(&app.ctx.db, "hf_token", Some(json!("secret"))).await.unwrap();
    let (s, pv) = app
        .post("/api/imports/preview", json!({"source": {"kind": "huggingface", "dataset": "o/d", "config": "main", "split": "test"}}))
        .await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(pv["hf_split"], "test");
    assert_eq!(pv["total_rows"], Value::Null);
    assert_eq!(hf.hits()[0].auth.as_deref(), Some("Bearer secret"));
}

#[tokio::test]
async fn errors_map_to_messages() {
    let hf = MockHf::start().await;
    let app = app_with(Some(&hf.base), None).await;
    let preview = |ds: &str| json!({"source": {"kind": "huggingface", "dataset": ds}});

    hf.json("/splits", status(404, json!({"error": "nope"})));
    let (s, v) = app.post("/api/imports/preview", preview("o/missing")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(v["detail"], "No dataset called o/missing on Hugging Face. Check the name.");
    assert_eq!(v["fields"]["source.dataset"], v["detail"]);

    // Gated: the datasets-server says 401, but the Hub API shows the dataset.
    hf.json("/splits", status(401, json!({"error": "gated"})));
    hf.json("/api/datasets/o/gated", ok(json!({"id": "o/gated"})));
    let (s, v) = app.post("/api/imports/preview", preview("o/gated")).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(v["detail"], "This dataset needs a Hugging Face token with access. Add one in Settings.");

    // Missing: both say 401.
    hf.json("/api/datasets/o/none", status(401, json!({"error": "nope"})));
    let (s, v) = app.post("/api/imports/preview", preview("o/none")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(v["detail"].as_str().unwrap().starts_with("No dataset called o/none"));

    // Rate limited by the datasets-server.
    hf.json("/splits", Canned { status: 429, body: json!({}), headers: vec![("retry-after", "150".into())] });
    let (s, v) = app.post("/api/imports/preview", preview("o/d")).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(v["detail"], "Hugging Face is limiting requests right now. Try again in about 3 minutes.");

    hf.dataset(&[("train", 100)], question_features(), question_rows(5));
    let (s, v) = app.post("/api/imports/preview", json!({"source": {"kind": "huggingface", "dataset": "o/d", "config": "other"}})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["detail"].as_str().unwrap().contains("no config"));

    let (s, v) = app.post("/api/imports/preview", preview("bad name!")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["detail"].as_str().unwrap().starts_with("That doesn't look like a dataset name."));

    // Unreachable server.
    let down = app_with(Some("http://127.0.0.1:9"), None).await;
    let (s, v) = down.post("/api/imports/preview", preview("o/d")).await;
    assert_eq!(s, StatusCode::BAD_GATEWAY, "{v}");
    assert_eq!(v["detail"], "Couldn't reach Hugging Face. Check this machine's internet connection.");
}

#[tokio::test]
async fn no_parquet_export() {
    let hf = MockHf::start().await;
    hf.dataset(&[("train", 100)], question_features(), question_rows(5));
    let app = app_with(Some(&hf.base), None).await;
    let commit = json!({"source": {"kind": "huggingface", "dataset": "o/d"}, "name": "x"});
    hf.json("/parquet", status(404, json!({"error": "no parquet"})));
    let (s, v) = app.post("/api/imports", commit.clone()).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["detail"], "Hugging Face can't provide a Parquet export for o/d yet (no parquet).");
    hf.json("/parquet", ok(json!({"parquet_files": [], "pending": [], "failed": []})));
    let (_, v) = app.post("/api/imports", commit.clone()).await;
    assert_eq!(v["detail"], "Hugging Face has no Parquet export for o/d (main, train), so it can't be read yet.");
    hf.json("/parquet", ok(json!({"parquet_files": [], "pending": [{"config": "main"}], "failed": []})));
    let (s, _) = app.post("/api/imports", commit).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM prompt_sets").fetch_one(&app.ctx.db).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn rows_across_shards_with_window_and_projection() {
    let hf = MockHf::start().await;
    hf.dataset(&[("train", 20)], json!([{"name": "question", "type": {"dtype": "string", "_type": "Value"}}, {"name": "response", "type": {"dtype": "string", "_type": "Value"}}, {"name": "n", "type": {"dtype": "int64", "_type": "Value"}}]), (0..5).map(|i| json!({"question": format!("Q{i}"), "response": "A", "n": i})).collect());
    // Listed out of order: sorted by file name.
    // ~300 KB of output per row: each row group's answer chunk is wider than the 1 MiB gap that
    // ranges are merged across.
    let (a, b) = (qa_parquet_sized(0, 10, 4, 300_000), qa_parquet_sized(10, 10, 4, 300_000));
    let total = a.len() + b.len();
    hf.parquet(vec![("train", "0001.parquet", b), ("train", "0000.parquet", a)]);
    let app = app_with(Some(&hf.base), None).await;
    hf.clear_hits();
    let set = app
        .import(json!({"source": {"kind": "huggingface", "dataset": "o/d"}, "mapping": {"offset": 8, "limit": 6}, "name": "w"}))
        .await;
    assert_eq!(set.status, PromptSetStatus::Ready, "{:?}", set.status_detail);
    let stored = app.stored(set.id).await;
    let got: Vec<_> = stored.iter().map(|(m, _)| m[0]["content"].as_str().unwrap().to_string()).collect();
    assert_eq!(got, ["Q8", "Q9", "Q10", "Q11", "Q12", "Q13"]);
    assert_eq!(stored[0].1, json!({"n": 8}));
    assert_eq!(set.rows_total, Some(6));
    assert_eq!(set.source["dataset"], "o/d");
    assert_eq!(set.source["split"], "train");
    assert_eq!(set.source["prompt_field"], "question");
    // The "response" column (~300 KB per row) is a generation: never downloaded.
    let file_bytes: usize = hf.hits().iter().filter(|h| h.path.starts_with("/cdn/")).map(|h| h.bytes).sum();
    assert!(file_bytes * 20 < total, "downloaded {file_bytes} of {total} bytes");
    // Each file is resolved once.
    let resolves = hf.hits().iter().filter(|h| h.method == "HEAD").count();
    assert_eq!(resolves, 2);
}

#[tokio::test]
async fn all_splits_in_order_with_progress_detail() {
    let hf = MockHf::start().await;
    hf.dataset(&[("train", 20), ("test", 10)], question_features(), question_rows(5));
    hf.parquet(vec![
        ("train", "0000.parquet", qa_parquet(0, 10, 5)),
        ("train", "0001.parquet", qa_parquet(10, 10, 5)),
        ("test", "0000.parquet", qa_parquet(100, 10, 5)),
    ]);
    let app = app_with(Some(&hf.base), None).await;
    let (s, v) = app.post("/api/imports", json!({"source": {"kind": "huggingface", "dataset": "o/d", "all_splits": true}, "name": "all"})).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    assert_eq!(v["rows_total"], 30);
    assert_eq!(v["status_detail"], "Reading train");
    let set = app.wait(v["id"].as_i64().unwrap()).await;
    assert_eq!(set.status, PromptSetStatus::Ready, "{:?}", set.status_detail);
    assert_eq!(set.prompt_count, 30);
    assert_eq!(set.source["splits"], json!(["train", "test"]));
    let stored = app.stored(set.id).await;
    assert_eq!(stored[19].0[0]["content"], "Q19");
    assert_eq!(stored[20].0[0]["content"], "Q100");
    // An offset past the first split skips it without opening its files.
    hf.clear_hits();
    let set = app
        .import(json!({"source": {"kind": "huggingface", "dataset": "o/d", "all_splits": true}, "mapping": {"offset": 22}, "name": "o"}))
        .await;
    let stored = app.stored(set.id).await;
    assert_eq!(stored.len(), 8);
    assert_eq!(stored[0].0[0]["content"], "Q102");
    assert!(!hf.hits().iter().any(|h| h.path.contains("/train/0001")), "{:?}", hf.hits());
}

#[tokio::test]
async fn conversations_first_user_from_parquet() {
    let fields = Fields::from(vec![Field::new("content", DataType::Utf8, true), Field::new("role", DataType::Utf8, true)]);
    let mut b = ListBuilder::new(StructBuilder::from_fields(fields, 16));
    for i in 0..6 {
        let s = b.values();
        for (role, content) in [("user", format!("Hello {i}")), ("assistant", format!("<think>hmm</think>Hi {i}")), ("user", format!("More {i}")), ("assistant", "Sure.".to_string())] {
            s.field_builder::<StringBuilder>(0).unwrap().append_value(content);
            s.field_builder::<StringBuilder>(1).unwrap().append_value(role);
            s.append(true);
        }
        b.append(true);
    }
    let batch = RecordBatch::try_from_iter(vec![
        ("prompt", Arc::new(StringArray::from_iter_values((0..6).map(|i| format!("Hello {i}")))) as ArrayRef),
        ("prompt_id", Arc::new(StringArray::from_iter_values((0..6).map(|i| format!("{i:08x}")))) as ArrayRef),
        ("messages", Arc::new(b.finish()) as ArrayRef),
    ])
    .unwrap();
    let hf = MockHf::start().await;
    let first: Vec<Value> = (0..3)
        .map(|i| {
            json!({"prompt": format!("Hello {i}"), "prompt_id": format!("{i:08x}"), "messages": [
                {"content": format!("Hello {i}"), "role": "user"}, {"content": "<think>hmm</think>Hi", "role": "assistant"},
                {"content": format!("More {i}"), "role": "user"}, {"content": "Sure.", "role": "assistant"}]})
        })
        .collect();
    hf.dataset(
        &[("test_sft", 6)],
        json!([{"name": "prompt", "type": {"dtype": "string", "_type": "Value"}}, {"name": "prompt_id", "type": {"dtype": "string", "_type": "Value"}}, {"name": "messages", "type": [{"content": {"dtype": "string", "_type": "Value"}, "role": {"dtype": "string", "_type": "Value"}}]}]),
        first,
    );
    hf.parquet(vec![("test_sft", "0000.parquet", parquet_bytes(&[batch], 4))]);
    let app = app_with(Some(&hf.base), None).await;
    // Choose the conversation column: turns apply.
    let m = json!({"prompt_field": "messages", "turns": "first_user"});
    let (_, pv) = app.post("/api/imports/preview", json!({"source": {"kind": "huggingface", "dataset": "o/d"}, "mapping": m})).await;
    assert_eq!(pv["has_conversations"], true);
    assert_eq!(pv["dropped"], json!(["assistant turns in messages"]));
    assert_eq!(pv["prompts"][0], json!([{"role": "user", "content": "Hello 0"}]));
    let set = app.import(json!({"source": {"kind": "huggingface", "dataset": "o/d"}, "mapping": m, "name": "c"})).await;
    assert_eq!(set.prompt_count, 6);
    for (messages, meta) in app.stored(set.id).await {
        assert_eq!(messages.as_array().unwrap().len(), 1);
        assert!(!messages.to_string().contains("think"));
        assert!(meta.get("prompt_id").is_some());
    }
    // up_to_last_user keeps the earlier exchange but not the final answer.
    let set = app
        .import(json!({"source": {"kind": "huggingface", "dataset": "o/d"}, "mapping": {"prompt_field": "messages"}, "name": "c2"}))
        .await;
    let stored = app.stored(set.id).await;
    let roles: Vec<_> = stored[0].0.as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap().to_string()).collect();
    assert_eq!(roles, ["user", "assistant", "user"]);
    assert!(!stored.iter().any(|(m, _)| m.to_string().contains("Sure.")));
    // A preview past the first rows reads the window from the Parquet files.
    let (_, pv) = app
        .post("/api/imports/preview", json!({"source": {"kind": "huggingface", "dataset": "o/d"}, "mapping": {"offset": 4, "turns": "first_user"}}))
        .await;
    assert_eq!(pv["prompt_field"], "prompt");
    assert_eq!(pv["prompts"], json!([[{"role": "user", "content": "Hello 4"}], [{"role": "user", "content": "Hello 5"}]]));
    assert_eq!(pv["estimated_prompts"], 2);
}

#[tokio::test]
async fn rate_limited_fails_fast() {
    let hf = MockHf::start().await;
    hf.dataset(&[("train", 10)], question_features(), question_rows(5));
    hf.parquet(vec![("train", "0000.parquet", qa_parquet(0, 10, 5))]);
    hf.state.lock().unwrap().resolve =
        Some(Canned { status: 429, body: json!({}), headers: vec![("ratelimit", "\"resolvers\";r=0;t=136".into())] });
    let app = app_with(Some(&hf.base), None).await;
    let t0 = std::time::Instant::now();
    let (s, v) = app.post("/api/imports", json!({"source": {"kind": "huggingface", "dataset": "o/d"}, "name": "x"})).await;
    assert!(t0.elapsed().as_secs() < 5);
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "{v}");
    assert_eq!(
        v["detail"],
        "Hugging Face is limiting downloads from this machine. Try again in about 3 minutes. Adding a Hugging Face token in Settings raises the limit."
    );
    // With a token, no hint.
    stellarator_core::settings::set(&app.ctx.db, "hf_token", Some(json!("hf_x"))).await.unwrap();
    let (_, v) = app.post("/api/imports", json!({"source": {"kind": "huggingface", "dataset": "o/d"}, "name": "x"})).await;
    assert_eq!(v["detail"], "Hugging Face is limiting downloads from this machine. Try again in about 3 minutes.");
    // Gated file: 403 on resolve.
    hf.state.lock().unwrap().resolve = Some(status(403, json!({})));
    let (s, v) = app.post("/api/imports", json!({"source": {"kind": "huggingface", "dataset": "o/d"}, "name": "x"})).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert!(v["detail"].as_str().unwrap().starts_with("Your Hugging Face token can't read this dataset."));
}

#[tokio::test]
async fn truncated_first_rows_cells_are_ignored() {
    let hf = MockHf::start().await;
    hf.dataset(&[("train", 3)], json!([{"name": "messages", "type": []}, {"name": "id", "type": {"dtype": "int64", "_type": "Value"}}]), vec![]);
    let rows = json!([
        {"row_idx": 0, "row": {"messages": [{"role": "user", "content": "Full question"}, {"role": "assistant", "content": "x"}], "id": 0}, "truncated_cells": []},
        {"row_idx": 1, "row": {"messages": "[{\"role\":\"user\",\"content\":\"Trunc", "id": 1}, "truncated_cells": ["messages"]},
    ]);
    hf.json("/first-rows", ok(json!({"features": [{"name": "messages", "type": []}, {"name": "id", "type": {"dtype": "int64", "_type": "Value"}}], "rows": rows})));
    let app = app_with(Some(&hf.base), None).await;
    let (_, pv) = app.post("/api/imports/preview", json!({"source": {"kind": "huggingface", "dataset": "o/d"}})).await;
    assert_eq!(pv["columns"][0]["kind"], "messages");
    assert_eq!(pv["prompt_field"], "messages");
    assert_eq!(pv["prompts"], json!([[{"role": "user", "content": "Full question"}]]));
}

#[tokio::test]
async fn first_rows_missing_falls_back_to_parquet() {
    let hf = MockHf::start().await;
    hf.dataset(&[("train", 10)], question_features(), question_rows(1));
    hf.json("/first-rows", status(500, json!({"error": "The size of the content of the first rows exceeds the maximum supported size."})));
    hf.parquet(vec![("train", "0000.parquet", qa_parquet(0, 10, 5))]);
    let app = app_with(Some(&hf.base), None).await;
    let (s, pv) = app.post("/api/imports/preview", json!({"source": {"kind": "huggingface", "dataset": "o/d"}})).await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(pv["prompt_field"], "question");
    assert_eq!(pv["prompts"].as_array().unwrap().len(), 10);
    assert_eq!(pv["dropped"], json!(["response"]));
    let set = app.import(json!({"source": {"kind": "huggingface", "dataset": "o/d"}, "name": "f"})).await;
    assert_eq!(set.prompt_count, 10);
    // Without a Parquet export either, the first-rows sentence stands.
    hf.json("/parquet", ok(json!({"parquet_files": [], "pending": [], "failed": []})));
    let (s, v) = app.post("/api/imports/preview", json!({"source": {"kind": "huggingface", "dataset": "o/d"}})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["detail"].as_str().unwrap().starts_with("Hugging Face can't provide a preview for o/d yet"), "{v}");
}
