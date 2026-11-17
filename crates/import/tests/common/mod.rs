//! Shared helpers for the integration tests: an app on a temp data dir, request helpers, a Parquet
//! writer and a mock Hugging Face (datasets-server, Hub API, resolve redirects and a CDN that
//! honours Range requests) on 127.0.0.1:0.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arrow_array::RecordBatch;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;
use http_body_util::BodyExt;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use serde_json::Value;
use stellarator_core::config::Mode;
use stellarator_core::schemas::{ImportFormat, ImportMapping};
use stellarator_core::{Config, Ctx};
use stellarator_import::files::{open_path, Opened};
use stellarator_import::prompts::{resolve_plan, scan_prompts, window, Prompt, Stats, TextPrompts, NO_TEXT_COLUMN};
use stellarator_import::table::Flow;
use stellarator_import::text::split_text;
use stellarator_import::{Imports, Problem};
use tower::ServiceExt;

pub fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(rel)
}

/// What reading a file through the importer produced.
pub struct Read {
    pub prompts: Vec<Prompt>,
    pub prompt_field: Option<String>,
    pub system_field: Option<String>,
    pub split_mode: Option<stellarator_core::schemas::SplitMode>,
    pub stats: Stats,
    pub warnings: Vec<String>,
}

impl Read {
    pub fn users(&self) -> Vec<String> {
        self.prompts.iter().map(|p| p.key().to_string()).collect()
    }
}

/// Open a file and read every prompt, like the Python `prompts_of` test helper.
pub fn prompts_of(path: &Path, mapping: &ImportMapping) -> Result<Read, Problem> {
    prompts_of_format(path, mapping, ImportFormat::Auto)
}

pub fn prompts_of_format(path: &Path, mapping: &ImportMapping, format: ImportFormat) -> Result<Read, Problem> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    match open_path(path, format, name.as_deref())? {
        Opened::Text(doc) => {
            let (pieces, mode) = split_text(&doc.text, mapping.split_mode, doc.markdown());
            let mut tp = TextPrompts::new(mapping.dedupe);
            let prompts = window(&pieces, mapping).iter().filter_map(|p| tp.take(p)).collect();
            let mut warnings = doc.warnings.clone();
            warnings.extend(tp.stats.warnings());
            Ok(Read { prompts, prompt_field: None, system_field: None, split_mode: Some(mode), stats: tp.stats, warnings })
        }
        Opened::Table(mut src) => {
            let (plan, pf, sf) = resolve_plan(&src.schema, &src.sample, mapping)?;
            let plan = plan.ok_or_else(|| Problem::new(NO_TEXT_COLUMN))?;
            let mut prompts = Vec::new();
            let stats = scan_prompts(
                src.table.as_mut(),
                &plan,
                mapping.offset as u64,
                mapping.limit.map(|l| l as u64),
                mapping.dedupe,
                &mut |p| {
                    prompts.push(p);
                    Flow::Continue
                },
            )?;
            let mut warnings = src.table.warnings();
            warnings.extend(stats.warnings());
            Ok(Read { prompts, prompt_field: pf, system_field: sf, split_mode: None, stats, warnings })
        }
    }
}

pub fn mapping(v: Value) -> ImportMapping {
    serde_json::from_value(v).expect("mapping")
}

// ------------------------------------------------------------------------------------ app

pub struct App {
    pub dir: tempfile::TempDir,
    pub ctx: Ctx,
    pub imports: Arc<Imports>,
    pub router: Router,
}

pub async fn app_with(hf_base: Option<&str>, max_upload: Option<u64>) -> App {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut config = Config::new(dir.path().to_path_buf(), Mode::Native);
    config.env_hf_token = None;
    let base = hf_base.unwrap_or("http://127.0.0.1:9");
    config.hf_datasets_server = base.to_string();
    config.hf_hub = base.to_string();
    let ctx = Ctx::open(config).await.expect("ctx");
    let imports = match max_upload {
        Some(m) => Imports::with_max_upload(ctx.clone(), m),
        None => Imports::new(ctx.clone()),
    };
    let router = stellarator_import::router(imports.clone());
    App { dir, ctx, imports, router }
}

pub async fn app() -> App {
    app_with(None, None).await
}

impl App {
    pub fn upload_dir(&self) -> PathBuf {
        self.ctx.config.upload_dir()
    }

