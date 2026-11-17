//! The web UI. Release builds embed `frontend/dist` (build it first: `npm run build` in
//! `frontend/`); debug builds read the same folder from disk. Unknown paths that are not under
//! `/api` get `index.html` so deep links work.

use std::sync::OnceLock;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use rust_embed::RustEmbed;
use sha2::{Digest, Sha256};

#[derive(RustEmbed)]
#[folder = "../../frontend/dist"]
struct Dist;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") || path == "api" {
        return (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"detail": "Not found."}))).into_response();
    }
    if !path.is_empty() {
        if let Some(file) = Dist::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let mime = if path.ends_with(".webmanifest") { "application/manifest+json".to_string() } else { mime.to_string() };
            // Hashed build assets never change; everything else is revalidated.
            let cache = if path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
            return Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .header(header::CACHE_CONTROL, cache)
                .body(Body::from(file.data.into_owned()))
                .unwrap();
        }
    }
    match Dist::get("index.html") {
        Some(file) => Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(file.data.into_owned()))
            .unwrap(),
        None => (
            StatusCode::NOT_FOUND,
            "The web UI is not built. Run `npm run build` in frontend/, then rebuild the server.",
        )
            .into_response(),
    }
}

/// Content-Security-Policy for every response: only this server's scripts (plus the inline theme
/// script in index.html, allowed by its hash), no remote images or frames, requests only to this
/// origin. Model output is rendered as Markdown, so this is the backstop if anything slips past.
fn csp() -> &'static HeaderValue {
    static CSP: OnceLock<HeaderValue> = OnceLock::new();
    CSP.get_or_init(|| {
        let mut hashes = String::new();
        if let Some(index) = Dist::get("index.html") {
            let html = String::from_utf8_lossy(&index.data).into_owned();
            let mut rest = html.as_str();
            while let Some(start) = rest.find("<script>") {
                let after = &rest[start + "<script>".len()..];
                let Some(end) = after.find("</script>") else { break };
                let digest = Sha256::digest(after[..end].as_bytes());
                hashes.push_str(&format!(" 'sha256-{}'", base64::engine::general_purpose::STANDARD.encode(digest)));
                rest = &after[end..];
            }
        }
        let policy = format!(
            "default-src 'self'; script-src 'self'{hashes}; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; font-src 'self' data:; connect-src 'self'; object-src 'none'; \
             base-uri 'none'; frame-ancestors 'none'; form-action 'self'"
        );
        HeaderValue::from_str(&policy).expect("valid header")
    })
}

pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(header::CONTENT_SECURITY_POLICY, csp().clone());
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    res
}
