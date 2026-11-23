//! Shared test setup: an app on an in-memory database, request helpers, and SQL seeding for
//! prompt sets, runs and samples (the engine and import crates are not involved).
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use stellarator_core::config::Mode;
use stellarator_core::probes::{ActiveRun, Idle, ImportsProbe, RunsProbe};
use stellarator_core::{Config, Ctx};
use stellarator_server::{AppState, Parts};
use tower::ServiceExt;

pub struct Env {
    pub ctx: Ctx,
    pub app: Router,
    pub dir: tempfile::TempDir,
}

pub async fn env() -> Env {
    env_with(Arc::new(Idle), Arc::new(Idle)).await
}

pub async fn env_with(runs: Arc<dyn RunsProbe>, imports: Arc<dyn ImportsProbe>) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let db = stellarator_core::db::connect_memory().await.unwrap();
    build(dir, db, runs, imports)
}

/// A file-backed database (several connections), for timing and concurrency checks.
pub async fn env_on_disk() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().to_path_buf(), Mode::Native);
    let db = stellarator_core::db::connect(&config).await.unwrap();
    build(dir, db, Arc::new(Idle), Arc::new(Idle))
}

/// [`env`] where stdio MCP servers are allowed (as in the desktop app, or with
/// STELLARATOR_ALLOW_STDIO_MCP=1).
pub async fn env_stdio_allowed() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let db = stellarator_core::db::connect_memory().await.unwrap();
    build_with(dir, db, Arc::new(Idle), Arc::new(Idle), true)
}

fn build(dir: tempfile::TempDir, db: sqlx::SqlitePool, runs: Arc<dyn RunsProbe>, imports: Arc<dyn ImportsProbe>) -> Env {
    build_with(dir, db, runs, imports, false)
}

fn build_with(
    dir: tempfile::TempDir,
    db: sqlx::SqlitePool,
    runs: Arc<dyn RunsProbe>,
    imports: Arc<dyn ImportsProbe>,
    allow_stdio_mcp: bool,
) -> Env {
    let mut config = Config::new(dir.path().to_path_buf(), Mode::Native);
    config.allow_stdio_mcp = allow_stdio_mcp;
    // No proxy: the fake servers are on 127.0.0.1.
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let ctx = Ctx { db, config: Arc::new(config), http };
    let parts = Parts { state: AppState { ctx: ctx.clone(), runs, imports }, extra: Vec::new(), lifecycle: Vec::new() };
    Env { ctx, app: stellarator_server::app(&parts), dir }
}

impl Env {
    pub async fn call(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(v) => {
                req = req.header("content-type", "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        let resp = self.app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) };
        (status, value)
    }
    pub async fn get(&self, uri: &str) -> (StatusCode, Value) {
        self.call(Method::GET, uri, None).await
    }
    pub async fn post(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, uri, Some(body)).await
    }
    pub async fn put(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PUT, uri, Some(body)).await
    }
    pub async fn patch(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PATCH, uri, Some(body)).await
    }
    pub async fn delete(&self, uri: &str) -> (StatusCode, Value) {
        self.call(Method::DELETE, uri, None).await
    }

    pub async fn count(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(&self.ctx.db).await.unwrap()
    }

    /// A prompt set of `n` prompts "Question i" with ids returned in idx order.
    pub async fn prompt_set(&self, name: &str, n: i64) -> (i64, Vec<i64>) {
        let id = sqlx::query(
            "INSERT INTO prompt_sets (name, source_kind, source, prompt_count, created_at) VALUES (?, 'paste', '{}', ?, ?)",
        )
        .bind(name)
        .bind(n)
        .bind(stellarator_core::time::now_db())
        .execute(&self.ctx.db)
        .await
        .unwrap()
        .last_insert_rowid();
        let mut ids = Vec::new();
        for i in 0..n {
            let messages = json!([{"role": "user", "content": format!("Question {i}")}]);
            let pid = sqlx::query("INSERT INTO prompts (prompt_set_id, idx, messages, meta) VALUES (?, ?, ?, '{}')")
                .bind(id)
                .bind(i)
                .bind(messages.to_string())
                .execute(&self.ctx.db)
                .await
                .unwrap()
                .last_insert_rowid();
            ids.push(pid);
        }
        (id, ids)
    }

    pub async fn run(&self, prompt_set_id: i64, name: &str, model: &str) -> i64 {
        self.run_on(prompt_set_id, name, model, None, "completed").await
    }

    pub async fn run_on(&self, prompt_set_id: i64, name: &str, model: &str, endpoint_id: Option<i64>, status: &str) -> i64 {
        sqlx::query(
            "INSERT INTO runs (name, endpoint_id, endpoint_name, model, prompt_set_id, params, concurrency, status, \
             total, done, failed, truncated, prompt_tokens, completion_tokens, reasoning_tokens, active_seconds, created_at) \
             VALUES (?, ?, 'Spark', ?, ?, '{}', 16, ?, 0, 0, 0, 0, 0, 0, 0, 0, ?)",
        )
        .bind(name)
        .bind(endpoint_id)
        .bind(model)
        .bind(prompt_set_id)
        .bind(status)
        .bind(stellarator_core::time::now_db())
        .execute(&self.ctx.db)
        .await
        .unwrap()
        .last_insert_rowid()
    }

    pub async fn sample(&self, run_id: i64, prompt_id: i64, sample_idx: i64, status: &str, finish_reason: &str) -> i64 {
        let idx: i64 = sqlx::query_scalar("SELECT idx FROM prompts WHERE id = ?")
            .bind(prompt_id)
            .fetch_one(&self.ctx.db)
            .await
            .unwrap();
        let done = status == "done";
        sqlx::query(
            "INSERT INTO samples (run_id, prompt_id, prompt_idx, sample_idx, status, reasoning, response, finish_reason, \
             completion_tokens, reasoning_tokens, attempts) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)",
        )
        .bind(run_id)
        .bind(prompt_id)
        .bind(idx)
        .bind(sample_idx)
        .bind(status)
        .bind(done.then(|| format!("thinking {run_id}/{idx}/{sample_idx}")))
        .bind(done.then(|| format!("answer {run_id}/{idx}/{sample_idx}")))
        .bind(done.then_some(finish_reason))
        .bind(done.then_some(100 + sample_idx))
        .bind(done.then_some(60 + sample_idx))
        .execute(&self.ctx.db)
        .await
        .unwrap()
        .last_insert_rowid()
    }

    pub async fn done(&self, run_id: i64, prompt_id: i64, sample_idx: i64) -> i64 {
        self.sample(run_id, prompt_id, sample_idx, "done", "stop").await
    }

    pub async fn endpoint(&self, name: &str) -> i64 {
        let now = stellarator_core::time::now_db();
        sqlx::query(
            "INSERT INTO endpoints (name, base_url, location, default_concurrency, headers, models, created_at, updated_at) \
             VALUES (?, 'http://127.0.0.1:1/v1', 'local', 4, '{}', '[]', ?, ?)",
        )
        .bind(name)
        .bind(&now)
        .bind(&now)
        .execute(&self.ctx.db)
        .await
        .unwrap()
        .last_insert_rowid()
    }
}

