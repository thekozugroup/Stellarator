//! Hugging Face datasets: splits, sizes and first rows from the datasets-server API, and rows read
//! from the auto-converted Parquet export with HTTP range requests (only the columns the import
//! needs, so a distilled dataset's long responses are never downloaded).

use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use bytes::Bytes;
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions};
use parquet::arrow::async_reader::{AsyncFileReader, ParquetRecordBatchStreamBuilder};
use parquet::arrow::ProjectionMask;
use parquet::errors::ParquetError;
use parquet::file::metadata::{ParquetMetaData, ParquetMetaDataReader};
use regex::Regex;
use reqwest::header::{HeaderMap, CONTENT_LENGTH, LOCATION, RANGE};
use reqwest::StatusCode;
use serde_json::Value;
use stellarator_core::openai::describe_reqwest_error;
use stellarator_core::schemas::HfSplit;
use tokio::runtime::Handle;

use crate::cancel::Cancel;
use crate::parquet_file::{batch_size, emit_batch, plan_row_groups, usable_columns};
use crate::problem::{Problem, Result};
use crate::schema::Hint;
use crate::table::{Flow, MemoryTable, Row, RowSink, Table, Window};

const API_TIMEOUT: Duration = Duration::from_secs(30);
/// Byte ranges closer than this are fetched in one request.
const COALESCE_GAP: usize = 1 << 20;
/// Footer bytes fetched with the first metadata request: 1/32 of the file, from 8 KiB to 1 MiB
/// (one request covers the metadata of a large shard; a small file isn't read whole).
fn footer_prefetch(size: usize) -> usize {
    (size / 32).clamp(8 * 1024, 1024 * 1024)
}

static NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\w.\-]+(/[\w.\-]+)?$").expect("regex"));
static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:https?://)?(?:www\.)?huggingface\.co/datasets/(?P<rest>[^?#]+)").expect("regex")
});
static PARQUET_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https?://[^/]+/datasets/(?P<repo>.+?)/resolve/(?P<rev>[^/]+)/(?P<rest>.+)$").expect("regex")
});
static RATELIMIT_T_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bt=(\d+)").expect("regex"));

const NOT_NAME_SEGMENTS: &[&str] = &["viewer", "tree", "blob", "resolve", "discussions", "commits", "settings", "raw"];
const MEDIA_TYPES: &[&str] = &["Image", "Audio", "Video", "Pdf"];

/// "owner/name" from a bare name or a huggingface.co/datasets/... link.
pub fn normalize_dataset(raw: &str) -> Result<String> {
    let mut s = raw.trim().to_string();
    if let Some(c) = URL_RE.captures(&s) {
        let rest = c.name("rest").map_or("", |m| m.as_str());
        let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() >= 2 && !NOT_NAME_SEGMENTS.contains(&parts[1].to_lowercase().as_str()) {
            s = format!("{}/{}", parts[0], parts[1]);
        } else if let Some(p) = parts.first() {
            s = p.to_string();
        }
    }
    let s = s.trim_matches('/').to_string();
    if !NAME_RE.is_match(&s) {
        return Err(Problem::new(
            "That doesn't look like a dataset name. Use owner/name or a huggingface.co/datasets link.",
        )
        .on("source.dataset"));
    }
    Ok(s)
}

/// Seconds to wait, from the `ratelimit` header (`"resolvers";r=0;t=136`) or `retry-after`.
pub fn retry_seconds(headers: &HeaderMap) -> Option<u64> {
    let rl = headers.get("ratelimit").and_then(|v| v.to_str().ok()).unwrap_or("");
    if let Some(c) = RATELIMIT_T_RE.captures(rl) {
        return c[1].parse().ok();
    }
    headers.get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse().ok())
}

fn wait_phrase(seconds: Option<u64>) -> String {
    let minutes = seconds.unwrap_or(60).div_ceil(60).max(1);
    if minutes == 1 {
        "in about a minute".into()
    } else {
        format!("in about {minutes} minutes")
    }
}

