//! Seed a large run and time its export.
//!
//!   export_bench <data_dir> seed <prompts> <samples_per_prompt>   # prints the run id
//!   export_bench <data_dir> export <run_id> <jsonl|json|csv|parquet> [format]
//!   export_bench <data_dir> serve <port>                          # for manual curl checks
//!
//! Run the export step under `/usr/bin/time -v` for peak RSS.

use std::time::Instant;

use axum::body::Body;
use axum::http::Request;
use futures::StreamExt;
use stellarator_core::config::{Config, Mode};
use stellarator_core::Ctx;
use tower::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&args[1]);
    std::fs::create_dir_all(&dir)?;
    let mut config = Config::new(dir, Mode::Native);
    config.env_hf_token = None;
    let ctx = Ctx::open(config).await?;
    match args[2].as_str() {
        "seed" => seed(&ctx, args[3].parse()?, args[4].parse()?).await,
        "export" => {
            let run: i64 = args[3].parse()?;
            let ft = &args[4];
            let fmt = args.get(5).map(String::as_str).unwrap_or("messages");
            let app = stellarator_export::router(ctx);
            let t = Instant::now();
            let resp = app
                .oneshot(Request::get(format!("/api/runs/{run}/export?file_type={ft}&format={fmt}")).body(Body::empty())?)
                .await?;
            let status = resp.status();
            let mut stream = resp.into_body().into_data_stream();
            let (mut bytes, mut chunks) = (0usize, 0usize);
            while let Some(c) = stream.next().await {
                bytes += c?.len();
                chunks += 1;
            }
            println!("{status} {ft}/{fmt}: {bytes} bytes in {chunks} chunks, {:.2}s", t.elapsed().as_secs_f64());
            Ok(())
        }
        "serve" => {
            let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", args[3])).await?;
            println!("listening on {}", listener.local_addr()?);
            axum::serve(listener, stellarator_export::router(ctx)).await?;
            Ok(())
        }
        other => anyhow::bail!("unknown mode {other}"),
    }
}

async fn seed(ctx: &Ctx, prompts: i64, per: i64) -> anyhow::Result<()> {
    let db = &ctx.db;
    let ts = "2026-01-01 00:00:00.000000";
    let ps = sqlx::query("INSERT INTO prompt_sets (name, source_kind, source, prompt_count, created_at) VALUES ('bench', 'paste', '{}', ?, ?)")
        .bind(prompts)
        .bind(ts)
        .execute(db)
        .await?
        .last_insert_rowid();
    sqlx::query(
        "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i + 1 < ?1) \
         INSERT INTO prompts (prompt_set_id, idx, messages, meta) \
         SELECT ?2, i, json_array(json_object('role','system','content','You are a careful math tutor.'), \
           json_object('role','user','content', 'Question ' || i || ': ' || replace(hex(zeroblob(40)), '00', 'what is the sum of ') || '?')), \
           json_object('answer', i) FROM n",
    )
    .bind(prompts)
    .bind(ps)
    .execute(db)
    .await?;
    let run = sqlx::query(
        "INSERT INTO runs (name, endpoint_name, model, prompt_set_id, params, concurrency, status, total, done, failed, truncated, \
         prompt_tokens, completion_tokens, reasoning_tokens, active_seconds, created_at) \
         VALUES ('bench run', 'local', 'qwen3', ?, '{\"temperature\": 0.7}', 4, 'completed', 0, 0, 0, 0, 0, 0, 0, 0.0, ?)",
    )
    .bind(ps)
    .bind(ts)
    .execute(db)
    .await?
    .last_insert_rowid();
    sqlx::query(
        "WITH RECURSIVE s(k) AS (SELECT 0 UNION ALL SELECT k + 1 FROM s WHERE k + 1 < ?1) \
         INSERT INTO samples (run_id, prompt_id, prompt_idx, sample_idx, status, reasoning, response, finish_reason, prompt_tokens, \
           completion_tokens, reasoning_tokens, latency_ms, ttft_ms, attempts, completed_at) \
         SELECT ?2, p.id, p.idx, s.k, 'done', \
           replace(hex(zeroblob(120)), '00', 'think step ') || p.idx || '-' || s.k, \
           replace(hex(zeroblob(40)), '00', 'answer text é ') || p.idx || '-' || s.k, \
           'stop', 50, 400, 300, 900, 40, 1, ?3 \
         FROM prompts p, s WHERE p.prompt_set_id = ?4",
    )
    .bind(per)
    .bind(run)
    .bind(ts)
    .bind(ps)
    .execute(db)
    .await?;
    println!("{run}");
    Ok(())
}
