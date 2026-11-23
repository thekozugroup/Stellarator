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
use types::{ExportPreviewOut, HubPushIn, HubPushOut, Page, RunLive, SampleOut, SampleSummary};
use types::{ImportCommitIn, ImportMapping, ImportPreviewIn, ImportPreviewOut, ImportSource, PromptOut, PromptSetOut, UploadOut};

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

    // ----------------------------------------------------------------------------- a run

    /// `GET /api/runs/{id}/live`: the run, its requests in flight, its rates and its throughput
    /// readings.
    pub async fn run_live(&self, id: i64) -> Result<RunLive, ApiError> {
        self.get(&format!("/api/runs/{id}/live")).await
    }

    /// `PATCH /api/runs/{id}` with `body` (`{"name": …}` or `{"concurrency": …}`).
    pub async fn update_run(&self, id: i64, body: &serde_json::Value) -> Result<RunOut, ApiError> {
        self.patch(&format!("/api/runs/{id}"), body).await
    }

    /// `POST /api/runs/{id}/{action}`: `action` is "pause", "resume" or "retry-failed".
    pub async fn run_action(&self, id: i64, action: &str) -> Result<RunOut, ApiError> {
        self.post(&format!("/api/runs/{id}/{action}"), &serde_json::json!({})).await
    }

    /// `DELETE /api/runs/{id}`: the run and its samples.
    pub async fn delete_run(&self, id: i64) -> Result<(), ApiError> {
        self.delete(&format!("/api/runs/{id}")).await
    }

    /// `GET /api/runs/{id}/samples` with `query` (`status=failed&limit=1`, no `?`): a page of the
    /// run's samples.
    pub async fn run_samples(&self, id: i64, query: &str) -> Result<Page<SampleSummary>, ApiError> {
        let sep = if query.is_empty() { "" } else { "?" };
        self.get(&format!("/api/runs/{id}/samples{sep}{query}")).await
    }

    /// `GET /api/samples/{id}`: one sample in full.
    pub async fn sample(&self, id: i64) -> Result<SampleOut, ApiError> {
        self.get(&format!("/api/samples/{id}")).await
    }

    /// `GET /api/endpoints/{id}`: one endpoint.
    pub async fn endpoint(&self, id: i64) -> Result<EndpointOut, ApiError> {
        self.get(&format!("/api/endpoints/{id}")).await
    }

    // ----------------------------------------------------------------------------- exports

    /// `POST {base}/preview` (`base` is `/api/runs/4/export` or `/api/preference-sets/2/export`):
    /// how many records an export with `options` holds, what it skips, and its first record.
    pub async fn export_preview(&self, base: &str, options: &serde_json::Value) -> Result<ExportPreviewOut, ApiError> {
        self.post(&format!("{base}/preview"), options).await
    }

    /// `POST {base}/hub`: pushes the export with `options` to a Hugging Face dataset repository.
    pub async fn export_hub(&self, base: &str, options: &serde_json::Value, target: &HubPushIn) -> Result<HubPushOut, ApiError> {
        self.post(&format!("{base}/hub"), &serde_json::json!({ "options": options, "target": target })).await
    }

    /// Saves what `GET path` answers (an export file) in the Downloads folder, under the file name
    /// the server sends (`fallback` without one), never over an existing file, as the Tauri app
    /// saved its downloads. The path it was saved at.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn download(&self, path: &str, fallback: &str) -> Result<std::path::PathBuf, ApiError> {
        match &self.transport {
            Transport::InProcess { router, runtime } => {
                in_process::download(router.clone(), runtime, path, fallback.to_owned()).await
            }
        }
    }

    /// `GET /api/prompt-sets`: every prompt set, newest first.
    pub async fn prompt_sets(&self) -> Result<Vec<PromptSetOut>, ApiError> {
        self.get("/api/prompt-sets").await
    }

    /// `GET /api/prompt-sets/{id}`.
    pub async fn prompt_set(&self, id: i64) -> Result<PromptSetOut, ApiError> {
        self.get(&format!("/api/prompt-sets/{id}")).await
    }

    /// `PATCH /api/prompt-sets/{id}`: renames it.
    pub async fn rename_prompt_set(&self, id: i64, name: &str) -> Result<PromptSetOut, ApiError> {
        self.patch(&format!("/api/prompt-sets/{id}"), &serde_json::json!({ "name": name })).await
    }

    /// `DELETE /api/prompt-sets/{id}`: the set, its prompts and its runs (also stops an import).
    pub async fn delete_prompt_set(&self, id: i64) -> Result<(), ApiError> {
        self.delete(&format!("/api/prompt-sets/{id}")).await
    }

    /// `POST /api/prompt-sets/{id}/stop`: stops the set's import and deletes the partial set; 409
    /// (the set kept) when the import has finished, even while the request ran.
    pub async fn stop_import(&self, id: i64) -> Result<(), ApiError> {
        let (status, body) = self.send(Method::Post, &format!("/api/prompt-sets/{id}/stop"), Some(serde_json::json!({}))).await?;
        if (200..300).contains(&status) { Ok(()) } else { Err(error_of(status, &body)) }
    }

    /// `GET /api/prompt-sets/{id}/prompts`: one page of a set's prompts, filtered by `q` when it is
    /// not empty.
    pub async fn prompts(&self, id: i64, offset: i64, limit: i64, q: &str) -> Result<Page<PromptOut>, ApiError> {
        let mut path = format!("/api/prompt-sets/{id}/prompts?offset={offset}&limit={limit}");
        if !q.is_empty() {
            path.push_str("&q=");
            path.push_str(&query_escape(q));
        }
        self.get(&path).await
    }

    /// `POST /api/imports/upload`: a file for an import (multipart, the field `file`).
    pub async fn import_upload(&self, filename: &str, bytes: Vec<u8>) -> Result<UploadOut, ApiError> {
        let boundary = "----stellarator-upload-3f9c2b7e";
        let (content_type, body) = multipart_file(boundary, filename, &bytes);
        let (status, answer) = self.send_body(Method::Post, "/api/imports/upload", Some((content_type, body))).await?;
        decode(status, &answer)
    }

    /// `POST /api/imports/preview`: the first prompts of a source as the mapping would import them.
    pub async fn import_preview(&self, source: &ImportSource, mapping: &ImportMapping) -> Result<ImportPreviewOut, ApiError> {
        let body = ImportPreviewIn { source: source.clone(), mapping: mapping.clone() };
        self.post("/api/imports/preview", &body).await
    }

    /// `POST /api/imports`: starts the import in the background; the answer is the set, importing.
    pub async fn import_commit(&self, source: &ImportSource, mapping: &ImportMapping, name: &str) -> Result<PromptSetOut, ApiError> {
        let body = ImportCommitIn { source: source.clone(), mapping: mapping.clone(), name: name.to_owned() };
        self.post("/api/imports", &body).await
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
        let body = body.map(|json| ("application/json".to_owned(), json.to_string().into_bytes()));
        self.send_body(method, path, body).await
    }

    /// One request with a body of any type: `(content type, bytes)`.
    async fn send_body(
        &self,
        method: Method,
        path: &str,
        body: Option<(String, Vec<u8>)>,
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
                if let Some((content_type, bytes)) = body {
                    request = request.header("content-type", content_type).body(bytes);
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
        body: Option<(String, Vec<u8>)>,
    ) -> Result<(u16, Vec<u8>), ApiError> {
        let mut request = Request::builder()
            .method(method.as_str())
            .uri(path)
            .header(header::ACCEPT, "application/json");
        let body = match body {
            Some((content_type, bytes)) => {
                request = request.header(header::CONTENT_TYPE, content_type);
                Body::from(bytes)
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

    /// `GET path`, its body written to a free file name in the Downloads folder as it streams (an
    /// export can be larger than [`MAX_BODY`]). A file left half written is removed.
    pub(super) async fn download(
        router: axum::Router,
        runtime: &tokio::runtime::Handle,
        path: &str,
        fallback: String,
    ) -> Result<std::path::PathBuf, ApiError> {
        use std::io::Write;

        let request = Request::builder()
            .method("GET")
            .uri(path)
            .body(Body::empty())
            .map_err(|e| ApiError::transport(e.to_string()))?;
        let task = runtime.spawn(async move {
            let response = router.oneshot(request).await.unwrap_or_else(|never| match never {});
            let status = response.status().as_u16();
            if !(200..300).contains(&status) {
                let bytes = response.into_body().collect().await.map(|b| b.to_bytes().to_vec()).unwrap_or_default();
                return Err(super::error_of(status, &bytes));
            }
            let name = response
                .headers()
                .get(header::CONTENT_DISPOSITION)
                .and_then(|v| v.to_str().ok())
                .and_then(super::disposition_name)
                .unwrap_or(fallback);
            let dir = dirs::download_dir()
                .or_else(dirs::home_dir)
                .ok_or_else(|| ApiError::transport("There is no Downloads folder to save the file in."))?;
            let failed = |e: std::io::Error| ApiError::transport(format!("Couldn’t save the file. {e}"));
            std::fs::create_dir_all(&dir).map_err(failed)?;
            let dest = super::free_path(&dir, &name);
            let mut file = std::fs::File::create(&dest).map_err(failed)?;
            let mut body = response.into_body();
            let written: Result<(), ApiError> = async {
                while let Some(frame) = body.frame().await {
                    let frame = frame.map_err(|e| ApiError::answered(status, e.to_string()))?;
                    if let Some(data) = frame.data_ref() {
                        file.write_all(data).map_err(failed)?;
                    }
                }
                file.flush().map_err(failed)
            }
            .await;
            if let Err(e) = written {
                drop(file);
                let _ = std::fs::remove_file(&dest);
                return Err(e);
            }
            Ok(dest)
        });
        task.await.map_err(|e| ApiError::transport(format!("The request did not finish: {e}")))?
    }
}

/// The file name of a `Content-Disposition` header (`attachment; filename="run-4-sft.jsonl"`).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn disposition_name(header: &str) -> Option<String> {
    let at = header.find("filename=")?;
    let rest = header[at + "filename=".len()..].trim();
    let name = match rest.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next().unwrap_or_default(),
        None => rest.split(';').next().unwrap_or_default().trim(),
    };
    // Only a name: no folders, nothing hidden.
    let name = name.rsplit(['/', '\\']).next().unwrap_or_default().trim_start_matches('.');
    (!name.is_empty()).then(|| name.to_owned())
}

/// `name` with " (n)" before its last extension: "run.jsonl" and 2 give "run (2).jsonl".
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn numbered(name: &str, n: u32) -> String {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    };
    format!("{stem} ({n}){ext}")
}