/// 429 on a file download: fail at once with how long to wait.
pub fn download_rate_limited(headers: &HeaderMap, has_token: bool) -> Problem {
    let hint = if has_token { "" } else { " Adding a Hugging Face token in Settings raises the limit." };
    Problem::status(
        429,
        format!("Hugging Face is limiting downloads from this machine. Try again {}.{hint}", wait_phrase(retry_seconds(headers))),
    )
}

fn api_rate_limited(headers: &HeaderMap) -> Problem {
    match retry_seconds(headers) {
        Some(s) => Problem::status(
            429,
            format!("Hugging Face is limiting requests right now. Try again {}.", wait_phrase(Some(s))),
        ),
        None => Problem::status(429, "Hugging Face is limiting requests right now. Wait a minute and try again."),
    }
}

fn token_problem(has_token: bool) -> Problem {
    if has_token {
        Problem::status(
            403,
            "Your Hugging Face token can't read this dataset. Accept its terms on the dataset page, or use a token with access.",
        )
    } else {
        Problem::status(403, "This dataset needs a Hugging Face token with access. Add one in Settings.")
    }
}

fn upstream_message(status: u16, body: &str) -> String {
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(Value::as_str).map(|s| s.trim().to_string()))
        .unwrap_or_default();
    if msg.is_empty() {
        format!("status {status}")
    } else {
        msg.chars().take(200).collect()
    }
}

/// Talks to Hugging Face for one request or import.
#[derive(Clone)]
pub struct Hf {
    /// datasets-server and Hub API calls.
    pub http: reqwest::Client,
    /// Parquet file reads: no redirects followed (the CDN address is resolved once per file).
    pub data: reqwest::Client,
    pub server: String,
    pub hub: String,
    pub token: Option<String>,
    /// File requests sent (for tests and logs).
    pub file_requests: Arc<AtomicU64>,
}

