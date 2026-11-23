//! The web UI. Release builds embed it; debug builds read the same folder from disk. Which UI is
//! chosen when the server compiles (build.rs):
//!
//! - the nsh app's web build (`app/.nsh/web/Stellarator`, made by `nsh package --target web --out
//!   .nsh/web` in app/), the default;
//! - the React UI (`frontend/dist`), with `STELLARATOR_WEB_UI=react`: the one the Tauri desktop app
//!   loads, kept until the switch to nsh is done.
//!
//! Unknown paths that are not under `/api` get `index.html` so deep links work. Files the build
//! ships precompressed (`x.br`, `x.gz` beside `x`, as nsh's web build does) are sent as they are to
//! browsers that accept them. Files named by their content (nsh's `name.0123456789.ext`, Vite's
//! `assets/`) never change and are cached for a year; everything else is revalidated.

use std::sync::OnceLock;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use rust_embed::RustEmbed;
use sha2::{Digest, Sha256};

#[cfg(not(web_ui_react))]
#[derive(RustEmbed)]
#[folder = "../../app/.nsh/web/Stellarator"]
#[allow_missing = true]
struct Dist;

#[cfg(web_ui_react)]
#[derive(RustEmbed)]
#[folder = "../../frontend/dist"]
#[allow_missing = true]
struct Dist;

/// Which UI this server serves, for the messages below.
const UI: &str = if cfg!(web_ui_react) { "react" } else { "nsh" };

pub async fn serve(uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") || path == "api" {
        return (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"detail": "Not found."}))).into_response();
    }
    if !path.is_empty() {
        if let Some(response) = file(path, &headers) {
            return response;
        }
    }
    match file("index.html", &headers) {
        Some(response) => response,
        None if UI == "react" => (
            StatusCode::NOT_FOUND,
            "The web UI is not built. Run `npm run build` in frontend/, then rebuild the server.",
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "The web UI is not built. Run `nsh package --target web --out .nsh/web` in app/, then rebuild the server.",
        )
            .into_response(),
    }
}

/// `path` from the build, in the best encoding `headers` accept; `None` when the build has no such
/// file.
fn file(path: &str, headers: &HeaderMap) -> Option<Response> {
    let original = Dist::get(path)?;
    let accepted = headers.get(header::ACCEPT_ENCODING).and_then(|v| v.to_str().ok()).unwrap_or("");
    let (data, encoding) = [("br", ".br"), ("gzip", ".gz")]
        .into_iter()
        .filter(|(name, _)| accepts(accepted, name))
        .find_map(|(name, ext)| Dist::get(&format!("{path}{ext}")).map(|f| (f.data, Some(name))))
        .unwrap_or((original.data, None));
    let mime = if path.ends_with(".webmanifest") {
        "application/manifest+json".to_string()
    } else if path.ends_with(".html") {
        "text/html; charset=utf-8".to_string()
    } else {
        mime_guess::from_path(path).first_or_octet_stream().to_string()
    };
    let cache = if immutable(path) { "public, max-age=31536000, immutable" } else { "no-cache" };
    let mut response = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, cache)
        .header(header::VARY, "Accept-Encoding");
    if let Some(encoding) = encoding {
        response = response.header(header::CONTENT_ENCODING, encoding);
    }
    Some(response.body(Body::from(data.into_owned())).unwrap())
}

/// Whether an `Accept-Encoding` value takes `name` (and does not refuse it with `q=0`).
fn accepts(header: &str, name: &str) -> bool {
    header.split(',').any(|part| {
        let mut pieces = part.trim().split(';');
        let coding = pieces.next().unwrap_or("").trim();
        let refused = pieces.any(|p| matches!(p.trim().replace(' ', "").as_str(), "q=0" | "q=0.0" | "q=0.00" | "q=0.000"));
        coding.eq_ignore_ascii_case(name) && !refused
    })
}

/// Whether `path` is named by its content: a name with a part of ten hex digits (nsh:
/// `nsh-page.6c2c70f7ae.css`, `browser.b998e17dc5/...`), or for the React UI Vite's `assets/`.
/// nsh's `assets/` holds the app's files under their own names, so it is revalidated.
fn immutable(path: &str) -> bool {
    (UI == "react" && path.starts_with("assets/"))
        || path.split('/').flat_map(|segment| segment.split('.')).any(|part| part.len() == 10 && part.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Content-Security-Policy for every response: only this server's scripts (plus the inline scripts
/// in index.html, allowed by their hashes), no remote images or frames, requests only to this
/// origin. Model output is rendered as Markdown, so this is the backstop if anything slips past.
/// The nsh UI also compiles its WebAssembly (`'wasm-unsafe-eval'`, which allows no JavaScript
/// eval) and draws some images from blobs.
fn csp() -> &'static HeaderValue {
    static CSP: OnceLock<HeaderValue> = OnceLock::new();
    CSP.get_or_init(|| {
        let mut hashes = String::new();
        if let Some(index) = Dist::get("index.html") {
            let html = String::from_utf8_lossy(&index.data).into_owned();
            for body in inline_scripts(&html) {
                let digest = Sha256::digest(body.as_bytes());
                hashes.push_str(&format!(" 'sha256-{}'", base64::engine::general_purpose::STANDARD.encode(digest)));
            }
        }
        let (wasm, images) = if UI == "nsh" { (" 'wasm-unsafe-eval'", " blob:") } else { ("", "") };
        let policy = format!(
            "default-src 'self'; script-src 'self'{wasm}{hashes}; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:{images}; font-src 'self' data:; connect-src 'self'; worker-src 'self'; \
             object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"
        );
        HeaderValue::from_str(&policy).expect("valid header")
    })
}

/// The bodies of the inline `<script>` elements of `html` (with or without attributes; not those
/// with a `src`).
fn inline_scripts(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        let after = &rest[start + "<script".len()..];
        let Some(tag_end) = after.find('>') else { break };
        let attributes = &after[..tag_end];
        let body_start = &after[tag_end + 1..];
        let Some(end) = body_start.find("</script>") else { break };
        if !attributes.contains("src=") && !body_start[..end].trim().is_empty() {
            out.push(&body_start[..end]);
        }
        rest = &body_start[end..];
    }
    out
}

pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(header::CONTENT_SECURITY_POLICY, csp().clone());
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodings_and_cache_names() {
        assert!(accepts("gzip, deflate, br", "br"));
        assert!(accepts("br;q=0.8, gzip", "gzip"));
        assert!(!accepts("gzip;q=0, br", "gzip"));
        assert!(!accepts("deflate", "br"));
        assert!(immutable("nsh-page.6c2c70f7ae.css"));
        assert!(immutable("browser.b998e17dc5/runtime.wasm"));
        assert_eq!(immutable("assets/index-abc.js"), UI == "react");
        assert!(!immutable("index.html"));
        assert!(!immutable("assets.json"));
    }

    #[test]
    fn inline_scripts_are_found_with_attributes() {
        let html = r#"<script>a()</script><script type="module">b()</script><script src="x.js"></script>"#;
        assert_eq!(inline_scripts(html), vec!["a()", "b()"]);
    }
}
