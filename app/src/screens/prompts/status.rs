//! A prompt set's status (frontend/src/components/prompts/set-status.tsx): importing is live (a
//! green dot, as a running run), failed is a problem (a red dot), ready is plain text.

use std::time::Duration;

use nsh_core::prelude::*;

use crate::api::types::{PromptSetOut, PromptSetStatus};
use crate::ui::status::{Tone, pill, pill_small};

/// The word for a status.
pub fn status_label(status: PromptSetStatus) -> &'static str {
    match status {
        PromptSetStatus::Importing => "Importing",
        PromptSetStatus::Ready => "Ready",
        PromptSetStatus::Failed => "Failed",
    }
}

/// The tone of a status.
pub fn status_tone(status: PromptSetStatus) -> Tone {
    match status {
        PromptSetStatus::Importing => Tone::Positive,
        PromptSetStatus::Failed => Tone::Negative,
        PromptSetStatus::Ready => Tone::Plain,
    }
}

pub fn set_status(status: PromptSetStatus) -> View {
    pill(status_tone(status), status_label(status))
}

pub fn set_status_small(status: PromptSetStatus) -> View {
    pill_small(status_tone(status), status_label(status))
}

/// How often to read again while a set is importing.
pub const IMPORTING_REFRESH: Duration = Duration::from_secs(1);

pub fn any_importing(sets: &[PromptSetOut]) -> bool {
    sets.iter().any(|s| s.status == PromptSetStatus::Importing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importing_is_live_and_failed_is_a_problem() {
        assert_eq!(status_tone(PromptSetStatus::Importing), Tone::Positive);
        assert_eq!(status_tone(PromptSetStatus::Failed), Tone::Negative);
        assert_eq!(status_tone(PromptSetStatus::Ready), Tone::Plain);
        assert_eq!(status_label(PromptSetStatus::Ready), "Ready");
    }
}