impl Hf {
    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(t) => req.bearer_auth(t),
            None => req,
        }
    }

    /// True when the Hub API shows this dataset (it exists, even if gated). Errs towards true.
    async fn hub_visible(&self, dataset: &str) -> bool {
        let url = format!("{}/api/datasets/{dataset}", self.hub.trim_end_matches('/'));
        match self.auth(self.http.get(url)).timeout(Duration::from_secs(10)).send().await {
            Ok(r) => r.status() == StatusCode::OK,
            Err(_) => true,
        }
    }

    /// GET from the datasets-server. `exists`: the dataset is known to exist, so a 404 means this
    /// endpoint has nothing for it rather than that the name is wrong.
    async fn get(&self, path: &str, params: &[(&str, &str)], dataset: &str, exists: bool, what: &str) -> Result<Value> {
        let mut url = url::Url::parse(&format!("{}{path}", self.server.trim_end_matches('/')))
            .map_err(|_| Problem::status(500, "The Hugging Face server address in the settings isn't a URL."))?;
        url.query_pairs_mut().extend_pairs(params);
        let resp = match self.auth(self.http.get(url)).timeout(API_TIMEOUT).send().await {
            Ok(r) => r,
            Err(e) if e.is_timeout() => {
                return Err(Problem::status(504, "Hugging Face took too long to answer. Try again in a moment."))
            }
            Err(_) => {
                return Err(Problem::status(502, "Couldn't reach Hugging Face. Check this machine's internet connection."))
            }
        };
        let code = resp.status().as_u16();
        let headers = resp.headers().clone();
        let body = match resp.text().await {
            Ok(b) => b,
            Err(e) if e.is_timeout() => {
                return Err(Problem::status(504, "Hugging Face took too long to answer. Try again in a moment."))
            }
            Err(_) => return Err(Problem::status(502, "Hugging Face sent a reply that couldn't be read. Try again.")),
        };
        if code == 200 {
            return match serde_json::from_str::<Value>(&body) {
                Ok(v @ Value::Object(_)) => Ok(v),
                Ok(_) => Ok(Value::Object(Default::default())),
                Err(_) => Err(Problem::status(502, "Hugging Face sent a reply that couldn't be read. Try again.")),
            };
        }
        let has_token = self.token.is_some();
        if code == 401 || code == 403 {
            // The datasets-server answers 401 both for gated datasets and for names that don't
            // exist. The Hub API shows gated datasets, so ask it which case this is.
            if !exists && !self.hub_visible(dataset).await {
                return Err(Problem::status(
                    404,
                    format!("No dataset called {dataset} on Hugging Face. Check the name. If it is private, add a token with access in Settings."),
                )
                .on("source.dataset"));
            }
            return Err(token_problem(has_token));
        }
        if code == 404 && !exists {
            return Err(Problem::status(404, format!("No dataset called {dataset} on Hugging Face. Check the name."))
                .on("source.dataset"));
        }
        if code == 429 {
            return Err(api_rate_limited(&headers));
        }
        let msg = upstream_message(code, &body);
        if code >= 500 && code != 501 && !exists {
            return Err(Problem::status(502, format!("Hugging Face couldn't open {dataset} ({msg}). Try again shortly.")));
        }
        Err(Problem::new(format!("Hugging Face can't provide {what} for {dataset} yet ({msg}).")))
    }

    /// Every config/split with row counts when known, and whether the counts cover only part of a
    /// very large dataset.
    pub async fn splits(&self, dataset: &str) -> Result<(Vec<HfSplit>, bool)> {
        let data = self.get("/splits", &[("dataset", dataset)], dataset, false, "its splits").await?;
        let mut out: Vec<HfSplit> = data
            .get("splits")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|s| {
                        Some(HfSplit {
                            config: s.get("config")?.as_str()?.to_string(),
                            split: s.get("split")?.as_str()?.to_string(),
                            num_rows: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if out.is_empty() {
            return Err(Problem::new(format!("{dataset} has no splits that Hugging Face can read.")));
        }
        let (sizes, partial) = self.sizes(dataset).await.unwrap_or_default();
        for s in &mut out {
            s.num_rows = sizes.get(&(s.config.clone(), s.split.clone())).copied();
        }
        Ok((out, partial))
    }

    async fn sizes(&self, dataset: &str) -> Result<(HashMap<(String, String), i64>, bool)> {
        let data = self.get("/size", &[("dataset", dataset)], dataset, true, "row counts").await?;
        let mut out = HashMap::new();
        let splits = data.pointer("/size/splits").and_then(Value::as_array).cloned().unwrap_or_default();
        for s in splits {
            if let (Some(c), Some(sp), Some(n)) = (
                s.get("config").and_then(Value::as_str),
                s.get("split").and_then(Value::as_str),
                s.get("num_rows").and_then(Value::as_i64),
            ) {
                out.insert((c.to_string(), sp.to_string()), n);
            }
        }
        Ok((out, data.get("partial").and_then(Value::as_bool).unwrap_or(false)))
    }
}

/// (config, the splits to read, the split whose rows a preview shows).
pub fn choose_split(
    dataset: &str,
    splits: &[HfSplit],
    config: Option<&str>,
    split: Option<&str>,
    all_splits: bool,
) -> Result<(String, Vec<String>, String)> {
    let mut configs: Vec<&str> = Vec::new();
    for s in splits {
        if !configs.contains(&s.config.as_str()) {
            configs.push(&s.config);
        }
    }
    let config = match config.filter(|c| !c.is_empty()) {
        None => (if configs.contains(&"default") { "default" } else { configs[0] }).to_string(),
        Some(c) if configs.contains(&c) => c.to_string(),
        Some(c) => {
            let shown = configs.iter().take(10).copied().collect::<Vec<_>>().join(", ");
            return Err(Problem::new(format!("{dataset} has no config called '{c}'. Configs are: {shown}.")).on("source.config"));
        }
    };
    let names: Vec<String> = splits.iter().filter(|s| s.config == config).map(|s| s.split.clone()).collect();
    let default = if names.iter().any(|n| n == "train") { "train".to_string() } else { names[0].clone() };
    if all_splits {
        return Ok((config, names, default));
    }
    let split = match split.filter(|s| !s.is_empty()) {
        None => default,
        Some(s) if names.iter().any(|n| n == s) => s.to_string(),
        Some(s) => {
            let shown = names.iter().take(10).cloned().collect::<Vec<_>>().join(", ");
            return Err(
                Problem::new(format!("{dataset} ({config}) has no split called '{s}'. Splits are: {shown}.")).on("source.split")
            );
        }
    };
    Ok((config, vec![split.clone()], split))
}

/// What a preview or an import knows about a dataset before reading its files.
pub struct HfDataset {
    pub dataset: String,
    pub config: String,
    /// The splits an import reads, in order.
    pub splits: Vec<String>,
    /// The split the first rows come from.
    pub shown_split: String,
    pub hf_splits: Vec<HfSplit>,
    /// Rows in the splits read, when every count is known.
    pub num_rows: Option<u64>,
    /// Hugging Face converted only part of the dataset to Parquet.
    pub partial: bool,
    /// The shown split's first rows (cells Hugging Face shortened are null).
    pub first_rows: MemoryTable,
}

fn temporal_feature(t: &Value) -> bool {
    t.get("_type").and_then(Value::as_str) == Some("Value")
        && t.get("dtype").and_then(Value::as_str).is_some_and(|d| {
            d.starts_with("timestamp") || d.starts_with("date") || d.starts_with("time") || d.starts_with("duration")
        })
}

impl Hf {
    pub async fn dataset(&self, raw: &str, config: Option<&str>, split: Option<&str>, all_splits: bool) -> Result<HfDataset> {
        let dataset = normalize_dataset(raw)?;
        let (hf_splits, partial) = self.splits(&dataset).await?;
        let (config, splits, shown) = choose_split(&dataset, &hf_splits, config, split, all_splits)?;
        let first = self.get("/first-rows", &[("dataset", &dataset), ("config", &config), ("split", &shown)], &dataset, true, "a preview").await;
        let counts: Vec<Option<i64>> = splits
            .iter()
            .map(|s| hf_splits.iter().find(|x| x.config == config && &x.split == s).and_then(|x| x.num_rows))
            .collect();
        let num_rows = if counts.iter().all(Option::is_some) {
            Some(counts.iter().map(|c| c.unwrap_or(0).max(0) as u64).sum())
        } else {
            None
        };
        let mut first_rows = match first {
            Ok(data) => first_rows_table(&data),
            // Some datasets have a Parquet export but no first rows (too large to compute): read
            // the start of the export instead.
            Err(p) if p.status == 422 => match self.first_rows_from_parquet(&dataset, &config, &shown).await {
                Ok(t) => t,
                Err(e) if e.status == 422 => return Err(p),
                Err(e) => return Err(e),
            },
            Err(p) => return Err(p),
        };
        first_rows.total_rows = num_rows;
        if partial {
            first_rows.warnings.push(
                "Hugging Face has converted only part of this dataset to Parquet, so an import reads that part only.".into(),
            );
        }
        Ok(HfDataset { dataset, config, splits, shown_split: shown, hf_splits, num_rows, partial, first_rows })
    }

    async fn first_rows_from_parquet(&self, dataset: &str, config: &str, split: &str) -> Result<MemoryTable> {
        let (shards, _) = self.parquet_files(dataset, config, &[split.to_string()]).await?;
        let mut table = HfParquetTable::open(self, shards, None, HashMap::new(), Cancel::new()).await?;
        let names = table.names().to_vec();
        let hints = table.hints();
        let rows = tokio::task::spawn_blocking(move || crate::table::read_rows(&mut table, None, 0, Some(FALLBACK_ROWS)))
            .await
            .map_err(|_| read_problem("the reader stopped."))??;
        let mut t = MemoryTable::new(names, rows, "huggingface", None);
        t.hints = hints;
        Ok(t)
    }
}

/// Rows read from the Parquet export when Hugging Face has no first rows for a split.
const FALLBACK_ROWS: u64 = 100;

/// The datasets-server's first rows as a table: media columns left out, shortened cells null.
fn first_rows_table(data: &Value) -> MemoryTable {
    let mut names: Vec<String> = Vec::new();
    let mut hints: Vec<Option<Hint>> = Vec::new();
    let mut skip: Vec<String> = Vec::new();
    for f in data.get("features").and_then(Value::as_array).into_iter().flatten() {
        let Some(name) = f.get("name").and_then(Value::as_str) else { continue };
        let t = f.get("type").cloned().unwrap_or(Value::Null);
        let ty = t.get("_type").and_then(Value::as_str).unwrap_or("");
        let binary = matches!(t.get("dtype").and_then(Value::as_str), Some("binary" | "large_binary"));
        if MEDIA_TYPES.contains(&ty) || binary {
            skip.push(name.to_string());
        } else {
            names.push(name.to_string());
            hints.push(temporal_feature(&t).then_some(Hint::Temporal));
        }
    }
    let raw_rows: Vec<&Value> = data
        .get("rows")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter(|r| r.get("row").is_some_and(Value::is_object)).collect())
        .unwrap_or_default();
    if names.is_empty() {
        // No features: the first row's keys (sorted).
        if let Some(Value::Object(o)) = raw_rows.first().and_then(|r| r.get("row")) {
            names = o.keys().filter(|k| !skip.contains(k)).cloned().collect();
            hints = vec![None; names.len()];
        }
    }
    let rows: Vec<Row> = raw_rows
        .iter()
        .map(|r| {
            let truncated: Vec<&str> = r
                .get("truncated_cells")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let row = r.get("row").and_then(Value::as_object);
            names
                .iter()
                .map(|n| {
                    if truncated.contains(&n.as_str()) {
                        return Value::Null;
                    }
                    row.and_then(|o| o.get(n)).cloned().unwrap_or(Value::Null)
                })
                .collect()
        })
        .collect();
    let mut first_rows = MemoryTable::new(names, rows, "huggingface", None);
    first_rows.hints = hints;
    first_rows
}

impl Hf {
    /// The Parquet files of these splits, in split order.
    pub async fn parquet_files(&self, dataset: &str, config: &str, splits: &[String]) -> Result<(Vec<Shard>, bool)> {
        let data = self.get("/parquet", &[("dataset", dataset)], dataset, true, "a Parquet export").await?;
        let files: Vec<&Value> = data.get("parquet_files").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
        let pending = data.get("pending").and_then(Value::as_array).is_some_and(|p| !p.is_empty());
        let mut shards = Vec::new();
        for split in splits {
            let mut mine: Vec<&Value> = files
                .iter()
                .copied()
                .filter(|f| {
                    f.get("config").and_then(Value::as_str) == Some(config)
                        && f.get("split").and_then(Value::as_str) == Some(split.as_str())
                })
                .collect();
            if mine.is_empty() {
                if pending {
                    return Err(Problem::status(
                        409,
                        "Hugging Face is still preparing a Parquet copy of this dataset. Try again in a few minutes.",
                    ));
                }
                return Err(Problem::new(format!(
                    "Hugging Face has no Parquet export for {dataset} ({config}, {split}), so it can't be read yet."
                )));
            }
            let key = |f: &Value| {
                f.get("filename").and_then(Value::as_str).or_else(|| f.get("url").and_then(Value::as_str)).unwrap_or("").to_string()
            };
            mine.sort_by_key(|f| key(f));
            for f in mine {
                let raw = f.get("url").and_then(Value::as_str).unwrap_or("");
                let c = PARQUET_URL_RE.captures(raw).ok_or_else(|| {
                    Problem::status(502, "Hugging Face returned a Parquet file address that couldn't be understood.")
                })?;
                let url = format!("{}/datasets/{}/resolve/{}/{}", self.hub.trim_end_matches('/'), &c["repo"], &c["rev"], &c["rest"]);
                shards.push(Shard { url, size: f.get("size").and_then(Value::as_u64), split: split.clone() });
            }
        }
        Ok((shards, data.get("partial").and_then(Value::as_bool).unwrap_or(false)))
    }
}

/// One Parquet file of the export.
#[derive(Debug, Clone)]
pub struct Shard {
    /// https://huggingface.co/datasets/{repo}/resolve/refs%2Fconvert%2Fparquet/{config}/{split}/0000.parquet
    pub url: String,
    pub size: Option<u64>,
    pub split: String,
}

// ------------------------------------------------------------------------------------ range reads

fn read_problem(detail: impl std::fmt::Display) -> Problem {
    let detail: String = detail.to_string().chars().take(200).collect();
    Problem::status(502, format!("Couldn't read the Parquet files from Hugging Face: {detail}"))
}

fn to_parquet_error(p: Problem) -> ParquetError {
    ParquetError::External(Box::new(p))
}

/// The Problem inside a Parquet error, or a sentence for the error itself.
pub fn from_parquet_error(e: ParquetError) -> Problem {
    if let ParquetError::External(inner) = e {
        return match inner.downcast::<Problem>() {
            Ok(p) => *p,
            Err(other) => read_problem(other),
        };
    }
    read_problem(e)
}

/// A Parquet file on Hugging Face read with range requests. The `resolve` address redirects to a
/// CDN; it is resolved once (one rate-limited request per file) and ranges go to the CDN.
pub struct HttpFile {
    hf: Hf,
    url: String,
    size: Option<u64>,
    /// The resolved address, and whether it gets the token.
    target: Option<(String, bool)>,
    cancel: Cancel,
}

impl HttpFile {
    pub fn new(hf: &Hf, shard: &Shard, cancel: Cancel) -> Self {
        Self { hf: hf.clone(), url: shard.url.clone(), size: shard.size, target: None, cancel }
    }

    async fn resolve(&mut self) -> Result<()> {
        self.hf.file_requests.fetch_add(1, Ordering::Relaxed);
        let req = self.hf.auth(self.hf.data.head(&self.url)).timeout(Duration::from_secs(20));
        let resp = tokio::select! {
            r = req.send() => r.map_err(|e| read_problem(describe_reqwest_error(&e)))?,
            _ = self.cancel.cancelled() => return Err(Problem::cancelled()),
        };
        let status = resp.status();
        let headers = resp.headers();
        let header_u64 = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
        if status.is_success() {
            if self.size.is_none() {
                self.size = header_u64(CONTENT_LENGTH.as_str());
            }
            self.target = Some((self.url.clone(), true));
            return Ok(());
        }
        if status.is_redirection() {
            let loc = headers.get(LOCATION).and_then(|v| v.to_str().ok()).unwrap_or("");
            let base = url::Url::parse(&self.url).map_err(read_problem)?;
            let target = base.join(loc).map_err(read_problem)?;
            if self.size.is_none() {
                self.size = header_u64("x-linked-size").or_else(|| header_u64(CONTENT_LENGTH.as_str()));
            }
            let same_origin = target.origin() == base.origin();
            self.target = Some((target.to_string(), same_origin));
            return Ok(());
        }
        Err(match status.as_u16() {
            429 => download_rate_limited(headers, self.hf.token.is_some()),
            401 | 403 => token_problem(self.hf.token.is_some()),
            code => read_problem(format!("status {code}.")),
        })
    }

    async fn size(&mut self) -> Result<u64> {
        if self.size.is_none() {
            self.resolve().await?;
        }
        self.size.ok_or_else(|| read_problem("Hugging Face didn't say how large a file is."))
    }

    /// Bytes `range` of the file. Retries network errors and server errors; re-resolves an expired
    /// CDN address; fails at once on 429.
    pub async fn fetch(&mut self, range: Range<usize>) -> Result<Bytes> {
        let want = range.end.saturating_sub(range.start);
        if want == 0 {
            return Ok(Bytes::new());
        }
        let mut last = String::new();
        let mut re_resolved = false;
        for attempt in 0..4u32 {
            if attempt > 0 {
                let pause = Duration::from_millis(500 * 2u64.pow(attempt - 1));
                tokio::select! {
                    _ = tokio::time::sleep(pause) => {}
                    _ = self.cancel.cancelled() => return Err(Problem::cancelled()),
                }
            }
            if self.target.is_none() {
                self.resolve().await?;
            }
            let Some((url, auth)) = self.target.clone() else { continue };
            self.hf.file_requests.fetch_add(1, Ordering::Relaxed);
            let mut req = self.hf.data.get(&url).header(RANGE, format!("bytes={}-{}", range.start, range.end - 1));
            if auth {
                req = self.hf.auth(req);
            }
            let sent = tokio::select! {
                r = req.send() => r,
                _ = self.cancel.cancelled() => return Err(Problem::cancelled()),
            };
            let resp = match sent {
                Ok(r) => r,
                Err(e) => {
                    last = describe_reqwest_error(&e);
                    continue;
                }
            };
            let status = resp.status().as_u16();
            match status {
                200 | 206 => {
                    let body = tokio::select! {
                        b = resp.bytes() => b,
                        _ = self.cancel.cancelled() => return Err(Problem::cancelled()),
                    };
                    match body {
                        Ok(b) if status == 206 && b.len() == want => return Ok(b),
                        // The server ignored the range and sent the whole file.
                        Ok(b) if status == 200 && b.len() >= range.end => return Ok(b.slice(range.clone())),
                        Ok(b) => last = format!("expected {want} bytes, got {}", b.len()),
                        Err(e) => last = describe_reqwest_error(&e),
                    }
                }
                429 => return Err(download_rate_limited(resp.headers(), self.hf.token.is_some())),
                401 | 403 | 410 if url != self.url && !re_resolved => {
                    // The signed CDN address expired: resolve again.
                    re_resolved = true;
                    self.target = None;
                    last = format!("status {status}");
                }
                401 | 403 => return Err(token_problem(self.hf.token.is_some())),
                s if s >= 500 => last = format!("status {s}"),
                s => return Err(read_problem(format!("status {s}."))),
            }
        }
        Err(read_problem(last))
    }
}

impl AsyncFileReader for HttpFile {
    fn get_bytes(&mut self, range: Range<usize>) -> BoxFuture<'_, parquet::errors::Result<Bytes>> {
        async move { self.fetch(range).await.map_err(to_parquet_error) }.boxed()
    }

    /// Ranges close together (a row group's projected column chunks) go out as one request.
    fn get_byte_ranges(&mut self, ranges: Vec<Range<usize>>) -> BoxFuture<'_, parquet::errors::Result<Vec<Bytes>>> {
        async move {
            let mut order: Vec<usize> = (0..ranges.len()).collect();
            order.sort_by_key(|&i| ranges[i].start);
            let mut merged: Vec<Range<usize>> = Vec::new();
            for &i in &order {
                let r = &ranges[i];
                match merged.last_mut() {
                    Some(m) if r.start <= m.end + COALESCE_GAP => m.end = m.end.max(r.end),
                    _ => merged.push(r.clone()),
                }
            }
            let mut fetched: Vec<(Range<usize>, Bytes)> = Vec::with_capacity(merged.len());
            for m in merged {
                let b = self.fetch(m.clone()).await.map_err(to_parquet_error)?;
                fetched.push((m, b));
            }
            Ok(ranges
                .iter()
                .map(|r| {
                    let (m, b) = fetched.iter().find(|(m, _)| m.start <= r.start && r.end <= m.end).expect("covered");
                    b.slice(r.start - m.start..r.end - m.start)
                })
                .collect())
        }
        .boxed()
    }

    fn get_metadata(&mut self) -> BoxFuture<'_, parquet::errors::Result<Arc<ParquetMetaData>>> {
        async move {
            let size = self.size().await.map_err(to_parquet_error)? as usize;
            let meta = ParquetMetaDataReader::new()
                .with_prefetch_hint(Some(footer_prefetch(size)))
                .load_and_finish(&mut *self, size)
                .await?;
            Ok(Arc::new(meta))
        }
        .boxed()
    }
}

