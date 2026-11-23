//! The preview (frontend/src/components/import/preview-panel.tsx): the first prompts of the source
//! as they will be stored, in a card with "First 20 of ~7,473 prompts" under its title and
//! "Updating…" while it is read again. The card also shows the server's warnings, the responses
//! left out (Advanced), and the columns; with no source, a failure or no prompts it says so.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::helpers::preview_summary;
use super::state::State;
use crate::api::types::ImportPreviewOut;
use crate::format::int;
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::{body, meta};
use crate::theme::{muted_ink, radius, token};
use crate::ui::blocks::{bar, empty_block, error_block};
use crate::ui::button::{Size, Variant, button};
use crate::ui::page::panel_tight;

/// A prompt clamps at `max-h-[9.6em]` of 13 px text: 124.8 px.
const CLAMP: f32 = 124.8;

/// The card, made again whenever what it shows changes.
pub fn preview_panel(st: State, refetch: impl Fn() + Copy + 'static, error: Signal<Option<String>>, empty_hint: Signal<Option<String>>) -> impl IntoElement {
    dynamic(move || {
        let preview = st.preview.get();
        let has_source = st.has_source.get();
        let loading = has_source && preview.is_none() && st.fetching.get();
        let updating = has_source && preview.is_some() && (st.fetching.get() || st.settling.get());
        let err = error.get();
        let show_dropped = !st.simple();
        let description = preview.as_ref().map(|p| preview_summary(p.prompts.len(), p.estimated_prompts));
        let actions = updating.then(|| meta(text("Updating…")).text_color(muted_ink).into_element());
        let content = content_of(preview.as_ref(), has_source, loading, updating, err, refetch, empty_hint.get(), show_dropped);
        panel_tight(Some("Preview".into()), description.map(SharedString::from), actions, content).into_element()
    })
}

#[allow(clippy::too_many_arguments, reason = "the card's one state, read once per draw")]
fn content_of(
    preview: Option<&ImportPreviewOut>,
    has_source: bool,
    loading: bool,
    updating: bool,
    error: Option<String>,
    refetch: impl Fn() + Copy + 'static,
    empty_hint: Option<String>,
    show_dropped: bool,
) -> AnyElement {
    let mut parts: Vec<AnyElement> = Vec::new();
    if let Some(p) = preview {
        if !p.warnings.is_empty() {
            parts.push(
                view()
                    .mb(px(8.0))
                    .gap(px(4.0))
                    .rounded(px(radius::SM))
                    .bg(token("coral-tint"))
                    .px(px(12.0))
                    .py(px(10.0))
                    .children(p.warnings.iter().map(|w| body(text(w.clone())).text_color(token("coral-ink")).into_element()).collect::<Vec<_>>())
                    .into_element(),
            );
        }
        if show_dropped && !p.dropped.is_empty() {
            parts.push(
                meta(text(format!("Responses left out: {}.", p.dropped.join(", ")))).text_color(muted_ink).px(px(8.0)).mb(px(8.0)).into_element(),
            );
        }
        if !p.columns.is_empty() {
            let names: Vec<AnyElement> = p
                .columns
                .iter()
                .enumerate()
                .flat_map(|(i, c)| {
                    let mut v: Vec<AnyElement> = Vec::new();
                    if i > 0 {
                        v.push(text(", ").into_element());
                    }
                    v.push(text(c.name.clone()).font_family(ROBOTO_MONO.family).into_element());
                    v
                })
                .collect();
            parts.push(meta(text("Columns: ")).text_color(muted_ink).px(px(8.0)).mb(px(8.0)).children(names).into_element());
        }
    }
    let body_el: AnyElement = match (error, preview) {
        (Some(message), None) => {
            let retry: Option<Rc<dyn Fn()>> = has_source.then(|| Rc::new(refetch) as Rc<dyn Fn()>);
            view().child(error_block(message, retry)).into_element()
        }
        (_, Some(p)) if p.prompts.is_empty() => view()
            .px(px(8.0))
            .child(empty_block(
                vec![body(text("No prompts came out of this source with these settings. Try another split or column.")).text_color(muted_ink).into_element()],
                None,
            ))
            .into_element(),
        (_, Some(p)) => view()
            .gap(px(8.0))
            .opacity(if updating { 0.7_f32 } else { 1.0 })
            .children(p.prompts.iter().enumerate().map(|(i, m)| prompt(i + 1, m)).collect::<Vec<_>>())
            .into_element(),
        _ if loading => view()
            .gap(px(8.0))
            .role(AccessibilityRole::Progressbar)
            .label("Loading preview")
            .children((0..5).map(|_| skeleton_row()).collect::<Vec<_>>())
            .into_element(),
        _ => view()
            .px(px(8.0))
            .child(empty_block(
                vec![body(text(empty_hint.unwrap_or_else(|| "No source yet.".to_owned()))).text_color(muted_ink).into_element()],
                None,
            ))
            .into_element(),
    };
    parts.push(body_el);
    view().children(parts).into_element()
}

