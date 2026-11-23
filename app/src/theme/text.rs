//! The type scale of DESIGN.md ("Typography"), pointer-fine sizes. Each function styles a text
//! element for one role; tracking in em is converted to points at that size.

use nsh_core::prelude::*;

use super::fonts::{INTER_DISPLAY, ROBOTO_MONO};
use super::{ink, muted_ink};

/// Tracking of `em` at `size` points, in points.
fn tracking(em: f32, size: f32) -> Points {
    px(em * size)
}

/// Body and controls (`text-sm`): 13 px on 18 px, -0.003em, ink.
pub fn body<T: Styled>(t: T) -> T {
    t.text_size(px(13.0)).line_height(px(18.0)).letter_spacing(tracking(-0.003, 13.0)).text_color(ink)
}

/// Body text at 500: labels of rows and buttons.
pub fn label<T: Styled>(t: T) -> T {
    body(t).font_weight(FontWeight::MEDIUM)
}

/// Meta and table headers (`text-xs`): 12 px on 16 px, muted.
pub fn meta<T: Styled>(t: T) -> T {
    t.text_size(px(12.0)).line_height(px(16.0)).text_color(muted_ink)
}

/// The toolbar title and section headings (`text-base`): 15 px on 20 px at 600, -0.011em.
pub fn title<T: Styled>(t: T) -> T {
    t.text_size(px(15.0))
        .line_height(px(20.0))
        .font_weight(FontWeight::SEMIBOLD)
        .letter_spacing(tracking(-0.011, 15.0))
        .text_color(ink)
}

/// Data (`font-mono`): Roboto Mono, 12 px on 16 px, ink.
pub fn mono<T: Styled>(t: T) -> T {
    t.font_family(ROBOTO_MONO.family).text_size(px(12.0)).line_height(px(16.0)).letter_spacing(px(0.0)).text_color(ink)
}

/// The display title of a screen: Inter Display, 32 px at 600, line height 1.1, -0.03em.
pub fn display<T: Styled>(t: T) -> T {
    t.font_family(INTER_DISPLAY.family)
        .text_size(px(32.0))
        .line_height(px(35.0))
        .font_weight(FontWeight::SEMIBOLD)
        .letter_spacing(tracking(-0.03, 32.0))
        .text_color(ink)
}