/// A probe that reports fixed active runs.
pub struct ActiveRuns(pub Vec<ActiveRun>);

impl RunsProbe for ActiveRuns {
    fn active_runs(&self) -> Vec<ActiveRun> {
        self.0.clone()
    }
}

/// A probe that says one set is importing and records cancel calls, and what the database held
/// at that moment (the delete must cancel before it removes rows).
pub struct FakeImports {
    pub importing: i64,
    pub cancelled: Mutex<Vec<i64>>,
}

impl FakeImports {
    pub fn new(importing: i64) -> Arc<Self> {
        Arc::new(Self { importing, cancelled: Mutex::new(Vec::new()) })
    }
}

impl ImportsProbe for FakeImports {
    fn is_importing(&self, id: i64) -> bool {
        id == self.importing && self.cancelled.lock().unwrap().is_empty()
    }
    fn cancel(&self, id: i64) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.cancelled.lock().unwrap().push(id);
        })
    }
}

// ------------------------------------------------------------------------------------ fake OpenAI

/// What the fake server saw on `/chat/completions`.
#[derive(Default)]
pub struct Seen {
    pub chat: Mutex<Vec<(Option<String>, Value)>>,
    pub models_auth: Mutex<Vec<Option<String>>>,
}

pub struct Fake {
    pub base_url: String,
    pub seen: Arc<Seen>,
}

/// A tiny OpenAI-compatible server on 127.0.0.1:0. `/v1/models` lists `models` (200) or answers
/// `models_status`; `/v1/chat/completions` answers `chat_status` with `chat_body`.
pub async fn fake_openai(models: &[&str], models_status: u16, chat_status: u16, chat_body: Value) -> Fake {
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::{get, post};
    use axum::Json;

    #[derive(Clone)]
    struct S {
        models: Vec<String>,
        models_status: u16,
        chat_status: u16,
        chat_body: Value,
        seen: Arc<Seen>,
    }
    let seen = Arc::new(Seen::default());
    let state = S {
        models: models.iter().map(|m| m.to_string()).collect(),
        models_status,
        chat_status,
        chat_body,
        seen: seen.clone(),
    };
    let auth = |h: &HeaderMap| h.get("authorization").and_then(|v| v.to_str().ok()).map(String::from);
    let app = Router::new()
        .route(
            "/v1/models",
            get(move |State(s): State<S>, h: HeaderMap| async move {
                s.seen.models_auth.lock().unwrap().push(auth(&h));
                let data: Vec<Value> = s.models.iter().map(|m| json!({"id": m})).collect();
                (StatusCode::from_u16(s.models_status).unwrap(), Json(json!({"data": data})))
            }),
        )
        .route(
            "/v1/chat/completions",
            post(move |State(s): State<S>, h: HeaderMap, Json(body): Json<Value>| async move {
                s.seen.chat.lock().unwrap().push((auth(&h), body));
                (StatusCode::from_u16(s.chat_status).unwrap(), Json(s.chat_body.clone()))
            }),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    Fake { base_url: format!("http://{addr}/v1"), seen }
}

impl FakeImports {
    pub fn is_importing_now(&self, id: i64) -> bool {
        ImportsProbe::is_importing(self, id)
    }
}
