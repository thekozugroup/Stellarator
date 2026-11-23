//! Choices (frontend/src/components/ui/toggle-group.tsx, tabs.tsx, toggle.tsx, checkbox.tsx,
//! radio-group.tsx; DESIGN.md, "Components"):
//!
//! - [`segmented`]: a segmented control: a `secondary` track with 10 px corners and 2 px padding
//!   holding 8 px segments, 32 px tall; the chosen segment in the ground color with a 1 px shadow
//!   (a hairline in dark), ink text at 500; the others muted at 400.
//! - [`line_tabs`]: tabs for switching views of one list: 13 px labels on a full-width hairline;
//!   the current one ink at 500 with a 2 px ink underline; counts muted after the label.
//! - [`toggle`]: one on/off button: no fill until on (`accent`).
//! - [`checkbox`] (16 px, 5 px corners), [`radio_group`] (16 px circles) and [`switch`].

use std::rc::Rc;

use nsh_base::{checkbox as base_checkbox, radio, radio_group as base_radio_group, switch as base_switch, theme};
use nsh_core::Checked;
use nsh_core::prelude::*;

use super::icon::{self, icon};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::body;
use crate::theme::{muted_ink, radius, token};

/// A segment's shadow in light (`shadow-xs`): 0 1px 2px at 5%.
fn segment_shadow() -> [BoxShadow; 1] {
    [BoxShadow { offset_y: px(1.0), blur: px(2.0), color: nsh_core::color::hsla(0.0, 0.0, 0.0, 0.05).into(), ..BoxShadow::default() }]
}

/// A segmented control choosing one of `options` (value, label) into `value`. `fill` stretches the
/// segments across the width.
pub fn segmented<V: Clone + PartialEq + 'static>(
    options: Vec<(V, &'static str)>,
    value: Signal<V>,
    accessibility: &'static str,
    fill: bool,
) -> impl IntoElement {
    segmented_with(options, Rc::new(move || value.get()), Rc::new(move |v| value.set(v)), accessibility, fill, 13.0, false)
}

/// A segmented control with 12 px labels whose value is read with `get` and changed with `set`
/// ("Rendered | Raw").
pub fn segmented_small<V: Clone + PartialEq + 'static>(
    options: Vec<(V, &'static str)>,
    get: impl Fn() -> V + 'static,
    set: impl Fn(V) + 'static,
    accessibility: &'static str,
) -> impl IntoElement {
    segmented_with(options, Rc::new(get), Rc::new(set), accessibility, false, 12.0, false)
}

/// A segmented control whose value is read with `get` and changed with `set`, 13 px labels;
/// `fill` stretches it across its column (the Theme control on a phone).
pub fn segmented_with_get<V: Clone + PartialEq + 'static>(
    options: Vec<(V, &'static str)>,
    get: impl Fn() -> V + 'static,
    set: impl Fn(V) + 'static,
    accessibility: &'static str,
    fill: bool,
) -> impl IntoElement {
    segmented_with(options, Rc::new(get), Rc::new(set), accessibility, fill, 13.0, false)
}

/// A segmented control with mono 12 px labels, for names
/// like `max_tokens` (the New run form's "Sent as"). Read with `get`, changed with `set`.
pub fn segmented_mono<V: Clone + PartialEq + 'static>(
    options: Vec<(V, &'static str)>,
    get: impl Fn() -> V + 'static,
    set: impl Fn(V) + 'static,
    accessibility: &'static str,
) -> impl IntoElement {
    segmented_with(options, Rc::new(get), Rc::new(set), accessibility, false, 12.0, true)
}

