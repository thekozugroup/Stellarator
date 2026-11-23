//! One response to the prompt (frontend/src/components/preferences/candidate-card.tsx; DESIGN.md,
//! Cards): a `muted` card with 16 px corners and no border. The thinking trace (folded) and the
//! response sit in ground wells with 8 px corners, inset 8 px from the card so the corners are
//! concentric. The letter is the response's name on screen and its key (A is 1, B is 2).
//!
//! A response of a run with tools also has its tool calls, folded to one line in a ground well
//! ("3 tool calls · lookup, search", `ui::trace::compact_calls`) between the thinking trace and the
//! response; open, a line per call with the start of its result.
//!
//! The chosen response carries the accent: a 2 px periwinkle ring and a CHOSEN tag. The rejected
//! one steps back: its text turns muted and it carries a REJECTED tag. Choose and Reject are the
//! two segments of one control at the card's top right.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::logic::{Mark, TraceUnit, card_identity, trace_size};
use crate::api::types::Candidate;
use crate::format::{count, middle};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::{body, meta};
use crate::theme::{ink, muted_ink, radius, token};
use crate::ui::icon::{self, icon};
use crate::ui::model_text::{ModelTextStyle, model_text_with};
use crate::ui::page::window_width;
use crate::ui::status::{Tone, pill_small};
use crate::ui::tooltip::tooltip_bubble;
use crate::ui::trace::{all_calls, compact_calls, trace_rounds};

/// What a card shows and does.
pub struct Card {
    pub candidate: Candidate,
    /// "A", "B", …
    pub label: String,
    /// 1-based: the digit that chooses it.
    pub position: usize,
    /// Its mark now (reactive).
    pub mark: Rc<dyn Fn() -> Option<Mark>>,
    pub show_model: bool,
    pub show_sample: bool,
    pub trace_unit: TraceUnit,
    /// The marks take no presses while the next prompt loads (reactive).
    pub disabled: Rc<dyn Fn() -> bool>,
    pub on_mark: Rc<dyn Fn(Mark)>,
}

