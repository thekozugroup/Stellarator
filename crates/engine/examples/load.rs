//! Load and memory check for the run engine. Drives one run through the engine's router against an
//! OpenAI-compatible server (normally `stellarator-mock` in its own process, so the numbers below
//! are the engine's alone) and reports time, peak in-flight requests and memory.
//!
//!     stellarator-mock --host 127.0.0.1 --port 9151 --tps 400 &
//!     cargo run --release -p stellarator-engine --example load -- \
//!         --base-url http://127.0.0.1:9151/v1 --prompts 1000 --samples 2 --concurrency 256
//!
//! Options: --data DIR (default: a temp dir), --model NAME (default mock-reasoning-content),
//! --serve ADDR (also serve the API there while the run goes, for curl).

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use stellarator_core::config::{Config, Mode};
use stellarator_core::{time, Ctx};
use stellarator_engine::{router, Engine};
use tower::ServiceExt;

fn status_kb(field: &str) -> u64 {
    let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    text.lines()
        .find(|l| l.starts_with(field))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn mb(kb: u64) -> f64 {
    kb as f64 / 1024.0
}

async fn call(app: &axum::Router, method: &str, path: &str, body: Option<Value>) -> anyhow::Result<Value> {
    let mut req = Request::builder().method(method).uri(path);
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(req.body(body)?).await?;
    let status = resp.status();
    let bytes = resp.into_body().collect().await?.to_bytes();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    anyhow::ensure!(status.is_success(), "{method} {path}: {status} {v}");
    Ok(v)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut base_url = String::from("http://127.0.0.1:9151/v1");
    let mut prompts: i64 = 1000;
    let mut samples: i64 = 2;
    let mut concurrency: i64 = 256;
    let mut model = String::from("mock-reasoning-content");
    let mut data: Option<String> = None;
    let mut serve: Option<SocketAddr> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let v = args.next().ok_or_else(|| anyhow::anyhow!("{a} needs a value"))?;
        match a.as_str() {
            "--base-url" => base_url = v,
            "--prompts" => prompts = v.parse()?,
            "--samples" => samples = v.parse()?,
            "--concurrency" => concurrency = v.parse()?,
            "--model" => model = v,
            "--data" => data = Some(v),
            "--serve" => serve = Some(v.parse()?),
            other => anyhow::bail!("unknown option {other}"),
        }
    }
    let tmp = tempfile::tempdir()?;
    let dir = match &data {
        Some(d) => {
            let p = std::path::PathBuf::from(d);
            let _ = std::fs::remove_file(p.join("stellarator.db"));
            let _ = std::fs::remove_file(p.join("stellarator.db-wal"));
            let _ = std::fs::remove_file(p.join("stellarator.db-shm"));
            p
        }
        None => tmp.path().to_path_buf(),
    };
    std::fs::create_dir_all(&dir)?;
    let ctx = Ctx::open(Config::new(dir.clone(), Mode::Native)).await?;
    let db = ctx.db.clone();
    let engine = Engine::new(ctx);
    engine.recover().await?;
    let app = router(engine.clone());
    if let Some(addr) = serve {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let app = app.clone();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        eprintln!("serving the runs API on http://{addr}");
    }

    let now = time::now_db();
    let ep = sqlx::query(
        "INSERT INTO endpoints (name, base_url, location, default_concurrency, headers, models, created_at, updated_at) \
         VALUES ('Mock', ?, 'local', 16, '{}', '[]', ?, ?)",
    )
    .bind(&base_url)
    .bind(&now)
    .bind(&now)
    .execute(&db)
    .await?
    .last_insert_rowid();
    let ps = sqlx::query(
        "INSERT INTO prompt_sets (name, source_kind, source, prompt_count, created_at) VALUES ('Load', 'paste', '{}', ?, ?)",
    )
    .bind(prompts)
    .bind(&now)
    .execute(&db)
    .await?
    .last_insert_rowid();
    let mut tx = db.begin().await?;
    let mut k = 0;
    while k < prompts {
        let end = (k + 500).min(prompts);
        let sql = format!(
            "INSERT INTO prompts (prompt_set_id, idx, messages, meta) VALUES {}",
            vec!["(?, ?, ?, '{}')"; (end - k) as usize].join(", ")
        );
        let mut q = sqlx::query(&sql);
        for n in k..end {
            let messages = json!([{"role": "user", "content": format!("Question {n}: what is {n} + {n}?")}]);
            q = q.bind(ps).bind(n).bind(messages.to_string());
        }
        q.execute(&mut *tx).await?;
        k = end;
    }
    tx.commit().await?;

    let rss_start = status_kb("VmRSS:");
    let t0 = Instant::now();
    let run = call(
        &app,
        "POST",
        "/api/runs",
        Some(json!({"endpoint_id": ep, "model": model, "prompt_set_id": ps, "concurrency": concurrency,
                    "params": {"samples_per_prompt": samples}})),
    )
    .await?;
    let created = t0.elapsed();
    let rid = run["id"].as_i64().unwrap_or(0);
    let total = run["total"].as_i64().unwrap_or(0);
    println!("run {rid}: {total} samples created in {:.2} s; RSS at start {:.1} MB", created.as_secs_f64(), mb(rss_start));
    let mut peak_in_flight = 0;
    let mut next_report = total / 10;
    let mut rss_points: Vec<(i64, f64)> = Vec::new();
    loop {
        let lv = call(&app, "GET", &format!("/api/runs/{rid}/live"), None).await?;
        peak_in_flight = peak_in_flight.max(lv["in_flight"].as_i64().unwrap_or(0));
        let finished = lv["run"]["done"].as_i64().unwrap_or(0) + lv["run"]["failed"].as_i64().unwrap_or(0);
        if finished >= next_report && next_report > 0 {
            let rss = mb(status_kb("VmRSS:"));
            rss_points.push((finished, rss));
            println!(
                "  {finished:>7} finished  {:>6.0} samples/min  {:>7.0} tokens/s  in flight {:>4}  RSS {rss:.1} MB",
                lv["samples_per_min"].as_f64().unwrap_or(0.0),
                lv["tokens_per_s"].as_f64().unwrap_or(0.0),
                lv["in_flight"].as_i64().unwrap_or(0),
            );
            next_report += total / 10;
        }
        if lv["run"]["status"] == "completed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let elapsed = t0.elapsed();
    let run = call(&app, "GET", &format!("/api/runs/{rid}"), None).await?;
    let (done, failed, pending, running): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT SUM(status = 'done'), SUM(status = 'failed'), SUM(status = 'pending'), SUM(status = 'running') \
         FROM samples WHERE run_id = ?",
    )
    .bind(rid)
    .fetch_one(&db)
    .await?;
    let tokens: (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(completion_tokens), 0), COALESCE(SUM(reasoning_tokens), 0) FROM samples WHERE run_id = ? AND status = 'done'",
    )
    .bind(rid)
    .fetch_one(&db)
    .await?;
    let matches = run["done"] == done
        && run["failed"] == failed
        && run["completion_tokens"] == tokens.0
        && run["reasoning_tokens"] == tokens.1;
    println!(
        "done {done}, failed {failed}, pending {pending}, running {running}; counters match rows: {matches}; \
         completion tokens {}",
        tokens.0
    );
    println!(
        "{total} samples at {concurrency} seqs: {:.1} s, peak in flight {peak_in_flight}, \
         peak RSS (VmHWM) {:.1} MB, RSS now {:.1} MB",
        elapsed.as_secs_f64(),
        mb(status_kb("VmHWM:")),
        mb(status_kb("VmRSS:")),
    );
    engine.shutdown().await;
    anyhow::ensure!(matches && pending == 0 && running == 0, "counters do not match the sample rows");
    Ok(())
}