fn segmented_with<V: Clone + PartialEq + 'static>(
    options: Vec<(V, &'static str)>,
    get: Rc<dyn Fn() -> V>,
    set: Rc<dyn Fn(V)>,
    accessibility: &'static str,
    fill: bool,
    text_size: f32,
    mono: bool,
) -> impl IntoElement {
    let segments = options.into_iter().map(move |(v, label)| {
        let chosen: Rc<dyn Fn() -> bool> = {
            let v = v.clone();
            let get = get.clone();
            Rc::new(move || get() == v)
        };
        let c1 = chosen.clone();
        let c2 = chosen.clone();
        let c3 = chosen.clone();
        let c4 = chosen.clone();
        let c5 = chosen.clone();
        let c6 = chosen.clone();
        let set = set.clone();
        let mut seg = view()
            .h(px(28.0))
            .min_w(px(28.0))
            .px(px(10.0))
            .flex_row()
            .items_center()
            .justify_center()
            .rounded(px(radius::SM))
            .border_1()
            // In dark the chosen segment has a hairline in place of the shadow.
            .border_color(move || {
                if c1() { AdaptiveColor { light: AdaptiveColor::TRANSPARENT.light, dark: theme().border.dark } } else { AdaptiveColor::TRANSPARENT }
            })
            .bg(move || if c2() { theme().background } else { AdaptiveColor::TRANSPARENT })
            .style_when(move || c3(), |s| s.shadow(segment_shadow()))
            .role(AccessibilityRole::Radio)
            .label(label)
            .accessibility_state(move || AccessibilityState { checked: Some(if c6() { Checked::True } else { Checked::False }), ..AccessibilityState::default() })
            .focusable(true)
            .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
            .cursor(Cursor::Pointer)
            .on_press({
                let v = v.clone();
                move || set(v.clone())
            })
            .child({
                let t = text(label)
                    .text_size(px(text_size))
                    .line_height(px(if text_size >= 13.0 { 18.0 } else { 16.0 }))
                    .number_of_lines(1)
                    .text_color(move || if c4() { theme().foreground } else { theme().muted_foreground })
                    .style_when(move || c5(), |s| s.font_weight(FontWeight::MEDIUM));
                if mono { t.font_family(ROBOTO_MONO.family) } else { t }
            });
        if fill {
            seg = seg.flex_1();
        }
        seg.into_element()
    });
    let track = view()
        .h(px(32.0))
        .flex_row()
        .items_center()
        .p(px(2.0))
        .rounded(px(radius::MD))
        .bg(move || theme().secondary)
        .role(AccessibilityRole::Radiogroup)
        .label(accessibility)
        .children(segments.collect::<Vec<_>>());
    if fill {
        track.into_element()
    } else {
        // In a row the track keeps its own width; the row lines up with its column's alignment.
        view().flex_row().child(track).into_element()
    }
}

/// Line tabs over one list: (value, label, count).
pub fn line_tabs<V: Clone + PartialEq + 'static>(
    tabs: Vec<(V, &'static str, Option<Rc<dyn Fn() -> String>>)>,
    value: Signal<V>,
) -> impl IntoElement {
    let items = tabs.into_iter().map(move |(v, label, count)| {
        let current = {
            let v = v.clone();
            move || value.with(|c| *c == v)
        };
        let c1 = current.clone();
        let c2 = current.clone();
        let c3 = current.clone();
        view()
            .h(px(40.0))
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .role(AccessibilityRole::Tab)
            .label(label)
            .accessibility_state(move || AccessibilityState { selected: c1(), ..AccessibilityState::default() })
            .focusable(true)
            .cursor(Cursor::Pointer)
            .on_press({
                let v = v.clone();
                move || value.set(v.clone())
            })
            .child(
                body(text(label))
                    .text_color(move || if c2() { theme().foreground } else { theme().muted_foreground })
                    .style_when(c3, |s| s.font_weight(FontWeight::MEDIUM)),
            )
            .child(count.map(|c| body(text(move || c())).tabular_nums().text_color(muted_ink)))
            .child(
                view()
                    .absolute()
                    .left(px(0.0))
                    .right(px(0.0))
                    .bottom(px(-1.0))
                    .h(px(2.0))
                    .bg(move || theme().foreground)
                    .opacity(move || if current() { 1.0_f32 } else { 0.0 }),
            )
            .into_element()
    });
    view()
        .flex_row()
        .gap(px(20.0))
        .border_b_1()
        .border_color(move || theme().border)
        .role(AccessibilityRole::Tablist)
        .children(items.collect::<Vec<_>>())
}

