//! A typed client for the Stellarator API, with the same paths and JSON the web UI uses
//! (frontend/src/lib/api.ts, frontend/src/lib/types.ts). Two transports, one per target:
//!
//! - Native (macOS, Windows, Linux): in-process. Each request goes through the server's axum
//!   router with `tower::ServiceExt::oneshot`, on the server's own runtime ([`crate::backend`]).
//!   Nothing listens on a port.
//! - Web (wasm32): nsh's `fetch` to the same `/api` paths on the page's own origin, the server
//!   that serves the app.
//!
//! The screens only see [`Api`] and the types; they do not know which transport runs.
//! The types below mirror the fields of the server's JSON the native screens read; serde ignores
//! the rest, so the server can add fields without breaking the app.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// A request that failed: the server's `detail` message or what went wrong on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    /// The HTTP status, when the server answered.
    pub status: Option<u16>,
    /// A sentence for the UI.
    pub message: String,
}

impl ApiError {
    fn transport(message: impl Into<String>) -> Self {
        ApiError { status: None, message: message.into() }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(status) => write!(f, "{} (HTTP {status})", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ApiError {}

/// An HTTP method the client sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Patch,
    Delete,
}

impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

/// The API. Cheap to clone.
#[derive(Clone)]
pub struct Api {
    transport: Transport,
}

/// How requests reach the server.
#[derive(Clone)]
enum Transport {
    /// The server's router in this process, run on the server's runtime.
    #[cfg(not(target_arch = "wasm32"))]
    InProcess { router: axum::Router, runtime: tokio::runtime::Handle },
    /// The browser's network stack, to the page's origin (`base`, no trailing slash).
    #[cfg(target_arch = "wasm32")]
    Fetch { base: String },
}

impl Api {
    /// The in-process API of the server's router, run on `runtime`.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn in_process(router: axum::Router, runtime: tokio::runtime::Handle) -> Self {
        Api { transport: Transport::InProcess { router, runtime } }
    }

    /// The API of the server at `base` (the page's origin, such as `http://127.0.0.1:8000`).
    #[cfg(target_arch = "wasm32")]
    pub fn fetch(base: impl Into<String>) -> Self {
        Api { transport: Transport::Fetch { base: base.into().trim_end_matches('/').to_owned() } }
    }

    /// `GET /api/health`.
    pub async fn health(&self) -> Result<Health, ApiError> {
        self.get("/api/health").await
    }

    /// `GET /api/runs`: every run, newest first.
    pub async fn runs(&self) -> Result<Vec<Run>, ApiError> {
        self.get("/api/runs").await
    }

    /// `GET` `path` and read its JSON.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let (status, body) = self.send(Method::Get, path, None).await?;
        decode(status, &body)
    }

    /// One request: its status and body. `body` is JSON.
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<(u16, Vec<u8>), ApiError> {
        match &self.transport {
            #[cfg(not(target_arch = "wasm32"))]
            Transport::InProcess { router, runtime } => {
                in_process::send(router.clone(), runtime, method, path, body).await
            }
            #[cfg(target_arch = "wasm32")]
            Transport::Fetch { base } => {
                let mut request = nsh_core::platform::HttpRequest::new(method.as_str(), format!("{base}{path}"))
                    .header("accept", "application/json");
                if let Some(json) = body {
                    request = request.header("content-type", "application/json").body(json.to_string());
                }
                let response = nsh_core::prelude::fetch(request).await.map_err(|e| ApiError::transport(e.to_string()))?;
                Ok((response.status(), response.body))
            }
        }
    }
}

/// The in-process transport: a request through the router on the server's runtime.
#[cfg(not(target_arch = "wasm32"))]
mod in_process {
    use axum::body::Body;
    use axum::http::{Request, header};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::{ApiError, Method};

    /// The largest response body read, as a guard: 64 MiB.
    const MAX_BODY: usize = 64 << 20;