/// The card.
pub fn candidate_card(card: Card) -> impl IntoElement {
    let c = card.candidate.clone();
    let mark = card.mark.clone();
    let m1 = mark.clone();
    let m2 = mark.clone();
    let rejected = Rc::new(move || m2() == Some(Mark::Rejected));
    let truncated = crate::ui::status::is_truncated(c.finish_reason.as_deref());
    let (identity, sample_note) = card_identity(&c, card.show_model, card.show_sample);
    let phone = window_width() < 640.0;

    // The letter in a 28 px ground tile.
    let r1 = rejected.clone();
    let letter = view()
        .w(px(28.0))
        .h(px(28.0))
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(radius::SM))
        .bg(move || theme().background)
        .role(AccessibilityRole::Header)
        .label(format!("Response {}", card.label))
        .child(
            body(text(card.label.clone()))
                .font_weight(FontWeight::SEMIBOLD)
                .tabular_nums()
                .text_color(move || if r1() { muted_ink() } else { ink() }),
        );

    // The name: the model in mono (shortened in the middle), the sample or the run.
    let r2 = rejected.clone();
    let shown = if card.show_model { middle(&identity, 52) } else { identity.clone() };
    let mut name = body(text(shown))
        .line_height(px(20.0))
        .number_of_lines(1)
        .text_color(move || if r2() { muted_ink() } else { ink() })
        .tooltip(tooltip_bubble(if card.show_model { c.model.clone() } else { c.run_name.clone() }));
    if card.show_model {
        // `text-xs` sets its own line height over `leading-5`: 16 px.
        name = name.font_family(ROBOTO_MONO.family).text_size(px(12.0)).line_height(px(16.0)).letter_spacing(px(0.0));
    }
    let note = (sample_note.is_some() || truncated).then(|| {
        view()
            .flex_row()
            .items_center()
            .gap_x(px(8.0))
            .min_w(px(0.0))
            .child(sample_note.map(|s| meta(text(s)).number_of_lines(1)))
            .child(truncated.then(|| pill_small(Tone::Attention, "Truncated")))
    });
    let names = view().min_w(px(0.0)).flex_shrink(1.0).gap(px(2.0)).child(name).child(note);

    let tag = dynamic(move || m1().map(mark_tag));

    let lead = view()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .min_h(px(32.0))
        .min_w(px(0.0))
        .basis(px(if phone { 0.0 } else { 192.0 }))
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .child(letter)
        .child(names)
        .child(tag);

    let d1 = card.disabled.clone();
    let d2 = card.disabled.clone();
    let on1 = card.on_mark.clone();
    let on2 = card.on_mark.clone();
    let m3 = mark.clone();
    let m4 = mark.clone();
    let hint = |p: usize, shift: bool| (p <= 9).then(|| if shift { format!("Shift+{p}") } else { p.to_string() });
    let control = view()
        .flex_row()
        .items_center()
        .h(px(32.0))
        .flex_none()
        .p(px(2.0))
        .rounded(px(radius::MD))
        .bg(move || theme().secondary)
        .role(AccessibilityRole::Group)
        .label(format!("Mark response {}", card.label))
        .when(phone, |v| v.ml(nsh_core::geometry::auto()))
        .child(mark_button(Mark::Chosen, move || m3() == Some(Mark::Chosen), d1, move || on1(Mark::Chosen), hint(card.position, false), &card.label, phone))
        .child(mark_button(Mark::Rejected, move || m4() == Some(Mark::Rejected), d2, move || on2(Mark::Rejected), hint(card.position, true), &card.label, phone));

    // The header: 20 px in at the start, the control 6 px from the card's top and end edges.
    let header = view()
        .flex_row()
        .when(!phone, |v| v.flex_wrap())
        .items_start()
        .gap_x(px(12.0))
        .gap_y(px(4.0))
        .pt(px(6.0))
        .pr(px(6.0))
        .pb(px(8.0))
        .pl(px(20.0))
        .child(lead)
        .child(control);

    let r3 = rejected.clone();
    let trace: AnyElement = match c.reasoning.clone() {
        Some(reasoning) => thinking_trace(reasoning, trace_size(&c, card.trace_unit)).into_element(),
        None => body(text("No thinking trace")).text_color(muted_ink).px(px(12.0)).py(px(4.0)).into_element(),
    };
    let response = c.response.clone().unwrap_or_default();
    let response_well = view().rounded(px(radius::SM)).bg(move || theme().background).px(px(12.0)).py(px(12.0)).child(
        if response.trim().is_empty() {
            body(text("The model returned an empty response.")).text_color(muted_ink).into_element()
        } else {
            model_text_with(move || response.clone(), None, ModelTextStyle::small().muted_when(move || r3())).into_element()
        },
    );
    let tokens = c.completion_tokens.map(|n| {
        meta(text(count(n, "completion token", "completion tokens"))).tabular_nums().px(px(12.0)).pt(px(2.0)).pb(px(4.0))
    });
    let calls = c.trace.as_ref().map(|t| all_calls(&trace_rounds(t))).filter(|calls| !calls.is_empty()).map(|calls| {
        let r4 = rejected.clone();
        compact_calls(calls, Rc::new(move || r4()))
    });
    let content = view().gap(px(8.0)).px(px(8.0)).pb(px(8.0)).min_w(px(0.0)).child(trace).child(calls).child(response_well).child(tokens);

    let m5 = mark.clone();
    view()
        .min_w(px(0.0))
        .rounded(px(radius::XL))
        .bg(move || theme().muted)
        .ring(move || px(if m5() == Some(Mark::Chosen) { 2.0 } else { 0.0 }))
        .ring_color(token("periwinkle"))
        .role(AccessibilityRole::Group)
        .label(format!("Response {}", card.label))
        .child(header)
        .child(content)
}

/// CHOSEN on the accent tint, REJECTED on the raised gray: a 24 px data tag (`label-mono`).
fn mark_tag(mark: Mark) -> impl IntoElement {
    let chosen = mark == Mark::Chosen;
    view()
        .h(px(24.0))
        .flex_none()
        .flex_row()
        .items_center()
        .px(px(10.0))
        .rounded(px(12.0))
        .bg(move || if chosen { token("periwinkle-tint")() } else { theme().secondary })
        .child(
            text(if chosen { "CHOSEN" } else { "REJECTED" })
                .font_family(ROBOTO_MONO.family)
                .text_size(px(12.0))
                .line_height(px(16.0))
                .letter_spacing(px(0.48))
                .text_color(move || if chosen { token("periwinkle-ink")() } else { muted_ink() }),
        )
}

