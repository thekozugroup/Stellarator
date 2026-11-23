//! Status (frontend/src/components/shared/status-pill.tsx; DESIGN.md, "Components", Status). Two
//! forms of the same thing:
//! - [`pill`], in lists and tables: a 6 px dot in the state's color and the word. Normal states
//!   ("Done", "Completed", "Pending") are a plain muted word with no dot.
//! - [`status_badge`], in a detail screen's header: a 28 px capsule in the state's tint with the
//!   dot (or an icon) and the word in the state's ink.
//!
//! Color never carries meaning alone; the word is always there.

use nsh_base::theme;
use nsh_core::element::ImageAsset;
use nsh_core::prelude::*;

use super::icon::{self, icon};
use crate::api::types::{RunStatus, SampleStatus};
use crate::theme::text::body;
use crate::theme::{ink, muted_ink, token};

/// What a state means: its colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// A normal state: a muted word, no dot.
    Plain,
    /// A gray dot (waiting).
    Neutral,
    /// Periwinkle (thinking).
    Accent,
    /// Green (running, live, answering, reachable).
    Positive,
    /// Red (failed, unreachable).
    Negative,
    /// Coral (paused, truncated).
    Attention,
}

impl Tone {
    /// The dot's color.
    pub fn dot(self) -> AdaptiveColor {
        match self {
            Tone::Plain | Tone::Neutral => theme().muted_foreground,
            Tone::Accent => token("periwinkle")(),
            Tone::Positive => token("positive")(),
            Tone::Negative => theme().destructive,
            Tone::Attention => token("coral")(),
        }
    }

    /// A badge's fill.
    pub fn tint(self) -> AdaptiveColor {
        match self {
            Tone::Plain | Tone::Neutral => theme().secondary,
            Tone::Accent => token("periwinkle-tint")(),
            Tone::Positive => token("positive-tint")(),
            Tone::Negative => token("negative-tint")(),
            Tone::Attention => token("coral-tint")(),
        }
    }

    /// A badge's ink.
    pub fn ink(self) -> AdaptiveColor {
        match self {
            Tone::Plain => theme().muted_foreground,
            Tone::Neutral => theme().foreground,
            Tone::Accent => token("periwinkle-ink")(),
            Tone::Positive => token("positive-ink")(),
            Tone::Negative => token("negative-ink")(),
            Tone::Attention => token("coral-ink")(),
        }
    }
}

fn dot(tone: Tone) -> impl IntoElement {
    view().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).flex_none().bg(move || tone.dot())
}

/// The dot and the word (lists and tables).
pub fn pill(tone: Tone, word: impl Into<SharedString>) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .flex_none()
        .child((tone != Tone::Plain).then(|| dot(tone)))
        .child(body(text(word.into())).number_of_lines(1).text_color(move || if tone == Tone::Plain { muted_ink() } else { ink() }))
}

/// The dot and the word at 12 px, for a "·" separated line of meta (phone rows).
pub fn pill_small(tone: Tone, word: impl Into<SharedString>) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .flex_none()
        .child((tone != Tone::Plain).then(|| dot(tone)))
        .child(
            text(word.into())
                .text_size(px(12.0))
                .line_height(px(16.0))
                .number_of_lines(1)
                .text_color(move || if tone == Tone::Plain { muted_ink() } else { ink() }),
        )
}

/// A 28 px capsule in the state's tint: the dot (or `glyph`) and the word in the state's ink, at 500
/// (detail headers).
pub fn status_badge(tone: Tone, glyph: Option<ImageAsset>, word: impl Into<SharedString>) -> View {
    let leading: Option<AnyElement> = match glyph {
        Some(g) => Some(icon(g, 14.0, move || tone.ink()).into_element()),
        None if tone != Tone::Plain => Some(dot(tone).into_element()),
        None => None,
    };
    view()
        .h(px(28.0))
        .flex_none()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .px(px(12.0))
        .rounded(px(14.0))
        .bg(move || tone.tint())
        .child(leading)
        .child(
            body(text(word.into()))
                .font_weight(FontWeight::MEDIUM)
                .number_of_lines(1)
                .text_color(move || tone.ink()),
        )
}

/// A run's tone and word.
pub fn run_tone(status: RunStatus) -> (Tone, &'static str) {
    match status {
        RunStatus::Running => (Tone::Positive, "Running"),
        RunStatus::Pausing => (Tone::Attention, "Pausing"),
        RunStatus::Paused => (Tone::Attention, "Paused"),
        RunStatus::Completed => (Tone::Plain, "Completed"),
    }
}

/// A run's status in lists: the dot and the word.
pub fn run_status_pill(status: RunStatus) -> View {
    let (tone, word) = run_tone(status);
    pill(tone, word)
}

/// A run's status in its header. Completed is a neutral badge with a check, so a finished run
/// never reads as a live one; the rest use their list tones.
pub fn run_status_badge(status: RunStatus) -> View {
    match status {
        RunStatus::Completed => status_badge(Tone::Neutral, Some(icon::CHECK), "Completed"),
        other => {
            let (tone, word) = run_tone(other);
            status_badge(tone, None, word)
        }
    }
}

/// A sample's status in lists; a finished sample cut off by max tokens reads "Truncated".
pub fn sample_status_pill(status: SampleStatus, finish_reason: Option<&str>) -> View {
    if status == SampleStatus::Done && finish_reason == Some("length") {
        return pill(Tone::Attention, "Truncated");
    }
    let (tone, word) = match status {
        SampleStatus::Pending => (Tone::Plain, "Pending"),
        SampleStatus::Running => (Tone::Positive, "In flight"),
        SampleStatus::Done => (Tone::Plain, "Done"),
        SampleStatus::Failed => (Tone::Negative, "Failed"),
    };
    pill(tone, word)
}