// ------------------------------------------------------------------------------------ the table

/// Rows of a dataset's Parquet export, across shards and splits, read with range requests. Scans
/// block on the async reader, so they run on a blocking thread.
pub struct HfParquetTable {
    hf: Hf,
    shards: Vec<Shard>,
    names: Vec<String>,
    hints: Vec<Option<Hint>>,
    index: HashMap<String, usize>,
    total_rows: Option<u64>,
    /// Rows per split from /size, to skip whole splits for an offset without opening their files.
    split_rows: HashMap<String, u64>,
    first: Option<(HttpFile, Arc<ParquetMetaData>)>,
    handle: Handle,
    cancel: Cancel,
}

impl HfParquetTable {
    /// Opens the first file (one resolve and one footer read) to learn the columns. Errors such as
    /// rate limits and missing access show up here, before an import starts.
    pub async fn open(
        hf: &Hf,
        shards: Vec<Shard>,
        total_rows: Option<u64>,
        split_rows: HashMap<String, u64>,
        cancel: Cancel,
    ) -> Result<Self> {
        let Some(first) = shards.first() else { return Err(read_problem("there are no files.")) };
        let mut file = HttpFile::new(hf, first, cancel.clone());
        let meta = file.get_metadata().await.map_err(from_parquet_error)?;
        let arrow = ArrowReaderMetadata::try_new(meta.clone(), ArrowReaderOptions::new()).map_err(from_parquet_error)?;
        let (names, _roots, hints) = usable_columns(arrow.schema());
        let index = names.iter().enumerate().map(|(i, n)| (n.clone(), i)).collect();
        Ok(Self {
            hf: hf.clone(),
            shards,
            names,
            hints,
            index,
            total_rows,
            split_rows,
            first: Some((file, meta)),
            handle: Handle::current(),
            cancel,
        })
    }