/// One on/off button: no fill until on (`accent`), 32 px, 10 px corners.
pub fn toggle(label: &'static str, on: Signal<bool>) -> impl IntoElement {
    view()
        .h(px(32.0))
        .min_w(px(32.0))
        .px(px(10.0))
        .flex_row()
        .items_center()
        .justify_center()
        .rounded(px(radius::MD))
        .bg(move || if on.get() { theme().accent } else { AdaptiveColor::TRANSPARENT })
        .hover(|s| s.bg(move || theme().accent))
        .role(AccessibilityRole::Button)
        .label(label)
        .accessibility_state(move || AccessibilityState { selected: on.get(), ..AccessibilityState::default() })
        .on_press(move || on.update(|v| *v = !*v))
        .child(
            body(text(label))
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || if on.get() { theme().foreground } else { theme().muted_foreground }),
        )
}

/// A checkbox and its label: 16 px, 5 px corners, the `input` edge; checked, the ink fill with a
/// check.
pub fn checkbox(label: impl Into<SharedString>, checked: Signal<bool>) -> impl IntoElement {
    checkbox_with(label, move || checked.get(), move |on| checked.set(on))
}

/// A checkbox whose state is read with `checked` and changed with `on_change`.
pub fn checkbox_with(
    label: impl Into<SharedString>,
    checked: impl Fn() -> bool + Clone + 'static,
    on_change: impl Fn(bool) + 'static,
) -> impl IntoElement {
    let label = label.into();
    let c1 = checked.clone();
    base_checkbox()
        .checked(move || checked())
        .on_change(move |next: &bool| on_change(*next))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .min_h(px(28.0))
        .self_start()
        .cursor(Cursor::Pointer)
        .indicator_style(|s| {
            s.w(px(16.0))
                .h(px(16.0))
                .items_center()
                .justify_center()
                .rounded(px(radius::XS))
                .border_1()
                .border_color(move || theme().input)
                .bg(move || theme().background)
                .checked(|s| s.bg(move || theme().primary).border_color(move || theme().primary))
        })
        .indicator(view().opacity(move || if c1() { 1.0_f32 } else { 0.0 }).child(icon(icon::CHECK, 14.0, move || theme().primary_foreground)))
        .child(body(text(label)))
}

/// Radio buttons choosing one of `options` (value, label) into `value`, 8 px apart.
pub fn radio_group<V: Clone + PartialEq + 'static>(options: Vec<(V, &'static str)>, value: Signal<V>) -> impl IntoElement {
    let items = options.into_iter().map(move |(v, label)| {
        let checked = {
            let v = v.clone();
            move || value.with(|c| *c == v)
        };
        let c1 = checked.clone();
        let c2 = checked.clone();
        radio()
            .checked(checked)
            .on_change({
                let v = v.clone();
                move |_: &bool| value.set(v.clone())
            })
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .cursor(Cursor::Pointer)
            .child(
                view()
                    .w(px(16.0))
                    .h(px(16.0))
                    .rounded(px(8.0))
                    .items_center()
                    .justify_center()
                    .border_1()
                    .border_color(move || if c1() { theme().primary } else { theme().input })
                    .bg(move || if c2() { theme().primary } else { theme().background })
                    .child(view().w(px(8.0)).h(px(8.0)).rounded(px(4.0)).bg(move || theme().primary_foreground)),
            )
            .child(body(text(label)))
            .into_element()
    });
    base_radio_group().gap(px(8.0)).children(items.collect::<Vec<_>>())
}

/// A switch and its label: a 32 × 18 track, the ink fill when on.
pub fn switch(label: impl Into<SharedString>, on: Signal<bool>) -> impl IntoElement {
    base_switch()
        .checked(move || on.get())
        .on_change(move |next: &bool| on.set(*next))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .self_start()
        .cursor(Cursor::Pointer)
        .track_style(|s| {
            s.w(px(32.0))
                .h(px(18.0))
                .p(px(2.0))
                .rounded(px(9.0))
                .bg(move || theme().input)
                .transition_colors()
                .checked(|s| s.bg(move || theme().primary))
        })
        .thumb_style(|s| {
            s.w(px(14.0))
                .h(px(14.0))
                .rounded(px(7.0))
                .bg(move || theme().background)
                .transition_transform()
                .checked(|s| s.translate_x(px(14.0)))
        })
        .child(body(text(label.into())))
}
