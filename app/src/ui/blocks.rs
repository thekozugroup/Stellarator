//! Block states (frontend/src/components/shared/block-states.tsx): a failure inside the block that
//! failed (never a full-screen takeover), an empty section that keeps its frame and says what will
//! appear, and flat gray placeholder bars at the size of what they stand for (no shimmer).

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::button::{Size, Variant, button};
use super::icon;
use crate::theme::text::body;
use crate::theme::{muted_ink, radius, token};

/// What failed, on the red tint, with Retry when `on_retry` is given.
pub fn error_block(message: impl Into<SharedString>, on_retry: Option<Rc<dyn Fn()>>) -> impl IntoElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .px(px(16.0))
        .py(px(12.0))
        .rounded(px(radius::XL))
        .bg(token("negative-tint"))
        .role(AccessibilityRole::Alert)
        .child(body(text(message.into())).flex_1().min_w(px(0.0)).text_color(token("negative-ink")))
        .child(on_retry.map(|retry| {
            button("Retry").variant(Variant::Secondary).on_card().size(Size::Sm).leading(icon::RETRY).on_press(move || retry())
        }))
}

/// An empty section: a sentence (or more) about what will appear, then the action that fills it.
pub fn empty_block(children: Vec<AnyElement>, action: Option<AnyElement>) -> View {
    view()
        .items_start()
        .gap(px(12.0))
        .py(px(24.0))
        .child(view().max_w(px(560.0)).gap(px(4.0)).children(children))
        .child(action)
}

/// A muted sentence for empty states.
pub fn empty_text(sentence: impl Into<SharedString>) -> impl IntoElement {
    body(text(sentence.into())).text_color(muted_ink)
}

/// A flat gray bar `w` by `h` points: a placeholder for a value still loading.
pub fn bar(w: f32, h: f32) -> impl IntoElement {
    view().w(px(w)).h(px(h)).rounded(px(radius::XS)).bg(move || theme().secondary).accessibility_elements_hidden(true)
}
