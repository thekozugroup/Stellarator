//! A typed client for the Stellarator API, with the same paths and JSON the web UI uses
//! (frontend/src/lib/api.ts, frontend/src/lib/types.ts). Two transports, one per target:
//!
//! - Native (macOS, Windows, Linux): in-process. Each request goes through the server's axum
//!   router with `tower::ServiceExt::oneshot`, on the server's own runtime ([`crate::backend`]).
//!   Nothing listens on a port.
//! - Web (wasm32): nsh's `fetch` to the same `/api` paths on the page's own origin, the server
//!   that serves the app.
//!
//! The screens only see [`Api`] and the types; they do not know which transport runs. The
//! request and response types are the server's own (`stellarator-api`, crates/api), re-exported
//! here as [`types`].

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
pub use stellarator_api as types;

use types::{EndpointOut, EndpointTestIn, EndpointTestOut, Location, RunOut};

/// A request that failed: the server's `detail` message or what went wrong on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    /// The HTTP status, when the server answered.
    pub status: Option<u16>,
    /// A sentence for the UI.
    pub message: String,
    /// Messages for single fields of the request (`{"base_url": "…"}`), when the server sent them.
    pub fields: BTreeMap<String, String>,
}

impl ApiError {
    fn transport(message: impl Into<String>) -> Self {
        ApiError { status: None, message: message.into(), fields: BTreeMap::new() }
    }

    fn answered(status: u16, message: impl Into<String>) -> Self {
        ApiError { status: Some(status), message: message.into(), fields: BTreeMap::new() }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The message alone, as the web UI shows it; the status is in `status`.
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

/// An HTTP method the client sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
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
    /// The browser's network stack, to the page's origin.
    #[cfg(target_arch = "wasm32")]
    Fetch,
}

impl Api {
    /// The in-process API of the server's router, run on `runtime`.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn in_process(router: axum::Router, runtime: tokio::runtime::Handle) -> Self {
        Api { transport: Transport::InProcess { router, runtime } }
    }

    /// The API of the server that serves the page.
    #[cfg(target_arch = "wasm32")]
    pub fn same_origin() -> Self {
        Api { transport: Transport::Fetch }
    }

    /// `GET /api/health`.
    pub async fn health(&self) -> Result<Health, ApiError> {
        self.get("/api/health").await
    }

    /// `GET /api/runs`: every run, newest first.
    pub async fn runs(&self) -> Result<Vec<RunOut>, ApiError> {
        self.get("/api/runs").await
    }

    /// `GET /api/endpoints`: every endpoint, as the list shows them.
    pub async fn endpoints(&self) -> Result<Vec<EndpointOut>, ApiError> {
        self.get("/api/endpoints").await
    }

    /// `GET /api/endpoint-presets`: the starting points of Add endpoint.
    pub async fn endpoint_presets(&self) -> Result<Vec<EndpointPreset>, ApiError> {
        self.get("/api/endpoint-presets").await
    }

    /// `POST /api/endpoints`: a new endpoint (`EndpointIn`'s fields). The server checks it at once.
    pub async fn create_endpoint(&self, body: &serde_json::Value) -> Result<EndpointOut, ApiError> {
        self.post("/api/endpoints", body).await
    }

    /// `PATCH /api/endpoints/{id}` (`EndpointPatch`'s fields; an absent field is left as it is).
    pub async fn update_endpoint(&self, id: i64, body: &serde_json::Value) -> Result<EndpointOut, ApiError> {
        self.patch(&format!("/api/endpoints/{id}"), body).await
    }

    /// `DELETE /api/endpoints/{id}`.
    pub async fn delete_endpoint(&self, id: i64) -> Result<(), ApiError> {
        self.delete(&format!("/api/endpoints/{id}")).await
    }

    /// `POST /api/endpoints/{id}/check`: reads the model list again and keeps the result.
    pub async fn check_endpoint(&self, id: i64) -> Result<EndpointOut, ApiError> {
        self.post(&format!("/api/endpoints/{id}/check"), &serde_json::json!({})).await
    }

    /// `POST /api/endpoints/test`: tries a base URL and key without saving them.
    pub async fn test_endpoint(&self, body: &EndpointTestIn) -> Result<EndpointTestOut, ApiError> {
        self.post("/api/endpoints/test", body).await
    }

    /// `GET /api/settings`.
    pub async fn settings(&self) -> Result<Settings, ApiError> {
        self.get("/api/settings").await
    }

    /// `PUT /api/settings`: `hf_token` set to a token saves it, `""` removes it.
    pub async fn save_hf_token(&self, token: &str) -> Result<Settings, ApiError> {
        self.put("/api/settings", &serde_json::json!({ "hf_token": token })).await
    }

