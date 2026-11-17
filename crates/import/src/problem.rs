//! `Problem`: something about an import the user can fix, with the HTTP status it answers with.

use axum::http::StatusCode;
use stellarator_core::ApiError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub status: u16,
    pub message: String,
    /// The request field the problem is about (e.g. "source.dataset"), for the form to highlight.
    pub field: Option<&'static str>,
}

impl Problem {
    /// 422: the request can't be imported as it is.
    pub fn new(message: impl Into<String>) -> Self {
        Self { status: 422, message: message.into(), field: None }
    }
    pub fn status(status: u16, message: impl Into<String>) -> Self {
        Self { status, message: message.into(), field: None }
    }
    pub fn on(mut self, field: &'static str) -> Self {
        self.field = Some(field);
        self
    }
    /// Internal marker for a stopped import; never shown.
    pub(crate) fn cancelled() -> Self {
        Self::status(499, "The import was stopped.")
    }
    pub(crate) fn is_cancelled(&self) -> bool {
        self.status == 499
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Problem {}

impl From<Problem> for ApiError {
    fn from(p: Problem) -> Self {
        let status = StatusCode::from_u16(p.status).unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
        let mut e = ApiError::new(status, p.message.clone());
        if let Some(f) = p.field {
            e.fields.insert(f.to_string(), p.message);
        }
        e
    }
}

pub type Result<T, E = Problem> = std::result::Result<T, E>;

/// Python-style thousands separators: 1234567 -> "1,234,567".
pub(crate) fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn thousands() {
        assert_eq!(super::thousands(0), "0");
        assert_eq!(super::thousands(999), "999");
        assert_eq!(super::thousands(1000), "1,000");
        assert_eq!(super::thousands(1234567), "1,234,567");
    }
}