/// Choose or Reject: a 28 px segment, raised on the track with ink text while pressed, muted
/// otherwise. Phones show the icons only; the accessible name keeps the word.
fn mark_button(
    kind: Mark,
    pressed: impl Fn() -> bool + Clone + 'static,
    disabled: Rc<dyn Fn() -> bool>,
    on_press: impl Fn() + 'static,
    key: Option<String>,
    label: &str,
    phone: bool,
) -> impl IntoElement {
    let (glyph, word) = if kind == Mark::Chosen { (icon::CHECK, "Choose") } else { (icon::X, "Reject") };
    let p1 = pressed.clone();
    let p2 = pressed.clone();
    let p3 = pressed.clone();
    let p4 = pressed.clone();
    let p5 = pressed.clone();
    let p6 = pressed.clone();
    let d1 = disabled.clone();
    let tip = match &key {
        Some(k) => format!("{word} ({k})"),
        None => word.to_owned(),
    };
    view()
        .h(px(28.0))
        .min_w(px(if phone { 44.0 } else { 0.0 }))
        .px(px(12.0))
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(radius::SM))
        .bg(move || {
            if p1() {
                AdaptiveColor { light: theme().background.light, dark: theme().accent.dark }
            } else {
                AdaptiveColor::TRANSPARENT
            }
        })
        .style_when(move || p2() && !dark(), |s| {
            s.shadow([BoxShadow {
                offset_y: px(1.0),
                blur: px(2.0),
                color: nsh_core::color::hsla(0.0, 0.0, 0.0, 0.05).into(),
                ..BoxShadow::default()
            }])
        })
        .opacity(move || if d1() { 0.5_f32 } else { 1.0 })
        .role(AccessibilityRole::Button)
        .label(format!("{word} response {label}"))
        .accessibility_state(move || AccessibilityState { selected: p3(), ..AccessibilityState::default() })
        .focusable(true)
        .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .active(|s| s.scale(0.97))
        .tooltip(tooltip_bubble(tip))
        .on_press(move || {
            if !disabled() {
                on_press();
            }
        })
        .child(icon(glyph, 16.0, move || {
            if p4() && kind == Mark::Chosen {
                token("periwinkle")()
            } else if pressed() {
                ink()
            } else {
                muted_ink()
            }
        }))
        .child((!phone).then(|| {
            body(text(word))
                .number_of_lines(1)
                .text_color(move || if p5() { ink() } else { muted_ink() })
                .style_when(move || p6(), |s| s.font_weight(FontWeight::MEDIUM))
        }))
}

/// The folded thinking trace: "› Thinking trace · 1,204 tokens" in a ground well; open, the trace
/// in muted text under a hairline, at most 512 px tall (it scrolls inside).
fn thinking_trace(reasoning: String, size: String) -> impl IntoElement {
    let open = signal(false);
    let trigger = view()
        .min_h(px(36.0))
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .px(px(12.0))
        .rounded(px(radius::SM))
        .role(AccessibilityRole::Button)
        .label(format!("Thinking trace, {size}"))
        .accessibility_state(move || AccessibilityState { expanded: Some(open.get()), ..AccessibilityState::default() })
        .focusable(true)
        .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .on_press(move || open.update(|o| *o = !*o))
        .child(
            icon(icon::CHEVRON_RIGHT, 16.0, muted_ink).rotate(move || if open.get() { 90.0_f32.to_radians() } else { 0.0 }),
        )
        .child(body(text("Thinking trace · ")).text_color(muted_ink).child(body(text(size)).tabular_nums().text_color(muted_ink)));
    view().rounded(px(radius::SM)).bg(move || theme().background).child(trigger).child(when(move || open.get(), move || {
        let reasoning = reasoning.clone();
        scroll().max_h(px(512.0)).border_t_1().border_color(move || theme().border).child(
            view()
                .px(px(12.0))
                .pt(px(10.0))
                .pb(px(12.0))
                .child(model_text_with(move || reasoning.clone(), None, ModelTextStyle::small().muted_when(|| true))),
        )
    }))
}
