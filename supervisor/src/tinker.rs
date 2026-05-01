use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::warn;

#[derive(Clone)]
pub struct TinkerClient {
    http: Client,
    base_url: String,
    api_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TinkerJob {
    pub id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub gpu_seconds: Option<f64>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub step: Option<i64>,
    #[serde(default)]
    pub metrics: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StreamMetric {
    #[serde(default)]
    pub step: i64,
    pub name: String,
    pub value: f64,
}

impl TinkerClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("stellarator-supervisor/0.1")
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
        })
    }

    pub async fn get_job(&self, job_id: &str) -> Result<Option<TinkerJob>> {
        let url = format!("{}/jobs/{}", self.base_url, job_id);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        match resp.status() {
            StatusCode::NOT_FOUND => Ok(None),
            s if s.is_success() => {
                let job = resp.json::<TinkerJob>().await.context("decode tinker job")?;
                Ok(Some(job))
            }
            s => {
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("tinker {s}: {body}")
            }
        }
    }

    /// Spawn a task that consumes the SSE metrics stream for `job_id` and
    /// forwards parsed events to the returned receiver. Best-effort: on any
    /// network error the task exits and the caller falls back to polling.
    pub fn spawn_metrics_stream(
        &self,
        job_id: String,
    ) -> (mpsc::Receiver<StreamMetric>, tokio::task::JoinHandle<()>) {
        let (tx, rx) = mpsc::channel(64);
        let http = self.http.clone();
        let base = self.base_url.clone();
        let key = self.api_key.clone();
        let handle = tokio::spawn(async move {
            let url = format!("{}/jobs/{}/metrics/stream", base, job_id);
            let resp = match http.get(&url).bearer_auth(&key).send().await {
                Ok(r) => r,
                Err(e) => {
                    warn!(%job_id, error=?e, "tinker SSE connect failed");
                    return;
                }
            };
            if !resp.status().is_success() {
                warn!(%job_id, status=?resp.status(), "tinker SSE non-success");
                return;
            }
            let mut byte_stream = resp.bytes_stream();
            let mut buf: Vec<u8> = Vec::new();
            while let Some(chunk) = byte_stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        warn!(%job_id, error=?e, "tinker SSE chunk error");
                        return;
                    }
                };
                buf.extend_from_slice(&chunk);
                while let Some(pos) = find_double_newline(&buf) {
                    let event: Vec<u8> = buf.drain(..pos + 2).collect();
                    if let Some(json) = extract_data(&event) {
                        if let Ok(ev) = serde_json::from_str::<StreamMetric>(&json) {
                            if tx.send(ev).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        (rx, handle)
    }
}

/// Test-only re-export so integration tests can exercise the parser.
#[doc(hidden)]
pub fn sse_find_terminator_for_test(buf: &[u8]) -> Option<usize> {
    find_double_newline(buf)
}

/// Test-only re-export so integration tests can exercise the parser.
#[doc(hidden)]
pub fn sse_extract_data_for_test(event: &[u8]) -> Option<String> {
    extract_data(event)
}

/// Find the end of an SSE event (blank-line terminator) in `buf`.
/// Returns an index `pos` such that `buf[..pos+2]` covers the event including
/// the trailing `\n\n`. Only LF terminators are accepted — the upstream Tinker
/// SSE stream is normalized to LF, and the rest of the parser strips a single
/// trailing `\r` per line, which together cover CRLF feeds in practice.
pub(crate) fn find_double_newline(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\n\n")
}

/// Parse an SSE event block into the joined `data:` payload.
///
/// Rules implemented (per the WHATWG SSE spec):
///   * Lines beginning with `:` are comments and are ignored (heartbeats).
///   * Lines without a colon are field names with empty values — ignored here.
///   * For `data:` lines, an optional single leading space after the colon is
///     stripped; remaining content is appended.
///   * Multiple `data:` lines within one event are joined with a literal `\n`.
///   * Other fields (`event:`, `id:`, `retry:`) are ignored — we only care
///     about the JSON payload.
pub(crate) fn extract_data(event: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(event).ok()?;
    let mut parts: Vec<String> = Vec::new();
    for raw in s.split('\n') {
        // Strip a single trailing CR (handles CRLF line endings).
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() {
            continue;
        }
        if line.starts_with(':') {
            // SSE comment / keep-alive heartbeat.
            continue;
        }
        let Some(rest) = line.strip_prefix("data:") else {
            continue;
        };
        // Per spec, strip exactly one leading space if present.
        let value = rest.strip_prefix(' ').unwrap_or(rest);
        parts.push(value.to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}