    pub async fn send(&self, req: Request<Body>) -> (StatusCode, Value) {
        let resp = self.router.clone().oneshot(req).await.expect("response");
        let status = resp.status();
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let v = serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, v)
    }

    pub async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("json")))
            .expect("request");
        self.send(req).await
    }

    pub async fn upload(&self, name: &str, data: &[u8]) -> (StatusCode, Value) {
        let boundary = "XyZboundary42";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
        body.extend_from_slice(data);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/imports/upload")
            .header("content-type", format!("multipart/form-data; boundary={boundary}"))
            .header("content-length", body.len())
            .body(Body::from(body))
            .expect("request");
        self.send(req).await
    }

    pub async fn upload_ok(&self, name: &str, data: &[u8]) -> Value {
        let (s, v) = self.upload(name, data).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        v
    }

    /// Wait until the set is no longer importing; returns it.
    pub async fn wait(&self, id: i64) -> stellarator_core::schemas::PromptSetOut {
        for _ in 0..3000 {
            let set = stellarator_import::load_set(&self.ctx.db, id).await.expect("set");
            if set.status != stellarator_core::schemas::PromptSetStatus::Importing {
                // The job leaves the registry right after its last write.
                for _ in 0..200 {
                    if !stellarator_core::probes::ImportsProbe::is_importing(self.imports.as_ref(), id) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                return set;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("import {id} did not finish");
    }

    /// Commit, expect 202, wait for the outcome.
    pub async fn import(&self, body: Value) -> stellarator_core::schemas::PromptSetOut {
        let (s, v) = self.post("/api/imports", body).await;
        assert_eq!(s, StatusCode::ACCEPTED, "{v}");
        assert_eq!(v["status"], "importing");
        self.wait(v["id"].as_i64().expect("id")).await
    }

    /// Stored prompts of a set: (messages, meta) in idx order.
    pub async fn stored(&self, id: i64) -> Vec<(Value, Value)> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT messages, meta FROM prompts WHERE prompt_set_id = ? ORDER BY idx")
                .bind(id)
                .fetch_all(&self.ctx.db)
                .await
                .expect("rows");
        rows.into_iter()
            .map(|(m, meta)| (serde_json::from_str(&m).expect("messages"), serde_json::from_str(&meta).expect("meta")))
            .collect()
    }
}

// ------------------------------------------------------------------------------------ parquet

pub fn parquet_bytes(batches: &[RecordBatch], row_group_size: usize) -> Vec<u8> {
    let mut buf = Vec::new();
    // Statistics truncated like pyarrow's (which caps them), so footers stay small.
    let props = WriterProperties::builder()
        .set_max_row_group_size(row_group_size)
        .set_statistics_truncate_length(Some(64))
        .set_column_index_truncate_length(Some(64))
        .build();
    let mut w = ArrowWriter::try_new(&mut buf, batches[0].schema(), Some(props)).expect("writer");
    for b in batches {
        w.write(b).expect("write");
    }
    w.close().expect("close");
    buf
}

// ------------------------------------------------------------------------------------ mock HF

#[derive(Clone)]
pub struct Canned {
    pub status: u16,
    pub body: Value,
    pub headers: Vec<(&'static str, String)>,
}

pub fn ok(body: Value) -> Canned {
    Canned { status: 200, body, headers: Vec::new() }
}

pub fn status(code: u16, body: Value) -> Canned {
    Canned { status: code, body, headers: Vec::new() }
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub method: String,
    pub path: String,
    pub query: String,
    pub auth: Option<String>,
    pub range: Option<String>,
    pub bytes: usize,
}

#[derive(Default)]
pub struct MockState {
    /// Path (and "path?dataset=..." for a more specific match) -> canned JSON answer.
    pub json: HashMap<String, Canned>,
    /// "main/train/0000.parquet" -> file bytes.
    pub files: HashMap<String, Vec<u8>>,
    /// Answer for HEAD on resolve URLs instead of the redirect.
    pub resolve: Option<Canned>,
    pub hits: Vec<Hit>,
}

#[derive(Clone)]
pub struct MockHf {
    pub base: String,
    pub state: Arc<Mutex<MockState>>,
}

impl MockHf {
    pub async fn start() -> Self {
        let state = Arc::new(Mutex::new(MockState::default()));
        let s2 = state.clone();
        let router = Router::new().fallback(move |method: Method, uri: Uri, headers: HeaderMap| {
            let state = s2.clone();
            async move { serve(state, method, uri, headers) }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Self { base: format!("http://{addr}"), state }
    }

    pub fn json(&self, path: &str, c: Canned) {
        self.state.lock().unwrap().json.insert(path.to_string(), c);
    }
    pub fn file(&self, rest: &str, bytes: Vec<u8>) {
        self.state.lock().unwrap().files.insert(rest.to_string(), bytes);
    }
    pub fn hits(&self) -> Vec<Hit> {
        self.state.lock().unwrap().hits.clone()
    }
    pub fn clear_hits(&self) {
        self.state.lock().unwrap().hits.clear();
    }

    /// A dataset with one config "main" and the given splits (name, rows), and its first rows.
    pub fn dataset(&self, splits: &[(&str, i64)], features: Value, rows: Vec<Value>) {
        self.json(
            "/splits",
            ok(serde_json::json!({"splits": splits.iter().map(|(s, _)| serde_json::json!({"dataset": "o/d", "config": "main", "split": s})).collect::<Vec<_>>()})),
        );
        self.json(
            "/size",
            ok(serde_json::json!({"size": {"splits": splits.iter().map(|(s, n)| serde_json::json!({"config": "main", "split": s, "num_rows": n})).collect::<Vec<_>>()}, "partial": false})),
        );
        self.json(
            "/first-rows",
            ok(serde_json::json!({
                "features": features,
                "rows": rows.into_iter().enumerate().map(|(i, r)| serde_json::json!({"row_idx": i, "row": r, "truncated_cells": []})).collect::<Vec<_>>(),
            })),
        );
    }

    /// Serve Parquet files and list them in /parquet. `files`: (split, filename, bytes).
    pub fn parquet(&self, files: Vec<(&str, &str, Vec<u8>)>) {
        let mut listed = Vec::new();
        for (split, name, bytes) in files {
            let url = format!("https://huggingface.co/datasets/o/d/resolve/refs%2Fconvert%2Fparquet/main/{split}/{name}");
            listed.push(serde_json::json!({"dataset": "o/d", "config": "main", "split": split, "url": url, "filename": name, "size": bytes.len()}));
            self.file(&format!("main/{split}/{name}"), bytes);
        }
        self.json("/parquet", ok(serde_json::json!({"parquet_files": listed, "pending": [], "failed": [], "partial": false})));
    }
}

fn serve(state: Arc<Mutex<MockState>>, method: Method, uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().to_string();
    let query = uri.query().unwrap_or("").to_string();
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).map(str::to_string);
    let range = headers.get("range").and_then(|v| v.to_str().ok()).map(str::to_string);
    let mut st = state.lock().unwrap();
    let mut hit = Hit { method: method.to_string(), path: path.clone(), query: query.clone(), auth, range: range.clone(), bytes: 0 };
    let resp = if let Some(rest) = path.strip_prefix("/cdn/") {
        match st.files.get(rest) {
            Some(bytes) => {
                let (start, end) = range
                    .as_deref()
                    .and_then(|r| r.strip_prefix("bytes="))
                    .and_then(|r| r.split_once('-'))
                    .and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()? + 1)))
                    .unwrap_or((0, bytes.len()));
                let end = end.min(bytes.len());
                let body = bytes[start..end].to_vec();
                hit.bytes = body.len();
                let code = if range.is_some() { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK };
                (code, body).into_response()
            }
            None => StatusCode::NOT_FOUND.into_response(),
        }
    } else if let Some((_, after)) = path.split_once("/resolve/") {
        if let Some(c) = st.resolve.clone() {
            canned(&c)
        } else {
            let rest = after.split_once('/').map_or("", |(_, r)| r);
            let mut r = StatusCode::FOUND.into_response();
            r.headers_mut().insert("location", HeaderValue::from_str(&format!("/cdn/{rest}")).expect("header"));
            r
        }
    } else {
        let key = format!("{path}?{query}");
        match st.json.get(&key).or_else(|| st.json.get(&path)) {
            Some(c) => canned(c),
            None => (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"error": "not mocked"}))).into_response(),
        }
    };
    st.hits.push(hit);
    resp
}

fn canned(c: &Canned) -> Response {
    let mut r = (StatusCode::from_u16(c.status).expect("status"), axum::Json(c.body.clone())).into_response();
    for (k, v) in &c.headers {
        r.headers_mut().insert(*k, HeaderValue::from_str(v).expect("header"));
    }
    r
}
