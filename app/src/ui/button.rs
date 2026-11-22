//! Buttons (DESIGN.md, "Components"): 32 px, 10 px corners, 13 px labels at 500. `Primary` is the
//! ink fill with inverse text, at most one per view; `Secondary` is the raised tone with ink text.

use nsh_base::{button as base_button, theme};
use nsh_core::element::ImageAsset;
use nsh_core::prelude::*;

use super::icon::icon;
use crate::theme::metrics::CONTROL;
use crate::theme::radius;
use crate::theme::text::label;

/// Which kind of button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Secondary,
}

/// A button with an optional leading icon.
pub fn button(
    variant: Variant,
    glyph: Option<ImageAsset>,
    text_label: &'static str,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    let fill = move || match variant {
        Variant::Primary => theme().primary,
        Variant::Secondary => theme().secondary,
    };
    let ink = move || match variant {
        Variant::Primary => theme().primary_foreground,
        Variant::Secondary => theme().secondary_foreground,
    };
    base_button()
        .label(text_label)
        .on_press(on_press)
        .h(px(CONTROL))
        .px(px(12.0))
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(radius::MD))
        .bg(fill)
        .hover(|s| s.opacity(0.9))
        .active(|s| s.opacity(0.8))
        .child(glyph.map(|g| icon(g, 16.0, ink)))
        .child(label(text(text_label)).text_color(ink))
}