    fn block_on<T>(&self, fut: impl std::future::Future<Output = T>) -> Result<T> {
        let cancel = self.cancel.clone();
        self.handle.block_on(async move {
            tokio::select! {
                v = fut => Ok(v),
                _ = cancel.cancelled() => Err(Problem::cancelled()),
            }
        })
    }
}

impl Table for HfParquetTable {
    fn format(&self) -> &'static str {
        "huggingface"
    }
    fn names(&self) -> &[String] {
        &self.names
    }
    fn total_rows(&self) -> Option<u64> {
        self.total_rows
    }
    fn small(&self) -> bool {
        false
    }
    fn hints(&self) -> Vec<Option<Hint>> {
        self.hints.clone()
    }
    fn scan(&mut self, columns: Option<&[usize]>, offset: u64, limit: Option<u64>, sink: &mut dyn RowSink) -> Result<()> {
        let wanted: Vec<&String> = match columns {
            Some(c) => c.iter().filter_map(|&i| self.names.get(i)).collect(),
            None => self.names.iter().collect(),
        };
        let mut skip = offset;
        let mut window = Window::new(0, limit);
        let mut current: Option<String> = None;
        let mut skipping_split = false;
        for i in 0..self.shards.len() {
            if window.done() || self.cancel.is_cancelled() {
                break;
            }
            let shard = self.shards[i].clone();
            if current.as_deref() != Some(shard.split.as_str()) {
                current = Some(shard.split.clone());
                skipping_split = false;
                if skip > 0 {
                    if let Some(&n) = self.split_rows.get(&shard.split) {
                        if skip >= n {
                            skip -= n;
                            skipping_split = true;
                        }
                    }
                }
                if !skipping_split {
                    sink.part(&shard.split);
                }
            }
            if skipping_split {
                continue;
            }
            let (file, meta) = match self.first.take() {
                Some(f) if i == 0 => f,
                _ => {
                    let mut file = HttpFile::new(&self.hf, &shard, self.cancel.clone());
                    let meta = self.block_on(file.get_metadata())?.map_err(from_parquet_error)?;
                    (file, meta)
                }
            };
            let Some((groups, first_skip)) = plan_row_groups(&meta, &mut skip) else { continue };
            let arrow = ArrowReaderMetadata::try_new(meta, ArrowReaderOptions::new()).map_err(from_parquet_error)?;
            let roots: Vec<usize> = arrow
                .schema()
                .fields()
                .iter()
                .enumerate()
                .filter(|(_, f)| wanted.iter().any(|w| *w == f.name()))
                .map(|(j, _)| j)
                .collect();
            let mask = ProjectionMask::roots(arrow.parquet_schema(), roots);
            let mut stream = ParquetRecordBatchStreamBuilder::new_with_metadata(file, arrow)
                .with_projection(mask)
                .with_row_groups(groups)
                .with_batch_size(batch_size(window.remaining, first_skip))
                .build()
                .map_err(from_parquet_error)?;
            window.skip = first_skip;
            loop {
                let next = self.block_on(stream.next())?;
                let Some(batch) = next else { break };
                let batch = batch.map_err(from_parquet_error)?;
                if emit_batch(&batch, &self.index, self.names.len(), &mut window, sink) == Flow::Stop || window.done() {
                    return Ok(());
                }
            }
        }
        if self.cancel.is_cancelled() {
            return Err(Problem::cancelled());
        }
        Ok(())
    }
}