/// `dir/name` when it is free, else `dir/<stem> (n).<ext>` with the first free n (as the Tauri
/// app's downloads).
#[cfg(not(target_arch = "wasm32"))]
fn free_path(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    if !path.exists() {
        return path;
    }
    (1u32..).map(|n| dir.join(numbered(name, n))).find(|p| !p.exists()).unwrap_or(path)
}

/// A `multipart/form-data` body of one file in the field `file`: its content type and bytes.
fn multipart_file(boundary: &str, filename: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    let name: String = filename.chars().filter(|c| !matches!(c, '"' | '\r' | '\n' | '\\')).collect();
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// `s` percent-encoded for a query value (everything but unreserved characters).
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
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

// ------------------------------------------------------------------------------- preferences

/// Preference sets (frontend/src/components/preferences/queries.ts and the dialogs there).
impl Api {
    /// `GET /api/preference-sets`: every preference set, newest first.
    pub async fn preference_sets(&self) -> Result<Vec<types::PreferenceSetOut>, ApiError> {
        self.get("/api/preference-sets").await
    }

    /// `GET /api/preference-sets/{id}`.
    pub async fn preference_set(&self, id: i64) -> Result<types::PreferenceSetOut, ApiError> {
        self.get(&format!("/api/preference-sets/{id}")).await
    }

    /// `POST /api/preference-sets`: a new set over `run_ids` (one prompt set).
    pub async fn create_preference_set(&self, body: &types::PreferenceSetCreate) -> Result<types::PreferenceSetOut, ApiError> {
        self.post("/api/preference-sets", body).await
    }

    /// `DELETE /api/preference-sets/{id}`: the set and its pairs; runs and responses are kept.
    pub async fn delete_preference_set(&self, id: i64) -> Result<(), ApiError> {
        self.delete(&format!("/api/preference-sets/{id}")).await
    }

    /// `GET /api/preference-sets/{id}/items?filter=…&offset=…&limit=…`: prompts with their
    /// candidates and pair; `filter` is "undecided", "decided" or "all".
    pub async fn preference_items(
        &self,
        id: i64,
        filter: &str,
        offset: i64,
        limit: i64,
    ) -> Result<types::Page<types::PreferenceItem>, ApiError> {
        self.get(&format!("/api/preference-sets/{id}/items?filter={filter}&offset={offset}&limit={limit}")).await
    }

    /// `PUT /api/preference-sets/{id}/pairs/{prompt_id}`: saves a prompt's pair.
    pub async fn save_pair(&self, id: i64, prompt_id: i64, body: &types::PairIn) -> Result<types::PreferenceItem, ApiError> {
        self.put(&format!("/api/preference-sets/{id}/pairs/{prompt_id}"), body).await
    }

    /// `DELETE /api/preference-sets/{id}/pairs/{prompt_id}`: clears a prompt's pair.
    pub async fn clear_pair(&self, id: i64, prompt_id: i64) -> Result<(), ApiError> {
        self.delete(&format!("/api/preference-sets/{id}/pairs/{prompt_id}")).await
    }

    /// `POST /api/preference-sets/{id}/auto-pair`: prefers one run's responses over another's.
    pub async fn auto_pair(&self, id: i64, body: &types::AutoPairIn) -> Result<types::AutoPairOut, ApiError> {
        self.post(&format!("/api/preference-sets/{id}/auto-pair"), body).await
    }

    /// `POST /api/preference-sets/{id}/export/preview`: the record count, what is skipped and the
    /// first record of an export with `options`.
    pub async fn preference_export_preview(
        &self,
        id: i64,
        options: &types::PrefExportOptions,
    ) -> Result<types::ExportPreviewOut, ApiError> {
        self.post(&format!("/api/preference-sets/{id}/export/preview"), options).await
    }

    /// `POST /api/preference-sets/{id}/export/hub`: pushes the pairs to a Hugging Face dataset.
    pub async fn preference_export_hub(&self, id: i64, body: &types::PrefHubIn) -> Result<types::HubPushOut, ApiError> {
        self.post(&format!("/api/preference-sets/{id}/export/hub"), body).await
    }
}

// ------------------------------------------------------------------------------- MCP servers

/// MCP servers: the tools a run can offer the model (crates/server/src/routes/mcp_servers.rs).
impl Api {
    /// `GET /api/mcp-servers`: every saved server, with its last check and its tools.
    pub async fn mcp_servers(&self) -> Result<Vec<types::McpServerOut>, ApiError> {
        self.get("/api/mcp-servers").await
    }

    /// `POST /api/mcp-servers`: a new server (not checked yet).
    pub async fn create_mcp_server(&self, body: &types::McpServerIn) -> Result<types::McpServerOut, ApiError> {
        self.post("/api/mcp-servers", body).await
    }

    /// `POST /api/mcp-servers/import`: the servers of a pasted config, each checked when `check`.
    pub async fn import_mcp_servers(&self, body: &types::McpImportIn) -> Result<types::McpImportOut, ApiError> {
        self.post("/api/mcp-servers/import", body).await
    }

    /// `PATCH /api/mcp-servers/{id}` (absent fields stay; `env` and `headers` are the whole set).
    pub async fn update_mcp_server(&self, id: i64, body: &types::McpServerPatch) -> Result<types::McpServerOut, ApiError> {
        self.patch(&format!("/api/mcp-servers/{id}"), body).await
    }

    /// `DELETE /api/mcp-servers/{id}`: 409 while a running run uses it.
    pub async fn delete_mcp_server(&self, id: i64) -> Result<(), ApiError> {
        self.delete(&format!("/api/mcp-servers/{id}")).await
    }

    /// `POST /api/mcp-servers/{id}/check`: connects, lists the tools and keeps them.
    pub async fn check_mcp_server(&self, id: i64) -> Result<types::McpServerOut, ApiError> {
        self.post(&format!("/api/mcp-servers/{id}/check"), &serde_json::json!({})).await
    }
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
    /// Stdio MCP servers can be added and run (always on the desktop; in server mode only with
    /// STELLARATOR_ALLOW_STDIO_MCP=1).
    #[serde(default)]
    pub stdio_mcp_allowed: bool,
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
    fn downloads_keep_the_servers_file_name() {
        assert_eq!(disposition_name("attachment; filename=\"run-4-sft.jsonl\"").as_deref(), Some("run-4-sft.jsonl"));
        assert_eq!(disposition_name("attachment; filename=pairs.csv; size=3").as_deref(), Some("pairs.csv"));
        assert_eq!(disposition_name("attachment; filename=\"../../.bashrc\"").as_deref(), Some("bashrc"));
        assert_eq!(disposition_name("attachment"), None);
        // The counter goes before the last extension, as the Tauri app named its downloads.
        assert_eq!(numbered("qwen3.5-run-sft.jsonl", 1), "qwen3.5-run-sft (1).jsonl");
        assert_eq!(numbered("export", 2), "export (2)");
        assert_eq!(numbered(".hidden", 1), ".hidden (1)");
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

    #[test]
    fn a_query_value_is_escaped() {
        assert_eq!(query_escape("a b&c=é"), "a%20b%26c%3D%C3%A9");
        assert_eq!(query_escape("x-y_z.~"), "x-y_z.~");
    }

    #[test]
    fn an_upload_is_one_multipart_field() {
        let (content_type, body) = multipart_file("B", "my \"file\".md", b"hi");
        assert_eq!(content_type, "multipart/form-data; boundary=B");
        let text = String::from_utf8(body).unwrap();
        assert!(text.starts_with("--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"my file.md\"\r\n"));
        assert!(text.ends_with("\r\n\r\nhi\r\n--B--\r\n"));
    }
}
