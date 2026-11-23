//! Tooltips, keyboard keys and the copy button (frontend/src/components/ui/tooltip.tsx, kbd.tsx,
//! shared/copy-button.tsx).

use std::time::Duration;

use nsh_base::{TooltipElement, theme, tooltip};
use nsh_core::prelude::*;

use super::button::{Button, Size, Variant};
use super::icon;
use super::material::{material_float, material_thick, popover_ink};
use crate::theme::radius;

/// A tooltip's bubble: the thick material, 8 px corners, 12 px text, after a 700 ms hover (keyboard
/// focus shows it at once). Put it on a view with `.tooltip(tooltip_bubble("…"))`.
pub fn tooltip_bubble(label: impl Into<SharedString>) -> TooltipElement {
    let label = label.into();
    let bubble = tooltip().delay(700.0).offset(px(4.0)).rounded(px(radius::SM)).px(px(8.0)).py(px(4.0)).max_w(px(320.0));
    material_float(material_thick(bubble))
        .child(text(label).text_size(px(12.0)).line_height(px(16.0)).text_color(popover_ink))
}

/// A key as it is labeled on the keyboard (`key_label("mod+[")` gives "⌘[" on a Mac, "Ctrl+[" on
/// Windows and Linux): a 20 px chip in the raised tone with 12 px muted text.
pub fn kbd(keys: impl Into<SharedString>) -> impl IntoElement {
    view()
        .h(px(20.0))
        .min_w(px(20.0))
        .px(px(4.0))
        .flex_row()
        .items_center()
        .justify_center()
        .rounded(px(radius::XS))
        .bg(move || theme().secondary)
        .child(
            text(keys.into())
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().muted_foreground),
        )
}

/// A ghost icon button that copies `value` and shows a check for 1.5 s. `label` names it
/// ("Copy base URL").
pub fn copy_button(value: impl Into<String>, label: impl Into<SharedString>) -> impl IntoElement {
    let value = value.into();
    let copied = signal(false);
    let label = label.into();
    view().child(dynamic(move || {
        let value = value.clone();
        let glyph = if copied.get() { icon::CHECK } else { icon::COPY };
        Button::icon(glyph, if copied.get() { SharedString::from("Copied") } else { label.clone() })
            .variant(Variant::Ghost)
            .size(Size::IconSm)
            .muted()
            .on_press(move || {
                let _ = set_clipboard(value.clone());
                copied.set(true);
                let _ = set_timeout(Duration::from_millis(1500), move || copied.set(false));
            })
    }))
}
