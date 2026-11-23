//! The (i) button after a label that explains a field in a sentence or two
//! (frontend/src/components/shared/info-tip.tsx): it opens on hover (after 300 ms), on keyboard
//! focus and on a press; Escape or a press outside closes it. A popover in the thick material,
//! 14 px corners, 12 px padding, at most 288 px wide.

use nsh_base::{popover, theme};
use nsh_core::prelude::*;

use super::icon::{self, icon};
use super::material::{material_float, material_thick, popover_ink};
use crate::theme::radius;
use crate::theme::text::body;

/// An (i) button about `label` that shows `explanation`.
pub fn info_tip(label: impl Into<SharedString>, explanation: impl Into<SharedString>) -> impl IntoElement {
    let label = label.into();
    let explanation = explanation.into();
    let trigger = view()
        .w(px(20.0))
        .h(px(20.0))
        .items_center()
        .justify_center()
        .rounded(px(10.0))
        .role(AccessibilityRole::Button)
        .label(format!("About {label}"))
        .accessibility_description(explanation.clone())
        .cursor(Cursor::Pointer)
        .child(icon(icon::INFO, 16.0, move || theme().muted_foreground))
        .hover(|s| s.opacity(0.8));
    let panel = material_float(material_thick(view().max_w(px(288.0)).p(px(12.0)).rounded(px(radius::LG))))
        .child(body(text(explanation)).text_color(popover_ink));
    popover().placement(Placement::Top).offset(px(6.0)).margin(px(16.0)).trigger(trigger).child(panel)
}
