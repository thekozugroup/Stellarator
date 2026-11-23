//! Loading data from the API into a screen: what a list or a detail has ([`Load`]) and the call
//! that fills it ([`call`]). The web UI's react-query, reduced to what the screens use: a screen
//! keeps a `Signal<Load<T>>`, reads it in its views, and calls [`call`] again to refresh it (after a
//! change, or on a timer).
//!
//! ```ignore
//! let endpoints = signal(Load::Loading);
//! let reload = move || call(|api| async move { api.endpoints().await }, move |r| endpoints.set(Load::from(r)));
//! reload();
//! ```

use std::future::Future;

use nsh_core::prelude::*;

use crate::api::{Api, ApiError};

/// What a screen has of something it loads.
#[derive(Debug, Clone, PartialEq)]
pub enum Load<T> {
    /// The first answer has not come yet (show placeholder bars at the real size).
    Loading,
    Loaded(T),
    /// The call failed: the sentence to show in an error block, with Retry.
    Failed(String),
}

impl<T> Load<T> {
    pub fn is_loading(&self) -> bool {
        matches!(self, Load::Loading)
    }

    pub fn loaded(&self) -> Option<&T> {
        match self {
            Load::Loaded(v) => Some(v),
            _ => None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Load::Failed(e) => Some(e),
            _ => None,
        }
    }
}

impl<T> From<Result<T, ApiError>> for Load<T> {
    fn from(result: Result<T, ApiError>) -> Self {
        match result {
            Ok(v) => Load::Loaded(v),
            Err(e) => Load::Failed(e.message),
        }
    }
}

/// Runs `request` against the API in a task of the current owner (the screen: it stops with the
/// screen), then `done` with its result. Where the API cannot start (the in-process server failed),
/// `done` gets that error.
pub fn call<T, F, Fut>(request: F, done: impl FnOnce(Result<T, ApiError>) + 'static)
where
    T: 'static,
    F: FnOnce(Api) -> Fut + 'static,
    Fut: Future<Output = Result<T, ApiError>> + 'static,
{
    let _ = spawn(async move {
        let result = match crate::api() {
            Ok(api) => request(api).await,
            Err(message) => Err(ApiError { status: None, message, fields: Default::default() }),
        };
        done(result);
    });
}


/// The current time, from nsh's clock (the same on native targets and the web).
pub fn now_utc() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_millis(nsh_core::time::unix_ms() as i64).unwrap_or_default()
}
