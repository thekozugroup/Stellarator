#![allow(dead_code)]
//! Shared test helpers: a temp-dir database, SQL seeding, request helpers and a fake Hub.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use bytes::Bytes;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use stellarator_core::config::{Config, Mode};
use stellarator_core::Ctx;
use tower::ServiceExt;

pub struct Env {
    pub ctx: Ctx,
    pub dir: tempfile::TempDir,
}

pub async fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::new(dir.path().to_path_buf(), Mode::Native);
    config.env_hf_token = None;
    let ctx = Ctx::open(config).await.unwrap();
    Env { ctx, dir }
}

impl Env {
    pub fn app(&self) -> Router {
        stellarator_export::router(self.ctx.clone())
    }
    pub fn leftovers(&self) -> Vec<String> {
        std::fs::read_dir(self.ctx.config.export_dir())
            .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into()).collect())
            .unwrap_or_default()
    }
}

pub struct Resp {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Bytes,
}

impl Resp {
    pub fn text(&self) -> String {
        String::from_utf8(self.body.to_vec()).unwrap()
    }
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| panic!("not json ({e}): {}", self.text()))
    }
    pub fn header(&self, name: &str) -> String {
        self.headers.get(name).map(|v| v.to_str().unwrap().to_string()).unwrap_or_default()
    }
}

pub async fn send(app: &Router, method: &str, uri: &str, body: Option<Value>) -> Resp {
    let mut b = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            b = b.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(b.body(body).unwrap()).await.unwrap();
    let (parts, body) = resp.into_parts();
    Resp { status: parts.status, headers: parts.headers, body: body.collect().await.unwrap().to_bytes() }
}

pub async fn post(app: &Router, uri: &str, body: Value) -> Resp {
    send(app, "POST", uri, Some(body)).await
}
pub async fn get(app: &Router, uri: &str) -> Resp {
    send(app, "GET", uri, None).await
}

pub const TS: &str = "2026-01-01 00:00:00.000000";

pub struct Seed {
    pub ps: i64,
    pub run: i64,
    pub prompts: Vec<i64>,
    pub samples: Vec<i64>,
}

pub async fn prompt_set(db: &SqlitePool, name: &str, n: i64) -> i64 {
    sqlx::query("INSERT INTO prompt_sets (name, source_kind, source, prompt_count, created_at) VALUES (?, 'paste', '{}', ?, ?)")
        .bind(name)
        .bind(n)
        .bind(TS)
        .execute(db)
        .await
        .unwrap()
        .last_insert_rowid()
}

pub async fn add_prompt(db: &SqlitePool, ps: i64, idx: i64, system: &str, user: &str) -> i64 {
    let messages = json!([{"role": "system", "content": system}, {"role": "user", "content": user}]);
    sqlx::query("INSERT INTO prompts (prompt_set_id, idx, messages, meta) VALUES (?, ?, ?, ?)")
        .bind(ps)
        .bind(idx)
        .bind(messages.to_string())
        .bind(json!({"ref": idx}).to_string())
        .execute(db)
        .await
        .unwrap()
        .last_insert_rowid()
}

pub async fn add_run(db: &SqlitePool, ps: i64, name: &str, params: Value) -> i64 {
    sqlx::query(
        "INSERT INTO runs (name, endpoint_name, model, prompt_set_id, params, concurrency, status, total, done, failed, \
         truncated, prompt_tokens, completion_tokens, reasoning_tokens, active_seconds, created_at) \
         VALUES (?, 'spark', 'qwen3-32b', ?, ?, 4, 'completed', 0, 0, 0, 0, 0, 0, 0, 0.0, ?)",
    )
    .bind(name)
    .bind(ps)
    .bind(params.to_string())
    .bind(TS)
    .execute(db)
    .await
    .unwrap()
    .last_insert_rowid()
}

