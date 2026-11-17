//! Helpers for talking to OpenAI-compatible servers: base URL handling, headers, error sentences,
//! the model list and the API key check.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

/// Normalize a base URL to the API root that `/chat/completions` and `/models` hang off.
///
/// Accepts "http://host:8000", "http://host:8000/v1", "https://api.example.com/v1/" or a URL that
/// already ends in "/chat/completions". A URL without any path gets "/v1" appended, which is what
/// vLLM, SGLang, llama.cpp, Ollama, LM Studio and most hosted APIs expect.
pub fn api_root(base_url: &str) -> String {
    let mut url = base_url.trim().trim_end_matches('/').to_string();
    for suffix in ["/chat/completions", "/completions", "/models"] {
        if let Some(stripped) = url.strip_suffix(suffix) {
            url = stripped.to_string();
        }
    }
    let path_empty = match url::Url::parse(&url) {
        Ok(u) => u.path().is_empty() || u.path() == "/",
        Err(_) => false,
    };
    if path_empty {
        url.push_str("/v1");
    }
    url
}

pub fn build_headers(api_key: Option<&str>, extra: &BTreeMap<String, String>) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        if let Ok(v) = HeaderValue::from_str(&format!("Bearer {key}")) {
            h.insert(AUTHORIZATION, v);
        }
    }
    for (k, v) in extra {
        if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
            h.insert(name, value);
        }
    }
    h
}

/// One sentence a person can act on, for a failed request.
pub fn describe_reqwest_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        if e.is_connect() {
            return "Connection timed out. Check the URL and network.".into();
        }
        return "The server took too long to answer.".into();
    }
    if e.is_connect() {
        return "Could not connect. Check the URL and that the server is up.".into();
    }
    if e.is_decode() || e.is_body() {
        return "The server sent a reply that couldn't be read.".into();
    }
    let text = e.to_string();
    if text.is_empty() {
        "The request failed.".into()
    } else {
        text
    }
}

/// One sentence for an HTTP error status, given the response body text.
pub fn describe_status(status: u16, body: &str) -> String {
    let body: String = body.trim().chars().take(300).collect();
    match status {
        401 => "The server rejected the API key (401).".into(),
        403 => "The API key is not allowed to do this (403).".into(),
        404 => "Nothing found at this URL (404). Does the base URL end in /v1?".into(),
        _ if body.is_empty() => format!("The server answered {status}."),
        _ => format!("The server answered {status}: {body}"),
    }
}

/// The server's own error message from a JSON error body: `{"error": {"message": ...}}`,
/// `{"error": "..."}` or `{"message": ...}`. At most 160 characters.
pub fn server_message(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let msg = match v.get("error") {
        Some(Value::Object(o)) => o.get("message").and_then(Value::as_str),
        Some(Value::String(s)) => Some(s.as_str()),
        _ => None,
    }
    .or_else(|| v.get("message").and_then(Value::as_str))?;
    let msg = msg.trim();
    if msg.is_empty() {
        None
    } else {
        Some(msg.chars().take(160).collect())
    }
}

#[derive(Debug)]
pub enum CheckError {
    Request(reqwest::Error),
    Status { status: u16, body: String },
    Shape(String),
}

impl CheckError {
    pub fn sentence(&self) -> String {
        match self {
            CheckError::Request(e) => describe_reqwest_error(e),
            CheckError::Status { status, body } => describe_status(*status, body),
            CheckError::Shape(s) => s.clone(),
        }
    }
}

/// GET {root}/models. Returns (model ids sorted case-insensitively, latency in ms).
pub async fn list_models(
    http: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    headers: &BTreeMap<String, String>,
) -> Result<(Vec<String>, i64), CheckError> {
    let start = Instant::now();
    let resp = http
        .get(format!("{}/models", api_root(base_url)))
        .headers(build_headers(api_key, headers))
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(CheckError::Request)?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(CheckError::Request)?;
    if !(200..300).contains(&status) {
        return Err(CheckError::Status { status, body: text });
    }
    let latency = start.elapsed().as_millis() as i64;
    let data: Value = serde_json::from_str(&text)
        .map_err(|_| CheckError::Shape("The server's model list isn't JSON. Is this an OpenAI-compatible API root?".into()))?;
    let items = match &data {
        Value::Object(o) => o.get("data").cloned().unwrap_or(Value::Null),
        other => other.clone(),
    };
    let mut ids: Vec<String> = Vec::new();
    if let Value::Array(items) = items {
        for item in items {
            match item {
                Value::Object(o) => {
                    if let Some(id) = o.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                        ids.push(id.to_string());
                    }
                }
                Value::String(s) => ids.push(s),
                _ => {}
            }
        }
    }
    // Ollama's native shape: {"models": [{"name": ...}]}
    if ids.is_empty() {
        if let Some(Value::Array(models)) = data.get("models") {
            for m in models {
                if let Some(n) = m.get("name").or_else(|| m.get("model")).and_then(Value::as_str) {
                    ids.push(n.to_string());
                }
            }
        }
    }
    ids.sort_by_key(|s| s.to_lowercase());
    ids.dedup();
    Ok((ids, latency))
}

/// A sentence when the server rejects the API key, else None.
///
/// Some servers (OpenRouter, for one) list models without checking the key, so a working
/// GET /models proves nothing about it. This sends an empty chat request instead: servers check
/// the key before they read the body, so 401 or 403 means the key was rejected, and any other
/// answer (400, 404, 422) means it was accepted. Nothing is generated; max_tokens is 1 in case a
/// lenient server tries anyway. Network errors return None: the models check reports those.
pub async fn check_key(
    http: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    headers: &BTreeMap<String, String>,
) -> Option<String> {
    let resp = http
        .post(format!("{}/chat/completions", api_root(base_url)))
        .headers(build_headers(api_key, headers))
        .json(&json!({"model": "", "messages": [], "max_tokens": 1}))
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .ok()?;
    let status = resp.status().as_u16();
    if status != 401 && status != 403 {
        return None;
    }
    let body = resp.text().await.unwrap_or_default();
    let sentence = describe_status(status, &body);
    Some(match server_message(&body) {
        Some(detail) if sentence.ends_with('.') => format!("{}: {detail}", &sentence[..sentence.len() - 1]),
        _ => sentence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_root_normalizes() {
        assert_eq!(api_root("http://h:8000"), "http://h:8000/v1");
        assert_eq!(api_root("http://h:8000/"), "http://h:8000/v1");
        assert_eq!(api_root("http://h:8000/v1/"), "http://h:8000/v1");
        assert_eq!(api_root("https://openrouter.ai/api/v1"), "https://openrouter.ai/api/v1");
        assert_eq!(api_root("http://h/v1/chat/completions"), "http://h/v1");
        assert_eq!(api_root("http://h/v1/models"), "http://h/v1");
    }

    #[test]
    fn server_message_shapes() {
        assert_eq!(server_message(r#"{"error":{"message":"User not found."}}"#).as_deref(), Some("User not found."));
        assert_eq!(server_message(r#"{"error":"nope"}"#).as_deref(), Some("nope"));
        assert_eq!(server_message("not json"), None);
    }
}
