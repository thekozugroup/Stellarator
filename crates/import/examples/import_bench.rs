//! Measure a large local import: duration and peak memory (VmHWM).
//!
//! ```text
//! cargo run --release -p stellarator-import --example import_bench -- gen parquet /tmp/x/big.parquet 1000000
//! cargo run --release -p stellarator-import --example import_bench -- gen csv /tmp/x/big.csv 1000000
//! cargo run --release -p stellarator-import --example import_bench -- run /tmp/x/data /tmp/x/big.parquet
//! ```
//!
//! The generated rows look like a distilled dataset: id, prompt, a ~600-character response (a
//! generation, so never read), and a source column kept as meta.

use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow_array::{ArrayRef, Int64Array, RecordBatch, StringArray};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use stellarator_core::config::Mode;
use stellarator_core::schemas::{ImportFormat, ImportMapping, ImportSource, PromptSetStatus};
use stellarator_core::{Config, Ctx};

const TOPICS: &[&str] = &["algebra", "geometry", "history", "biology", "poetry", "physics", "coding", "law"];

fn prompt(i: u64) -> String {
    format!(
        "Question {i}: explain, step by step and with one worked example, how {} relates to {} in about {} words.",
        TOPICS[(i % 8) as usize],
        TOPICS[((i / 8) % 8) as usize],
        50 + i % 200
    )
}

fn response(i: u64) -> String {
    format!("<think>Let me think about item {i}. {}</think>Here is the answer for {i}.", "We consider each case in turn. ".repeat(18))
}

fn status_kb(field: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with(field)).and_then(|l| l.split_whitespace().nth(1)?.parse().ok()))
        .unwrap_or(0)
}

fn gen_parquet(path: &str, rows: u64) -> anyhow::Result<()> {
    let file = std::fs::File::create(path)?;
    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .set_max_row_group_size(100_000)
        .set_statistics_truncate_length(Some(64))
        .build();
    let batch_rows = |start: u64, n: u64| -> anyhow::Result<RecordBatch> {
        let ids = (start..start + n).map(|i| i as i64);
        Ok(RecordBatch::try_from_iter(vec![
            ("id", Arc::new(Int64Array::from_iter_values(ids)) as ArrayRef),
            ("prompt", Arc::new(StringArray::from_iter_values((start..start + n).map(prompt))) as ArrayRef),
            ("response", Arc::new(StringArray::from_iter_values((start..start + n).map(response))) as ArrayRef),
            ("source", Arc::new(StringArray::from_iter_values((start..start + n).map(|i| TOPICS[(i % 8) as usize]))) as ArrayRef),
        ])?)
    };
    let first = batch_rows(0, 1)?;
    let mut w = ArrowWriter::try_new(file, first.schema(), Some(props))?;
    let mut start = 0;
    while start < rows {
        let n = (rows - start).min(10_000);
        w.write(&batch_rows(start, n)?)?;
        start += n;
    }
    w.close()?;
    Ok(())
}

fn gen_csv(path: &str, rows: u64) -> anyhow::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(f, "id,prompt,response,source")?;
    for i in 0..rows {
        writeln!(f, "{i},\"{}\",\"{}\",{}", prompt(i), response(i), TOPICS[(i % 8) as usize])?;
    }
    f.flush()?;
    Ok(())
}

async fn run(data_dir: &str, path: &str) -> anyhow::Result<()> {
    let mut config = Config::new(data_dir.into(), Mode::Native);
    config.env_hf_token = None;
    let ctx = Ctx::open(config).await?;
    let imports = stellarator_import::Imports::new(ctx.clone());
    // Put the file where an upload would be (a hard link; the import removes it when done).
    let upload_id = uuid_like();
    let folder = ctx.config.upload_dir().join(&upload_id);
    std::fs::create_dir_all(&folder)?;
    let name = std::path::Path::new(path).file_name().map(|n| n.to_owned()).unwrap_or_default();
    std::fs::hard_link(path, folder.join(&name)).or_else(|_| std::fs::copy(path, folder.join(&name)).map(|_| ()))?;
    let before = status_kb("VmRSS:");
    let mapping: ImportMapping = serde_json::from_value(serde_json::json!({}))?;
    let t0 = Instant::now();
    let set = imports
        .start(&ImportSource::File { upload_id, format: ImportFormat::Auto }, &mapping, "bench")
        .await
        .map_err(|e| anyhow::anyhow!("{} {}", e.status, e.detail))?;
    let accepted = t0.elapsed();
    let mut max_rss = 0;
    let set = loop {
        let s = stellarator_import::load_set(&ctx.db, set.id).await?;
        max_rss = max_rss.max(status_kb("VmRSS:"));
        if s.status != PromptSetStatus::Importing {
            break s;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let elapsed = t0.elapsed();
    println!(
        "status={:?} prompts={} rows_read={} accepted_in={:.2}s total={:.2}s ({:.0} prompts/s)",
        set.status,
        set.prompt_count,
        set.rows_read,
        accepted.as_secs_f64(),
        elapsed.as_secs_f64(),
        set.prompt_count as f64 / elapsed.as_secs_f64()
    );
    println!(
        "rss_before={} MB  rss_max_sampled={} MB  peak_rss(VmHWM)={} MB",
        before / 1024,
        max_rss / 1024,
        status_kb("VmHWM:") / 1024
    );
    if let Some(d) = set.status_detail {
        println!("detail: {d}");
    }
    Ok(())
}

fn uuid_like() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:032x}", t)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("gen") => {
            let rows: u64 = args.get(4).and_then(|r| r.parse().ok()).unwrap_or(1_000_000);
            let t0 = Instant::now();
            match args.get(2).map(String::as_str) {
                Some("parquet") => gen_parquet(&args[3], rows)?,
                Some("csv") => gen_csv(&args[3], rows)?,
                _ => anyhow::bail!("gen parquet|csv <path> <rows>"),
            }
            let size = std::fs::metadata(&args[3])?.len();
            println!("wrote {rows} rows, {} MB in {:.1}s", size / (1 << 20), t0.elapsed().as_secs_f64());
        }
        Some("run") => run(&args[2], &args[3]).await?,
        _ => anyhow::bail!("usage: gen parquet|csv <path> <rows> | run <data_dir> <file>"),
    }
    Ok(())
}