#[derive(Clone)]
pub struct S<'a> {
    pub status: &'a str,
    pub reasoning: Option<&'a str>,
    pub response: Option<&'a str>,
    pub finish: &'a str,
}

pub async fn add_sample(db: &SqlitePool, run: i64, prompt: i64, pidx: i64, sidx: i64, s: S<'_>) -> i64 {
    sqlx::query(
        "INSERT INTO samples (run_id, prompt_id, prompt_idx, sample_idx, status, reasoning, response, finish_reason, \
         prompt_tokens, completion_tokens, reasoning_tokens, latency_ms, ttft_ms, attempts, error, completed_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 5, 9, 3, 50, 5, 1, NULL, ?)",
    )
    .bind(run)
    .bind(prompt)
    .bind(pidx)
    .bind(sidx)
    .bind(s.status)
    .bind(s.reasoning)
    .bind(s.response)
    .bind(s.finish)
    .bind(TS)
    .execute(db)
    .await
    .unwrap()
    .last_insert_rowid()
}

/// The same fixture as the Python API tests.
pub async fn seed(db: &SqlitePool) -> Seed {
    let ps = prompt_set(db, "GSM8K sample", 4).await;
    let mut prompts = Vec::new();
    for i in 0..4 {
        prompts.push(add_prompt(db, ps, i, "sys", &format!("q{i}")).await);
    }
    let run = add_run(db, ps, "Qwen3 32B on GSM8K", json!({"temperature": 0.7, "user_template": "Solve: {prompt}"})).await;
    let mut samples = Vec::new();
    let specs: Vec<(usize, i64, String, Option<String>, Option<String>, &str)> = vec![
        (0, 0, "done".into(), Some("think 0-0".into()), Some("answer 0-0 é".into()), "stop"),
        (0, 1, "done".into(), Some("think 0-1".into()), Some("answer 0-1 é".into()), "stop"),
        (1, 0, "done".into(), Some("think 1-0".into()), Some("answer 1-0 é".into()), "length"),
        (2, 0, "done".into(), None, Some("answer 2-0 é".into()), "stop"),
        (3, 0, "failed".into(), None, None, "stop"),
        (3, 1, "pending".into(), None, None, "stop"),
        (3, 2, "done".into(), Some("".into()), Some("".into()), "stop"),
    ];
    for (p, si, status, reasoning, response, finish) in specs {
        let id = add_sample(
            db,
            run,
            prompts[p],
            p as i64,
            si,
            S { status: &status, reasoning: reasoning.as_deref(), response: response.as_deref(), finish },
        )
        .await;
        samples.push(id);
    }
    Seed { ps, run, prompts, samples }
}

pub async fn seed_pref(db: &SqlitePool, s: &Seed) -> i64 {
    let id = sqlx::query("INSERT INTO preference_sets (name, prompt_set_id, run_ids, created_at) VALUES ('GSM8K prefs', ?, ?, ?)")
        .bind(s.ps)
        .bind(json!([s.run]).to_string())
        .bind(TS)
        .execute(db)
        .await
        .unwrap()
        .last_insert_rowid();
    for (prompt, chosen, rejected) in [(s.prompts[0], s.samples[0], s.samples[1]), (s.prompts[2], s.samples[3], s.samples[2])] {
        sqlx::query(
            "INSERT INTO preference_pairs (preference_set_id, prompt_id, chosen_sample_id, rejected_sample_id, source, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 'manual', ?, ?)",
        )
        .bind(id)
        .bind(prompt)
        .bind(chosen)
        .bind(rejected)
        .bind(TS)
        .bind(TS)
        .execute(db)
        .await
        .unwrap();
    }
    id
}

// ------------------------------------------------------------------------------ parquet

