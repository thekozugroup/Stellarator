//! The web UI. Release builds embed `frontend/dist` (build it first: `npm run build` in
//! `frontend/`); debug builds read the same folder from disk. Unknown paths that are not under
//! `/api` get `index.html` so deep links work.

use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

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
