//! Shared setup for the engine's integration tests: a database in a temp dir, the engine and its
//! router, the mock server in-process, and helpers that mirror backend/tests (make_endpoint,
//! make_prompt_set, start_run, wait_for, truth).

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use stellarator_core::config::{Config, Mode};
use stellarator_core::{time, Ctx};
use stellarator_engine::{router, Engine};
use tower::ServiceExt;

pub struct Env {
    pub engine: Arc<Engine>,
    pub app: Router,
    pub db: SqlitePool,
    /// Base URL of the in-process mock server ("http://127.0.0.1:port/v1").
    pub mock: String,
    mock_task: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}

impl Drop for Env {
    fn drop(&mut self) {
        self.mock_task.abort();
    }
}

/// A fresh engine on a fresh database, with the mock at 400 tokens/s per sequence (like conftest).
pub async fn env() -> Env {
    env_with_tps(400.0).await
}

pub async fn env_with_tps(tps: f64) -> Env {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = Config::new(dir.path().to_path_buf(), Mode::Native);
    let ctx = Ctx::open(config).await.expect("open database");
    let db = ctx.db.clone();
    let engine = Engine::new(ctx);
    engine.recover().await.expect("recover");
    let app = router(engine.clone());
    let (addr, mock_task) =
        stellarator_mock::serve(SocketAddr::from(([127, 0, 0, 1], 0)), tps).await.expect("mock server");
    Env { engine, app, db, mock: format!("http://{addr}/v1"), mock_task, _dir: dir }
}

/// A URL where nothing listens.
pub fn dead_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    format!("http://127.0.0.1:{port}/v1")
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Truth {
    pub done: i64,
    pub failed: i64,
    pub pending: i64,
    pub running: i64,
    pub truncated: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
}

impl Env {
    pub async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(path);
        let body = match body {
            Some(v) => {
                req = req.header("content-type", "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        let resp = self.app.clone().oneshot(req.body(body).expect("request")).await.expect("response");
        let status = resp.status();
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let value = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) };
        (status, value)
    }

    pub async fn get(&self, path: &str) -> Value {
        let (status, v) = self.call(Method::GET, path, None).await;
        assert_eq!(status, StatusCode::OK, "GET {path}: {v}");
        v
    }