/// Read a Parquet file back as JSON records (nulls omitted, like pyarrow's to_pylist after
/// `strip_none`).
pub fn read_parquet(data: &[u8]) -> Vec<Value> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::copy_from_slice(data)).unwrap().build().unwrap();
    let mut w = arrow_json::LineDelimitedWriter::new(Vec::new());
    for b in reader {
        w.write(&b.unwrap()).unwrap();
    }
    w.finish().unwrap();
    String::from_utf8(w.into_inner())
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

pub fn parquet_rows(data: &[u8]) -> usize {
    use parquet::file::reader::{FileReader, SerializedFileReader};
    SerializedFileReader::new(Bytes::copy_from_slice(data)).unwrap().metadata().file_metadata().num_rows() as usize
}

// ------------------------------------------------------------------------------ fake hub

#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub auth: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Default)]
pub struct HubCfg {
    pub create_status: Option<u16>,
    pub commit_status: Option<u16>,
    pub lfs: bool,
}

pub struct FakeHub {
    pub url: String,
    pub seen: Arc<Mutex<Vec<Seen>>>,
    pub cfg: Arc<Mutex<HubCfg>>,
}

impl FakeHub {
    pub fn requests(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
    pub fn calls(&self) -> Vec<String> {
        self.requests().iter().map(|r| format!("{} {}", r.method, r.path)).collect()
    }
}

pub async fn fake_hub() -> FakeHub {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen: Arc<Mutex<Vec<Seen>>> = Default::default();
    let cfg: Arc<Mutex<HubCfg>> = Default::default();
    let (s2, c2, base) = (seen.clone(), cfg.clone(), url.clone());
    let app = Router::new().fallback(move |req: Request<Body>| {
        let (seen, cfg, base) = (s2.clone(), c2.clone(), base.clone());
        async move {
            let (parts, body) = req.into_parts();
            let body = body.collect().await.unwrap().to_bytes().to_vec();
            let path = parts.uri.path().to_string();
            let method = parts.method.to_string();
            let auth = parts.headers.get("authorization").and_then(|v| v.to_str().ok()).map(String::from);
            seen.lock().unwrap().push(Seen { method: method.clone(), path: path.clone(), auth, body: body.clone() });
            let (create, commit, lfs) = {
                let c = cfg.lock().unwrap();
                (c.create_status, c.commit_status, c.lfs)
            };
            let ok = |v: Value| (StatusCode::OK, axum::Json(v));
            let fail = |s: u16| (StatusCode::from_u16(s).unwrap(), axum::Json(json!({"error": "boom"})));
            if path == "/api/repos/create" {
                return match create {
                    Some(s) => fail(s),
                    None => ok(json!({"url": "x"})),
                };
            }
            if path == "/api/whoami-v2" {
                return ok(json!({"name": "me"}));
            }
            if path.ends_with("/preupload/main") {
                let v: Value = serde_json::from_slice(&body).unwrap();
                let p = v["files"][0]["path"].as_str().unwrap().to_string();
                let mode = if lfs && p.ends_with(".parquet") { "lfs" } else { "regular" };
                return ok(json!({"files": [{"path": p, "uploadMode": mode, "shouldIgnore": false}]}));
            }
            if path.ends_with("/commit/main") {
                return match commit {
                    Some(s) => fail(s),
                    None => ok(json!({"commitUrl": "x"})),
                };
            }
            if path.ends_with("/info/lfs/objects/batch") {
                let v: Value = serde_json::from_slice(&body).unwrap();
                let o = &v["objects"][0];
                return ok(json!({"transfer": "basic", "objects": [{
                    "oid": o["oid"], "size": o["size"],
                    "actions": {
                        "upload": {"href": format!("{base}/lfs-put/{}", o["oid"].as_str().unwrap()), "header": {"x-test": "1"}},
                        "verify": {"href": format!("{base}/lfs-verify"), "header": {}},
                    }
                }]}));
            }
            ok(json!({}))
        }
    });
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    FakeHub { url, seen, cfg }
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn ndjson(body: &[u8]) -> Vec<Value> {
    std::str::from_utf8(body).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}
