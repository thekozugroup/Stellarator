//! Desktop only: a per-launch token, so other programs on the machine (and web pages, through
//! DNS rebinding or cross-site requests) cannot drive the local API. The app opens
//! `http://127.0.0.1:<port>/?launch=<token>`; the server answers with an HttpOnly, SameSite=Strict
//! cookie and a redirect to the clean URL, and from then on requires that cookie.

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;

const COOKIE: &str = "stellarator_session";

pub fn wrap(app: Router, token: Option<String>) -> Router {
    match token {
        None => app,
        Some(token) => app.layer(middleware::from_fn(move |req: Request, next: Next| {
            let token = token.clone();
            async move { check(req, next, &token).await }
        })),
    }
}

async fn check(req: Request, next: Next, token: &str) -> Response {
    // First load from the app: trade the query token for the cookie.
    if let Some(q) = req.uri().query() {
        if let Some(value) = q.split('&').find_map(|kv| kv.strip_prefix("launch=")) {
            if constant_time_eq(value.as_bytes(), token.as_bytes()) {
                let cookie = format!("{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/");
                let location = req.uri().path().to_string();
                return Response::builder()
                    .status(StatusCode::SEE_OTHER)
                    .header(header::LOCATION, location)
                    .header(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap())
                    .body(Body::empty())
                    .unwrap();
            }
        }
    }
    let ok = req
        .headers()
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().strip_prefix(&format!("{COOKIE}=")).map(str::to_string))
        .any(|v| constant_time_eq(v.as_bytes(), token.as_bytes()));
    if ok {
        return next.run(req).await;
    }
    (StatusCode::FORBIDDEN, "Open Stellarator from its app.").into_response()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
