//! API errors. Every error reaches the browser as
//! `{"detail": "One sentence saying what happened and how to fix it.", "fields": {...}}`,
//! the same shape the Python backend used, so the frontend's `ApiError` keeps working.

use std::collections::BTreeMap;

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub detail: String,
    pub fields: BTreeMap<String, String>,
}

impl ApiError {
    pub fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self { status, detail: detail.into(), fields: BTreeMap::new() }
    }
    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, detail)
    }
    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, detail)
    }
    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, detail)
    }
    /// 422 for one field: `detail` is the message, `fields[field]` the same message.
    pub fn field(field: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        let mut fields = BTreeMap::new();
        fields.insert(field.to_string(), message.clone());
        Self { status: StatusCode::UNPROCESSABLE_ENTITY, detail: message, fields }
    }
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, detail)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.status.as_u16(), self.detail)
    }
}

impl std::error::Error for ApiError {}

#[derive(Serialize)]
struct Body<'a> {
    detail: &'a str,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    fields: &'a BTreeMap<String, String>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(status = self.status.as_u16(), detail = %self.detail, "request failed");
        }
        (self.status, Json(Body { detail: &self.detail, fields: &self.fields })).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => ApiError::not_found("Not found."),
            other => {
                tracing::error!(error = %other, "database error");
                ApiError::internal("The database hit an error. Check the Stellarator logs, then retry.")
            }
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        if let Some(api) = e.downcast_ref::<ApiError>() {
            return ApiError { status: api.status, detail: api.detail.clone(), fields: api.fields.clone() };
        }
        tracing::error!(error = %e, "internal error");
        ApiError::internal("Stellarator hit an error. Check its logs, then retry.")
    }
}

/// Collects field errors during validation; empty means valid.
#[derive(Debug, Default)]
pub struct FieldErrors(pub BTreeMap<String, String>);

impl FieldErrors {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, field: &str, message: impl Into<String>) {
        self.0.entry(field.to_string()).or_insert_with(|| message.into());
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Ok when empty, else a 422 whose detail is the first message.
    pub fn finish(self) -> Result<(), ApiError> {
        if self.0.is_empty() {
            return Ok(());
        }
        let detail = self.0.values().next().cloned().unwrap_or_else(|| "Invalid request.".into());
        Err(ApiError { status: StatusCode::UNPROCESSABLE_ENTITY, detail, fields: self.0 })
    }
}

/// Types that check their own values after deserializing (lengths, ranges, URL shape).
pub trait Validate {
    fn validate(&mut self, errors: &mut FieldErrors);
}

/// `Json<T>` whose rejections and validation failures become 422 `{detail, fields}`.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = match axum::body::Bytes::from_request(req, state).await {
            Ok(b) => b,
            Err(_) => return Err(ApiError::bad_request("Couldn't read the request body.")),
        };
        let mut value: T = parse_json(&bytes)?;
        let mut errors = FieldErrors::new();
        value.validate(&mut errors);
        errors.finish()?;
        Ok(ApiJson(value))
    }
}

/// Deserialize with the failing field's path, e.g. `params.temperature`.
pub fn parse_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ApiError> {
    let de = &mut serde_json::Deserializer::from_slice(bytes);
    serde_path_to_error::deserialize(de).map_err(|e| {
        let mut path = e.path().to_string();
        let raw = e.into_inner().to_string();
        // A missing field is reported at its parent ("."), with the name only in the message.
        if let Some(name) = raw.strip_prefix("missing field `").and_then(|r| r.split('`').next()) {
            path = if path.is_empty() || path == "." { name.to_string() } else { format!("{path}.{name}") };
        }
        let message = humanize_serde(&raw);
        if path.is_empty() || path == "." {
            ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, message)
        } else {
            ApiError::field(&path, message)
        }
    })
}

fn humanize_serde(raw: &str) -> String {
    // "missing field `name` at line 1 column 2" -> "Field required."
    let raw = raw.split(" at line ").next().unwrap_or(raw);
    if raw.starts_with("missing field") {
        return "Field required.".into();
    }
    if raw.starts_with("invalid type") || raw.starts_with("invalid value") {
        return format!("{}{}.", raw[..1].to_uppercase(), &raw[1..]);
    }
    if raw.starts_with("unknown variant") {
        // "unknown variant `x`, expected one of `a`, `b`"
        return format!("Not one of the allowed values: {}.", raw.split("expected ").nth(1).unwrap_or(""));
    }
    let mut s = raw.to_string();
    if !s.ends_with('.') {
        s.push('.');
    }
    s
}

/// `Query<T>` whose rejections become 422 `{detail, fields}`.
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(v)) => Ok(ApiQuery(v)),
            Err(e) => Err(query_error(e)),
        }
    }
}

fn query_error(e: QueryRejection) -> ApiError {
    let text = e.body_text();
    let text = text.trim_start_matches("Failed to deserialize query string: ");
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, humanize_serde(text))
}

#[allow(dead_code)]
fn json_error(e: JsonRejection) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, humanize_serde(&e.body_text()))
}

/// No-op validation for request types without rules.
#[macro_export]
macro_rules! no_validation {
    ($($t:ty),* $(,)?) => {
        $(impl $crate::error::Validate for $t {
            fn validate(&mut self, _errors: &mut $crate::error::FieldErrors) {}
        })*
    };
}