    /// `GET` `path` and read its JSON.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let (status, body) = self.send(Method::Get, path, None).await?;
        decode(status, &body)
    }

    /// `POST` `body` as JSON to `path` and read the JSON answer.
    pub async fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T, ApiError> {
        self.json(Method::Post, path, body).await
    }

    /// `PUT` `body` as JSON to `path` and read the JSON answer.
    pub async fn put<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T, ApiError> {
        self.json(Method::Put, path, body).await
    }

    /// `PATCH` `body` as JSON to `path` and read the JSON answer.
    pub async fn patch<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T, ApiError> {
        self.json(Method::Patch, path, body).await
    }

    /// `DELETE` `path`; any 2xx answer (204 has no body) is success.
    pub async fn delete(&self, path: &str) -> Result<(), ApiError> {
        let (status, body) = self.send(Method::Delete, path, None).await?;
        if (200..300).contains(&status) { Ok(()) } else { Err(error_of(status, &body)) }
    }

    async fn json<T: DeserializeOwned>(&self, method: Method, path: &str, body: &impl Serialize) -> Result<T, ApiError> {
        let body = serde_json::to_value(body).map_err(|e| ApiError::transport(e.to_string()))?;
        let (status, bytes) = self.send(method, path, Some(body)).await?;
        decode(status, &bytes)
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
            Transport::Fetch => {
                let base = crate::web::page_origin().await;
                let mut request = nsh_core::platform::HttpRequest::new(method.as_str(), format!("{base}{path}"))
                    .header("accept", "application/json");
                if let Some(json) = body {
                    request = request.header("content-type", "application/json").body(json.to_string());
                }
                let response = nsh_core::prelude::fetch(request)
                    .await
                    .map_err(|_| ApiError::transport("Can’t reach the Stellarator server. Is it running?"))?;
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
                .map_err(|e| ApiError::answered(status, e.to_string()))?
                .to_bytes();
            if bytes.len() > MAX_BODY {
                return Err(ApiError::answered(status, "The response is too large."));
            }
            Ok((status, bytes.to_vec()))
        });
        task.await.map_err(|e| ApiError::transport(format!("The request did not finish: {e}")))?
    }
}

/// The JSON of a response: `T` on success, else the error ([`error_of`]).
fn decode<T: DeserializeOwned>(status: u16, bytes: &[u8]) -> Result<T, ApiError> {
    if (200..300).contains(&status) {
        return serde_json::from_slice(bytes)
            .map_err(|e| ApiError::answered(status, format!("The server's answer could not be read: {e}")));
    }
    Err(error_of(status, bytes))
}

/// The error of a failed response: the server's `detail` (FastAPI's shape, which the Rust server
/// keeps) and its `fields`; without a detail, the web UI's words for the status (lib/api.ts).
fn error_of(status: u16, bytes: &[u8]) -> ApiError {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        detail: Option<serde_json::Value>,
        #[serde(default)]
        fields: BTreeMap<String, String>,
    }
    let body = serde_json::from_slice::<Body>(bytes).ok();
    let detail = match body.as_ref().and_then(|b| b.detail.clone()) {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    };
    let message = detail.unwrap_or_else(|| match status {
        404 => "Not found.".to_owned(),
        409 => "This conflicts with the current state. Refresh and try again.".to_owned(),
        500.. => "The Stellarator server hit an error. Check its logs, then retry.".to_owned(),
        _ => format!("Request failed ({status})."),
    });
    ApiError { status: Some(status), message, fields: body.map(|b| b.fields).unwrap_or_default() }
}

/// `GET /api/endpoint-presets`: one starting point of Add endpoint (the server's `EndpointPreset`,
/// read back).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EndpointPreset {
    /// "openrouter", "lmstudio", "ollama", "vllm", "sglang", "llamacpp", "openai".
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub location: Location,
    pub default_concurrency: i64,
    pub needs_api_key: bool,
    pub api_key_url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

/// `GET /api/settings` (the server's `SettingsOut`, read back).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Settings {
    pub hf_token_set: bool,
    pub hf_token_hint: Option<String>,
    pub hf_token_from_env: bool,
    pub data_dir: String,
    pub version: String,
    /// "docker", "desktop" or "native".
    pub mode: String,
}

/// `GET /api/health`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Health {
    pub ok: bool,
    pub version: String,
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
        let run: RunOut = serde_json::from_str(json).unwrap();
        assert_eq!(run.status, types::RunStatus::Paused);
        assert_eq!((run.done, run.failed, run.total), (120, 3, 400));
    }

    #[test]
    fn errors_carry_the_servers_detail() {
        let err = decode::<Health>(404, br#"{"detail":"Run not found."}"#).unwrap_err();
        assert_eq!(err, ApiError::answered(404, "Run not found."));
        let err = decode::<Health>(422, br#"{"detail":"Check the form.","fields":{"base_url":"Must start with http."}}"#)
            .unwrap_err();
        assert_eq!(err.fields.get("base_url").map(String::as_str), Some("Must start with http."));
        assert_eq!(decode::<Health>(500, b"").unwrap_err().message, "The Stellarator server hit an error. Check its logs, then retry.");
    }

    #[test]
    fn presets_and_settings_read_back() {
        let preset: EndpointPreset = serde_json::from_str(
            r#"{"id":"openrouter","name":"OpenRouter","base_url":"https://openrouter.ai/api/v1","location":"cloud",
            "default_concurrency":16,"needs_api_key":true,"api_key_url":"https://openrouter.ai/keys","headers":{}}"#,
        )
        .unwrap();
        assert_eq!(preset.location, Location::Cloud);
        let settings: Settings = serde_json::from_str(
            r#"{"hf_token_set":false,"hf_token_hint":null,"hf_token_from_env":false,"data_dir":"/d","version":"0.3.1","mode":"desktop"}"#,
        )
        .unwrap();
        assert_eq!(settings.mode, "desktop");
    }
}
