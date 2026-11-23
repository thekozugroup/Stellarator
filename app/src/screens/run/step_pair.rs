//! Previous and next as a ‹ › capsule pair (frontend/src/components/runs/step-pair.tsx), the way
//! macOS and iOS step through a series: one raised capsule holding two round 26 px buttons, each
//! with its own name ("Previous sample") and, where it has one, its key in the tooltip.

use std::rc::Rc;

use nsh_base::{button as base_button, theme};
use nsh_core::prelude::*;

use crate::theme::token;
use crate::ui::icon::{self, icon};
use crate::ui::tooltip::tooltip_bubble;

/// One direction: its name, its key, whether there is nothing that way, and what it does.
pub struct Step {
    pub label: &'static str,
    pub shortcut: Option<&'static str>,
    pub disabled: Rc<dyn Fn() -> bool>,
    pub on_press: Rc<dyn Fn()>,
}

fn step_button(step: Step, glyph: nsh_core::element::ImageAsset) -> impl IntoElement {
    let tip = match step.shortcut {
        Some(k) => format!("{} ({})", step.label, k.to_uppercase()),
        None => step.label.to_owned(),
    };
    let disabled = step.disabled.clone();
    let press = step.on_press.clone();
    let b = base_button()
        .disabled(move || disabled())
        .on_press(move || press())
        .w(px(26.0))
        .h(px(26.0))
        .rounded(px(13.0))
        .items_center()
        .justify_center()
        .hover(|s| s.bg(move || theme().accent))
        .disabled_style(|s| s.opacity(0.5))
        .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .accessibility_label(Some(SharedString::from(step.label)))
        .child(icon(glyph, 16.0, move || theme().foreground));
    view().tooltip(tooltip_bubble(tip)).child(b)
}

/// The pair.
pub fn step_pair(prev: Step, next: Step) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(px(15.0))
        .bg(move || theme().secondary)
        .self_start()
        .child(step_button(prev, icon::CHEVRON_LEFT))
        .child(step_button(next, icon::CHEVRON_RIGHT))
}
