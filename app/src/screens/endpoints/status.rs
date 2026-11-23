//! An endpoint's state (frontend/src/components/endpoints/endpoint-status.tsx): Reachable (green),
//! Unreachable or Key rejected (red), Not checked (plain), with the server's reason for a failure.

use chrono::{DateTime, Utc};
use nsh_core::prelude::*;

use crate::api::types::EndpointOut as Endpoint;
use crate::format::{DASH, relative};
use crate::theme::muted_ink;
use crate::theme::text::{body, meta};
use crate::ui::status::{Tone, pill, pill_small, status_badge};

/// The server answered but refused the key (the server words it "The server rejected the API key
/// (401)…").
pub fn is_key_rejected(error: Option<&str>) -> bool {
    error.is_some_and(|e| e.to_lowercase().starts_with("the server rejected the api key"))
}

/// The server's own words after its status code: "The server rejected the API key (401): User not
/// found." gives "401: User not found.".
pub fn server_reason(error: Option<&str>) -> Option<String> {
    let error = error?;
    let bytes = error.as_bytes();
    let at = (0..bytes.len().saturating_sub(4)).find(|&i| {
        bytes[i] == b'(' && bytes[i + 1..i + 4].iter().all(u8::is_ascii_digit) && bytes[i + 4] == b')'
    })?;
    let code = &error[at + 1..at + 4];
    let mut rest = error[at + 5..].trim_start();
    if let Some(c) = rest.chars().next().filter(|c| matches!(c, ':' | '.' | '-' | '–')) {
        rest = rest[c.len_utf8()..].trim_start();
    }
    let body = rest.trim();
    Some(if body.is_empty() { code.to_owned() } else { format!("{code}: {body}") })
}

/// The state as a word, a tone and, for a failure, the reason.
pub struct State {
    pub word: &'static str,
    pub tone: Tone,
    pub reason: Option<String>,
}

pub fn endpoint_state(endpoint: &Endpoint) -> State {
    match endpoint.last_ok {
        None => State { word: "Not checked", tone: Tone::Plain, reason: None },
        Some(true) => State { word: "Reachable", tone: Tone::Positive, reason: None },
        Some(false) => {
            let error = endpoint.last_error.as_deref();
            let rejected = is_key_rejected(error);
            State {
                word: if rejected { "Key rejected" } else { "Unreachable" },
                tone: Tone::Negative,
                reason: if rejected { server_reason(error) } else { endpoint.last_error.clone() },
            }
        }
    }
}

/// In lists: the dot and the word, and for a failing endpoint the reason under it in 12 px muted
/// text (two lines at most).
pub fn endpoint_status(endpoint: &Endpoint) -> impl IntoElement {
    let state = endpoint_state(endpoint);
    view()
        .gap(px(2.0))
        .items_start()
        .child(pill(state.tone, state.word))
        .child(state.reason.map(|r| meta(text(r)).number_of_lines(2).max_w(px(272.0))))
}

/// The word only at 12 px, for the phone rows' "·" separated line.
pub fn endpoint_status_small(endpoint: &Endpoint) -> impl IntoElement {
    let state = endpoint_state(endpoint);
    pill_small(state.tone, state.word)
}

/// In a header (the details sheet): the status as a capsule in its tint.
pub fn endpoint_status_badge(endpoint: &Endpoint) -> impl IntoElement {
    let state = endpoint_state(endpoint);
    status_badge(state.tone, None, state.word)
}

/// When it was last checked ("5 minutes ago"), or the dash.
pub fn checked_at(endpoint: &Endpoint, now: DateTime<Utc>) -> String {
    endpoint.last_checked_at.map(|t| relative(t, now)).unwrap_or_else(|| DASH.to_owned())
}

/// [`checked_at`] in 13 px muted text.
pub fn checked_at_text(endpoint: &Endpoint, now: DateTime<Utc>) -> impl IntoElement {
    body(text(checked_at(endpoint, now))).text_color(muted_ink).number_of_lines(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_servers_reason_follows_its_status_code() {
        let e = "The server rejected the API key (401): User not found.";
        assert!(is_key_rejected(Some(e)));
        assert_eq!(server_reason(Some(e)).as_deref(), Some("401: User not found."));
        assert_eq!(server_reason(Some("The server rejected the API key (403)")).as_deref(), Some("403"));
        assert_eq!(server_reason(Some("Connection refused")), None);
        assert!(!is_key_rejected(Some("Connection refused")));
    }
}
