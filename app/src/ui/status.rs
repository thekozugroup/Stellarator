//! A run's status in lists and tables (DESIGN.md, "Components", Status): a 6 px dot in the
//! state's color and the word. Normal states (Completed) are a plain muted word with no dot.
//! Running is green; Pausing and Paused coral.

use nsh_core::prelude::*;

use crate::api::RunStatus;
use crate::theme::text::body;
use crate::theme::{ink, muted_ink, token};

/// The word for `status`.
pub fn run_label(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Running => "Running",
        RunStatus::Pausing => "Pausing",
        RunStatus::Paused => "Paused",
        RunStatus::Completed => "Completed",
    }
}

/// The dot's color token for `status`; `None` for a plain state.
pub fn run_dot(status: RunStatus) -> Option<&'static str> {
    match status {
        RunStatus::Running => Some("positive"),
        RunStatus::Pausing | RunStatus::Paused => Some("coral"),
        RunStatus::Completed => None,
    }
}

/// The dot and the word.
pub fn run_status(status: RunStatus) -> impl IntoElement {
    let dot = run_dot(status);
    view()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .child(dot.map(|t| view().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).flex_none().bg(token(t))))
        .child(body(text(run_label(status))).text_color(move || if dot.is_some() { ink() } else { muted_ink() }))
}