/// A row's frame: the number in 40 px, the content beside it, on the ground.
fn row_frame(number: impl IntoElement, content: impl IntoElement) -> AnyElement {
    view()
        .flex_row()
        .gap(px(8.0))
        .rounded(px(radius::SM))
        .bg(move || theme().background)
        .px(px(12.0))
        .py(px(12.0))
        .child(view().w(px(40.0)).flex_none().pt(px(2.0)).child(number))
        .child(view().flex_1().min_w(px(0.0)).child(content))
        .into_element()
}

fn skeleton_row() -> AnyElement {
    row_frame(bar(24.0, 20.0), view().gap(px(8.0)).child(bar(360.0, 16.0)).child(bar(288.0, 16.0)).child(bar(144.0, 16.0)))
}

fn role_of(m: &serde_json::Value) -> &str {
    m.get("role").and_then(|r| r.as_str()).unwrap_or_default()
}

fn content_of_message(m: &serde_json::Value) -> String {
    m.get("content").and_then(|c| c.as_str()).unwrap_or_default().to_owned()
}

/// One prompt: its number, its system message (two lines at most), its messages as plain text
/// clamped to about nine lines, and Show all when there is more.
fn prompt(n: usize, messages: &[serde_json::Value]) -> AnyElement {
    let open = signal(false);
    let overflowing = signal(false);
    let system = messages.iter().find(|m| role_of(m) == "system").cloned();
    let rest: Vec<serde_json::Value> = messages.iter().filter(|m| Some(*m) != system.as_ref()).cloned().collect();
    let multi = rest.len() > 1;

    let system_line = system.map(|s| {
        meta(text(""))
            .text_color(muted_ink)
            .number_of_lines(2)
            .child(text("System ").font_weight(FontWeight::MEDIUM))
            .child(text(content_of_message(&s)))
    });
    let texts: Vec<(Option<String>, String)> =
        rest.iter().map(|m| (multi.then(|| role_of(m).to_owned()), content_of_message(m))).collect();
    let body_block = dynamic(move || {
        let mut col = view().gap(px(6.0));
        for (role, content) in &texts {
            let mut line = body(text("")).line_height(px(20.8)).selectable(true);
            if let Some(role) = role {
                line = line.child(meta(text(format!("{role}  "))).font_weight(FontWeight::MEDIUM).text_color(muted_ink));
            }
            col = col.child(line.child(text(content.clone())));
        }
        if open.get() {
            col.into_element()
        } else {
            view()
                .max_h(px(CLAMP))
                .overflow_hidden()
                .child(view().flex_shrink(0.0).on_layout(move |b: &Bounds<Points>| overflowing.set(b.size.height.0 > CLAMP + 1.0)).child(col))
                .into_element()
        }
    });
    let toggle = dynamic(move || {
        (overflowing.get() || open.get()).then(|| {
            view().self_start().ml(px(-8.0)).child(
                button(if open.get() { "Show less" } else { "Show all" })
                    .variant(Variant::Ghost)
                    .size(Size::Xs)
                    .muted()
                    .on_press(move || open.update(|o| *o = !*o)),
            )
        })
    });
    let number = body(text(format!("#{}", int(n as i64)))).font_family(ROBOTO_MONO.family).tabular_nums().text_color(muted_ink);
    row_frame(number, view().gap(px(6.0)).child(system_line).child(body_block).child(toggle))
}