    pub(super) async fn send(
        router: axum::Router,
        runtime: &tokio::runtime::Handle,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<(u16, Vec<u8>), ApiError> {
        let mut request = Request::builder()
            .method(method.as_str())
            .uri(path)
            .header(header::ACCEPT, "application/json");
        let body = match body {
            Some(json) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        let request = request.body(body).map_err(|e| ApiError::transport(e.to_string()))?;
        // Awaited from nsh's executor: the JoinHandle is a plain future.
        let task = runtime.spawn(async move {
            let response = router.oneshot(request).await.unwrap_or_else(|never| match never {});
            let status = response.status().as_u16();
            let bytes = response
                .into_body()
                .collect()
                .await
                .map_err(|e| ApiError { status: Some(status), message: e.to_string() })?
                .to_bytes();
            if bytes.len() > MAX_BODY {
                return Err(ApiError { status: Some(status), message: "The response is too large.".into() });
            }
            Ok((status, bytes.to_vec()))
        });
        task.await.map_err(|e| ApiError::transport(format!("The request did not finish: {e}")))?
    }
}

/// The JSON of a response: `T` on success, else the server's `detail` (FastAPI's shape, which the
/// Rust server keeps).
fn decode<T: DeserializeOwned>(status: u16, bytes: &[u8]) -> Result<T, ApiError> {
    if (200..300).contains(&status) {
        return serde_json::from_slice(bytes).map_err(|e| ApiError {
            status: Some(status),
            message: format!("The server's answer could not be read: {e}"),
        });
    }
    #[derive(Deserialize)]
    struct Detail {
        detail: serde_json::Value,
    }
    let message = match serde_json::from_slice::<Detail>(bytes) {
        Ok(Detail { detail: serde_json::Value::String(s) }) => s,
        Ok(Detail { detail }) => detail.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).trim().to_owned(),
    };
    let message = if message.is_empty() { format!("The server answered {status}.") } else { message };
    Err(ApiError { status: Some(status), message })
}

/// `GET /api/health`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Health {
    pub ok: bool,
    pub version: String,
}

/// A run's state (`RunStatus` in frontend/src/lib/types.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Pausing,
    Paused,
    Completed,
}

impl RunStatus {
    /// Running or pausing: the list refreshes while one is.
    pub fn is_active(self) -> bool {
        matches!(self, RunStatus::Running | RunStatus::Pausing)
    }
}

/// A run as `GET /api/runs` lists it (`Run` in frontend/src/lib/types.ts), the fields the list
/// shows.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Run {
    pub id: i64,
    pub name: String,
    pub endpoint_name: String,
    pub model: String,
    pub prompt_set_name: String,
    pub concurrency: i64,
    pub status: RunStatus,
    pub total: i64,
    pub done: i64,
    pub failed: i64,
    pub truncated: i64,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_reads_from_the_servers_json() {
        let json = r#"{"id":4,"name":"Distill gsm8k","endpoint_id":1,"endpoint_name":"Local vLLM",
            "model":"mock-reasoning","prompt_set_id":1,"prompt_set_name":"gsm8k train","params":{},
            "concurrency":4,"status":"paused","status_reason":null,"total":400,"done":120,"failed":3,
            "truncated":0,"pending":277,"prompt_tokens":0,"completion_tokens":0,"reasoning_tokens":0,
            "active_seconds":12.5,"created_at":"2026-11-18T21:38:37.491158Z","started_at":null,
            "finished_at":null}"#;
        let run: Run = serde_json::from_str(json).unwrap();
        assert_eq!(run.status, RunStatus::Paused);
        assert_eq!((run.done, run.failed, run.total), (120, 3, 400));
    }

    #[test]
    fn errors_carry_the_servers_detail() {
        let err = decode::<Health>(404, br#"{"detail":"Run not found."}"#).unwrap_err();
        assert_eq!(err, ApiError { status: Some(404), message: "Run not found.".into() });
    }
}
