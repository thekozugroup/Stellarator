//! Live check against Hugging Face: serve the import routes on 127.0.0.1:8152, preview and import
//! openai/gsm8k (main/train, every row) and HuggingFaceH4/ultrachat_200k (test_sft, messages,
//! first user turn only), then check what was stored.
//!
//! `cargo run --release -p stellarator-import --example live_check -- /tmp/claude-0/rs-agents/import`

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use stellarator_core::config::Mode;
use stellarator_core::schemas::PromptSetStatus;
use stellarator_core::{Config, Ctx};

async fn post(client: &reqwest::Client, url: &str, body: Value) -> anyhow::Result<(u16, Value)> {
    let r = client.post(url).json(&body).send().await?;
    let status = r.status().as_u16();
    Ok((status, r.json().await.unwrap_or(Value::Null)))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/claude-0/rs-agents/import".into());
    std::fs::create_dir_all(&data_dir)?;
    let ctx = Ctx::open(Config::new(data_dir.into(), Mode::Native)).await?;
    let imports = stellarator_import::Imports::new(ctx.clone());
    imports.recover().await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8152").await?;
    let router = stellarator_import::router(imports.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let base = "http://127.0.0.1:8152";
    let client = reqwest::Client::builder().timeout(Duration::from_secs(120)).build()?;

    let jobs = [
        ("gsm8k", json!({"kind": "huggingface", "dataset": "openai/gsm8k", "config": "main", "split": "train"}), json!({})),
        (
            "ultrachat",
            json!({"kind": "huggingface", "dataset": "HuggingFaceH4/ultrachat_200k", "split": "test_sft"}),
            json!({"prompt_field": "messages", "turns": "first_user"}),
        ),
    ];
    for (name, source, mapping) in jobs {
        let t0 = Instant::now();
        let (s, pv) = post(&client, &format!("{base}/api/imports/preview"), json!({"source": source, "mapping": mapping})).await?;
        println!(
            "\n[{name}] preview {s} in {:.1}s: prompt_field={} system_field={} has_conversations={} dropped={} total_rows={} estimated={} columns={:?}",
            t0.elapsed().as_secs_f64(),
            pv["prompt_field"],
            pv["system_field"],
            pv["has_conversations"],
            pv["dropped"],
            pv["total_rows"],
            pv["estimated_prompts"],
            pv["columns"].as_array().map(|c| c.iter().map(|c| format!("{}:{}{}", c["name"].as_str().unwrap_or(""), c["kind"].as_str().unwrap_or(""), if c["looks_like_output"] == true { "(output)" } else { "" })).collect::<Vec<_>>()),
        );
        if s != 200 {
            println!("  {pv}");
            continue;
        }
        println!("  first prompt: {}", serde_json::to_string(&pv["prompts"][0])?.chars().take(300).collect::<String>());
        // Auto-detection, for the record.
        if name == "ultrachat" {
            let (_, auto) = post(&client, &format!("{base}/api/imports/preview"), json!({"source": source})).await?;
            println!("  auto-detected prompt_field={} dropped={}", auto["prompt_field"], auto["dropped"]);
        }
        let t0 = Instant::now();
        let (s, set) = post(&client, &format!("{base}/api/imports"), json!({"source": source, "mapping": mapping, "name": name})).await?;
        println!("  commit {s} in {:.2}s: status={} rows_total={} detail={}", t0.elapsed().as_secs_f64(), set["status"], set["rows_total"], set["status_detail"]);
        let Some(id) = set["id"].as_i64() else { continue };
        let mut last = String::new();
        let done = loop {
            let cur = stellarator_import::load_set(&ctx.db, id).await?;
            let line = format!("{:?} {} rows read, {} prompts, {:?}", cur.status, cur.rows_read, cur.prompt_count, cur.status_detail);
            if line != last {
                println!("  {:>6.1}s {line}", t0.elapsed().as_secs_f64());
                last = line;
            }
            if cur.status != PromptSetStatus::Importing {
                break cur;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        };
        println!("  finished in {:.1}s: {:?}, {} prompts from {} rows; source={}", t0.elapsed().as_secs_f64(), done.status, done.prompt_count, done.rows_read, done.source);
        let (n, assistant, max_msgs): (i64, i64, i64) = sqlx::query_as(
            "SELECT COUNT(*),
                    SUM(CASE WHEN messages LIKE '%\"role\":\"assistant\"%' THEN 1 ELSE 0 END),
                    MAX(json_array_length(messages))
             FROM prompts WHERE prompt_set_id = ?",
        )
        .bind(id)
        .fetch_one(&ctx.db)
        .await?;
        let first: (String, String) = sqlx::query_as("SELECT messages, meta FROM prompts WHERE prompt_set_id = ? ORDER BY idx LIMIT 1")
            .bind(id)
            .fetch_one(&ctx.db)
            .await?;
        println!("  stored {n} prompts; with an assistant message: {assistant}; most messages in one prompt: {max_msgs}");
        println!("  first stored: {} meta={}", first.0.chars().take(200).collect::<String>(), first.1.chars().take(200).collect::<String>());
    }
    imports.shutdown().await;
    Ok(())
}
