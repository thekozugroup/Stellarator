//! The progress bar (DESIGN.md, "Components"): a 6 px capsule, the `secondary` track, periwinkle
//! for done and red for failed at their true ratio, always labeled in text beside it.

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::theme::token;

/// The bar's fills as fractions of the track: (done, failed), each clamped so both fit.
pub fn fractions(done: i64, failed: i64, total: i64) -> (f32, f32) {
    if total <= 0 {
        return (0.0, 0.0);
    }
    let d = (done.max(0) as f32 / total as f32).min(1.0);
    let f = (failed.max(0) as f32 / total as f32).min(1.0 - d);
    (d, f)
}

/// A bar `width` points wide.
pub fn progress_bar(done: i64, failed: i64, total: i64, width: f32) -> impl IntoElement {
    let (d, f) = fractions(done, failed, total);
    view()
        .w(px(width))
        .h(px(6.0))
        .flex_none()
        .flex_row()
        .rounded(px(3.0))
        .overflow_hidden()
        .bg(move || theme().secondary)
        .child(view().h(px(6.0)).w(px(width * d)).bg(token("periwinkle")))
        .child(view().h(px(6.0)).w(px(width * f)).bg(move || theme().destructive))
}

#[cfg(test)]
mod tests {
    use super::fractions;

    #[test]
    fn the_bar_shows_the_true_ratio() {
        assert_eq!(fractions(0, 0, 0), (0.0, 0.0));
        assert_eq!(fractions(50, 0, 100), (0.5, 0.0));
        assert_eq!(fractions(50, 25, 100), (0.5, 0.25));
        assert_eq!(fractions(120, 10, 100), (1.0, 0.0));
    }
}
