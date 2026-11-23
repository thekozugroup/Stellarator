//! Chips (frontend/src/components/shared/chip.tsx; DESIGN.md, "Components", Chips): 28 px capsules
//! in the raised tone with 13 px ink text. In a detail screen's header, after the status badge,
//! chips name the objects the screen uses (model, endpoint, prompt set). A chip with a press
//! action shows the highlight on hover; `mono` sets its text in Roboto Mono (model names, ids);
//! `leading` holds a 20 px circle (a plasma tile, clipped round, concentric with the capsule).

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::body;

/// A chip, built with the methods below.
pub struct Chip {
    label: SharedString,
    mono: bool,
    leading: Option<AnyElement>,
    on_press: Option<Rc<dyn Fn()>>,
    selected: Option<Rc<dyn Fn() -> bool>>,
    trigger: bool,
}

pub fn chip(label: impl Into<SharedString>) -> Chip {
    Chip { label: label.into(), mono: false, leading: None, on_press: None, selected: None, trigger: false }
}

impl Chip {
    /// Roboto Mono, 12 px (model names, ids).
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// A 20 px circle before the label: a plasma tile or an icon.
    pub fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading = Some(element.into_element());
        self
    }

    /// The chip is a link: pressing it runs `f`, hovering it shows the highlight.
    pub fn on_press(mut self, f: impl Fn() + 'static) -> Self {
        self.on_press = Some(Rc::new(f));
        self
    }

    /// The trigger of a popover (the run's tools): it looks like a chip that links and reads as a
    /// button, and the popover takes the press.
    pub fn popover_trigger(mut self) -> Self {
        self.trigger = true;
        self
    }

    /// A choice chip (the endpoint presets): the accent tint and ink while `selected` holds.
    pub fn selected(mut self, selected: impl Fn() -> bool + 'static) -> Self {
        self.selected = Some(Rc::new(selected));
        self
    }
}

impl IntoElement for Chip {
    fn into_element(self) -> AnyElement {
        let selected = self.selected.clone();
        let is_selected = move || selected.as_ref().is_some_and(|s| s());
        let s1 = is_selected.clone();
        let s2 = is_selected.clone();
        let mut label = body(text(self.label.clone())).number_of_lines(1).text_color(move || {
            if s1() { crate::theme::token("periwinkle-ink")() } else { theme().foreground }
        });
        if self.mono {
            label = label.font_family(ROBOTO_MONO.family).text_size(px(12.0)).line_height(px(16.0)).letter_spacing(px(0.0));
        }
        label = label.style_when(s2, |s| s.font_weight(FontWeight::MEDIUM));
        let leading = self.leading.map(|l| view().w(px(20.0)).h(px(20.0)).rounded(px(10.0)).overflow_hidden().flex_none().child(l));
        let has_leading = leading.is_some();
        let s3 = is_selected.clone();
        let mut c = view()
            .h(px(28.0))
            .min_w(px(0.0))
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .pl(px(if has_leading { 4.0 } else { 12.0 }))
            .pr(px(12.0))
            .rounded(px(14.0))
            .bg(move || if s3() { crate::theme::token("periwinkle-tint")() } else { theme().secondary })
            .child(leading)
            .child(label);
        if self.on_press.is_some() || self.trigger {
            c = c
                .role(AccessibilityRole::Button)
                .label(self.label.clone())
                .hover(move |s| s.bg(move || theme().accent))
                .cursor(Cursor::Pointer)
                .focusable(true)
                .focus_visible(|s| s.ring(px(2.0)).ring_color(crate::theme::token("ring")).ring_offset(px(2.0)));
        }
        if let Some(f) = self.on_press {
            c = c.on_press(move || f());
        }
        c.into_element()
    }
}

/// A row of chips (and a status badge first), wrapping, 8 px apart.
pub fn chip_row(children: Vec<AnyElement>) -> impl IntoElement {
    view().flex_row().flex_wrap().items_center().gap(px(8.0)).min_w(px(0.0)).children(children)
}