    pub async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, path, Some(body)).await
    }

    pub async fn make_endpoint(&self, base_url: &str, name: &str) -> i64 {
        let now = time::now_db();
        sqlx::query(
            "INSERT INTO endpoints (name, base_url, api_key, location, default_concurrency, headers, models, \
             created_at, updated_at) VALUES (?, ?, NULL, 'local', 16, '{}', '[]', ?, ?)",
        )
        .bind(name)
        .bind(base_url)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert endpoint")
        .last_insert_rowid()
    }

    /// A prompt set with `n` prompts "Question i: what is i + i?".
    pub async fn make_prompt_set(&self, n: i64) -> i64 {
        self.make_prompt_set_with(n, &format!("Test set ({n})"), "ready", |i| {
            json!([{"role": "user", "content": format!("Question {i}: what is {i} + {i}?")}])
        })
        .await
    }

    pub async fn make_prompt_set_with(&self, n: i64, name: &str, status: &str, messages: impl Fn(i64) -> Value) -> i64 {
        let mut tx = self.db.begin().await.expect("begin");
        let set_id = sqlx::query(
            "INSERT INTO prompt_sets (name, source_kind, source, prompt_count, created_at, status) VALUES (?, 'paste', '{}', ?, ?, ?)",
        )
        .bind(name)
        .bind(n)
        .bind(time::now_db())
        .bind(status)
        .execute(&mut *tx)
        .await
        .expect("insert prompt set")
        .last_insert_rowid();
        let mut i = 0;
        while i < n {
            let end = (i + 500).min(n);
            let sql = format!(
                "INSERT INTO prompts (prompt_set_id, idx, messages, meta) VALUES {}",
                vec!["(?, ?, ?, '{}')"; (end - i) as usize].join(", ")
            );
            let mut q = sqlx::query(&sql);
            for k in i..end {
                q = q.bind(set_id).bind(k).bind(messages(k).to_string());
            }
            q.execute(&mut *tx).await.expect("insert prompts");
            i = end;
        }
        tx.commit().await.expect("commit");
        set_id
    }

    pub async fn start_run(&self, endpoint_id: i64, prompt_set_id: i64, model: &str, concurrency: i64, params: Value) -> Value {
        let (status, run) = self
            .post(
                "/api/runs",
                json!({"endpoint_id": endpoint_id, "model": model, "prompt_set_id": prompt_set_id,
                       "concurrency": concurrency, "params": params}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{run}");
        run
    }

    pub async fn run(&self, run_id: i64) -> Value {
        self.get(&format!("/api/runs/{run_id}")).await
    }

    pub async fn live(&self, run_id: i64) -> Value {
        self.get(&format!("/api/runs/{run_id}/live")).await
    }

    pub async fn wait_for(&self, run_id: i64, pred: impl Fn(&Value) -> bool, timeout_s: f64) -> Value {
        let deadline = Instant::now() + Duration::from_secs_f64(timeout_s);
        loop {
            let run = self.run(run_id).await;
            if pred(&run) {
                return run;
            }
            assert!(Instant::now() < deadline, "timed out; last state: {run}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub async fn wait_live(&self, run_id: i64, pred: impl Fn(&Value) -> bool, timeout_s: f64) -> Value {
        let deadline = Instant::now() + Duration::from_secs_f64(timeout_s);
        loop {
            let lv = self.live(run_id).await;
            if pred(&lv) {
                return lv;
            }
            assert!(Instant::now() < deadline, "timed out; last live state: in_flight={} run={}", lv["in_flight"], lv["run"]);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Counters computed from the sample rows.
    pub async fn truth(&self, run_id: i64) -> Truth {
        type Row = (String, i64, Option<i64>, Option<i64>, Option<i64>);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT status, COUNT(*), SUM(prompt_tokens), SUM(completion_tokens), SUM(reasoning_tokens) \
             FROM samples WHERE run_id = ? GROUP BY status",
        )
        .bind(run_id)
        .fetch_all(&self.db)
        .await
        .expect("truth");
        let truncated: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM samples WHERE run_id = ? AND status = 'done' \
             AND finish_reason IN ('length', 'tool_rounds')",
        )
        .bind(run_id)
        .fetch_one(&self.db)
        .await
        .expect("truncated");
        let mut t = Truth { truncated, ..Truth::default() };
        for (status, n, p, c, r) in rows {
            match status.as_str() {
                "done" => {
                    t.done = n;
                    t.prompt_tokens = p.unwrap_or(0);
                    t.completion_tokens = c.unwrap_or(0);
                    t.reasoning_tokens = r.unwrap_or(0);
                }
                "failed" => t.failed = n,
                "pending" => t.pending = n,
                "running" => t.running = n,
                _ => {}
            }
        }
        t
    }

    pub async fn count(&self, sql: &str, run_id: i64) -> i64 {
        sqlx::query_scalar(sql).bind(run_id).fetch_one(&self.db).await.expect("count")
    }
}

pub fn assert_counters_match(run: &Value, t: &Truth) {
    let pairs = [
        ("done", t.done),
        ("failed", t.failed),
        ("truncated", t.truncated),
        ("prompt_tokens", t.prompt_tokens),
        ("completion_tokens", t.completion_tokens),
        ("reasoning_tokens", t.reasoning_tokens),
    ];
    for (key, want) in pairs {
        assert_eq!(run[key].as_i64(), Some(want), "{key}: run {} truth {want}", run[key]);
    }
}

pub fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| panic!("not an integer: {v}"))
}

pub fn s(v: &Value) -> &str {
    v.as_str().unwrap_or_else(|| panic!("not a string: {v}"))
}
