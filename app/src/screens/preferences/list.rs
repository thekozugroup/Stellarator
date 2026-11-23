//! Preferences (frontend/src/pages/preferences-list.tsx): every preference set, newest first. From
//! 1024 px a table (the name, the prompt set with its tile, the models compared, the decided
//! prompts at their true ratio, when it was made); below that one stacked row per set, as Runs.
//! With none, the empty card offers New preference set (or New run when there are no runs).

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::new_set::new_set_dialog;
use crate::api::types::{PreferenceSetOut as Set, RunOut};
use crate::data::{Load, call, now_utc};
use crate::format::{int, middle, relative};
use crate::nav::{self, Route};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::{body, label, meta, mono};
use crate::theme::{muted_ink, radius};
use crate::ui::blocks::{bar, empty_block, error_block};
use crate::ui::button::{Button, Size, Variant, button};
use crate::ui::icon;
use crate::ui::page::{is_phone, page, window_width};
use crate::ui::plasma::plasma;
use crate::ui::progress::{progress_bar, progress_bar_full};
use crate::ui::table::row;

/// The window width from which the sets show as a table (Tailwind's `lg`).
const TABLE_FROM: f32 = 1024.0;

/// The table's columns: title, share of the width (`table-fixed` percentages), right-aligned.
const COLUMNS: [(&str, f32, bool); 5] =
    [("Name", 28.0, false), ("Prompt set", 22.0, false), ("Runs", 22.0, false), ("Decided", 16.0, false), ("Created", 12.0, true)];

/// The Preferences screen.
pub fn screen() -> impl IntoElement {
    let sets = signal(Load::<Vec<Set>>::Loading);
    let runs = signal(Load::<Vec<RunOut>>::Loading);
    let reload = move || call(|api| async move { api.preference_sets().await }, move |r| sets.set(Load::from(r)));
    reload();
    call(|api| async move { api.runs().await }, move |r| runs.set(Load::from(r)));

    let open = signal(false);
    let empty = move || sets.with(|l| l.loaded().is_some_and(Vec::is_empty));
    let no_runs = move || runs.with(|l| l.loaded().is_some_and(Vec::is_empty));

    let content = dynamic(move || -> AnyElement {
        let state = sets.with(|l| match l {
            Load::Loading => 0,
            Load::Failed(_) => 1,
            Load::Loaded(v) if v.is_empty() => 2,
            Load::Loaded(_) => 3,
        });
        match state {
            0 => loading_rows().into_element(),
            1 => {
                let message = sets.with(|l| l.error().unwrap_or_default().to_owned());
                error_block(
                    format!("Couldn't load preference sets. {message}"),
                    Some(Rc::new(move || {
                        sets.set(Load::Loading);
                        reload();
                    })),
                )
                .into_element()
            }
            2 => empty_card(no_runs, move || open.set(true)).into_element(),
            _ => sets_list(move || sets.with(|l| l.loaded().cloned().unwrap_or_default())).into_element(),
        }
    });

    let mut frame = page("Preferences");
    // The toolbar's button while loading and with sets (the empty card has its own); on a phone
    // the "+" in the bar instead.
    if !untracked(is_phone) {
        frame = frame.actions(move || {
            if empty() {
                return view().into_element();
            }
            button("New preference set").leading(icon::PLUS).on_press(move || open.set(true)).into_element()
        });
    }
    frame
        .bar_actions(move || {
            dynamic(move || {
                (!empty()).then(|| {
                    Button::icon(icon::PLUS, "New preference set")
                        .variant(Variant::Secondary)
                        .size(Size::Icon)
                        .round()
                        .on_press(move || open.set(true))
                })
            })
            .into_element()
        })
        .child(content)
        .child(new_set_dialog(open, Rc::new(move |_| reload())))
}

/// No sets: the pixel field, one sentence and the action that fills the list.
fn empty_card(no_runs: impl Fn() -> bool + Copy + 'static, new_set: impl Fn() + Copy + 'static) -> impl IntoElement {
    dynamic(move || {
        let none = no_runs();
        let sentence = if none {
            "No runs yet. Start a run with 2 or more samples per prompt, then compare its responses here."
        } else {
            "No preference sets yet. Pick runs on one prompt set and choose the better response to each prompt."
        };
        let action = if none {
            button("New run").on_press(|| nav::push(Route::NewRun, None))
        } else {
            button("New preference set").on_press(new_set)
        };
        empty_block(
            vec![
                plasma("preferences-empty", 8, 56.0, radius::MD, false).into_element(),
                body(text(sentence)).text_color(muted_ink).mt(px(16.0)).into_element(),
            ],
            Some(action.into_element()),
        )
        .gap(px(16.0))
        .p(px(24.0))
        .rounded(px(radius::XL))
        .bg(move || theme().muted)
    })
}

/// The sets as a table from 1024 px, as stacked rows below.
fn sets_list(sets: impl Fn() -> Vec<Set> + Copy + 'static) -> impl IntoElement {
    let count = move || sets().len();
    when(move || window_width() >= TABLE_FROM, move || {
        view().child(table_header()).child(each(sets, |s: &Set| s.id, move |set, index| {
            let last = move || index.get() + 1 == count();
            dynamic(move || table_row(set.get(), last))
        }))
    })
    .otherwise(move || {
        view().child(each(sets, |s: &Set| s.id, move |set, index| {
            let last = move || index.get() + 1 == count();
            dynamic(move || stacked_row(set.get(), last))
        }))
    })
}

/// A table cell taking `share` of the row's width.
fn share_cell(share: f32, end: bool, content: impl IntoElement) -> View {
    let c = view()
        .basis(px(0.0))
        .flex_grow(share)
        .min_w(px(0.0))
        .flex_row()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .overflow_hidden();
    let c = if end { c.justify_end() } else { c };
    c.child(content)
}

/// The header band: 32 px of `muted` with 10 px corners and 12 px muted labels.
fn table_header() -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .h(px(32.0))
        .rounded(px(radius::MD))
        .bg(move || theme().muted)
        .children(
            COLUMNS
                .iter()
                .map(|&(title, share, end)| share_cell(share, end, meta(text(title)).number_of_lines(1)).py(px(0.0)).into_element())
                .collect::<Vec<_>>(),
        )
}

/// A prompt set's name beside its pixel-field tile (20 px, 5 px corners), muted: 13 px, or 12 px
/// when `small`.
fn prompt_set_name(set: &Set, small: bool) -> impl IntoElement {
    let name = text(set.prompt_set_name.clone());
    let name = if small { meta(name) } else { body(name).text_color(muted_ink) };
    view()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .min_w(px(0.0))
        .child(plasma(&format!("prompt-set-{}", set.prompt_set_id), 5, 20.0, radius::XS, false))
        .child(name.number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)))
}

/// Decided prompts at their true ratio: in the table the bar then "2 of 6"; stacked, "2 of 6
/// decided" over a full-width bar.
fn progress(set: &Set, stacked: bool) -> AnyElement {
    if set.prompt_count == 0 {
        return body(text("Nothing to compare yet"))
            .text_color(muted_ink)
            .tooltip(crate::ui::tooltip::tooltip_bubble("No prompt has two finished responses yet"))
            .into_element();
    }
    let figures = body(text(int(set.decided)))
        .tabular_nums()
        .number_of_lines(1)
        .child(body(text(format!(" of {}{}", int(set.prompt_count), if stacked { " decided" } else { "" }))).text_color(muted_ink));
    let label = format!("{} of {} prompts decided", int(set.decided), int(set.prompt_count));
    if stacked {
        view()
            .gap(px(6.0))
            .child(figures)
            .child(view().role(AccessibilityRole::Progressbar).label(label).child(progress_bar_full(set.decided, 0, set.prompt_count)))
            .into_element()
    } else {
        view()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(view().role(AccessibilityRole::Progressbar).label(label).child(progress_bar(set.decided, 0, set.prompt_count, 80.0)))
            .child(figures)
            .into_element()
    }
}

/// One set's row in the table; a press opens its review.
fn table_row(set: Set, last: impl Fn() -> bool + 'static) -> impl IntoElement {
    let created = set.created_at;
    let (id, name) = (set.id, set.name.clone());
    let (ps_id, ps_name) = (set.prompt_set_id, set.prompt_set_name.clone());
    let runs: AnyElement = if set.runs.is_empty() {
        body(text("No runs left")).text_color(muted_ink).into_element()
    } else {
        view()
            .gap(px(2.0))
            .min_w(px(0.0))
            .children(
                set.runs
                    .iter()
                    .map(|r| {
                        mono(text(middle(&r.model, 44)))
                            .text_color(muted_ink)
                            .number_of_lines(1)
                            .tooltip(crate::ui::tooltip::tooltip_bubble(format!("{} ({})", r.name, r.model)))
                            .into_element()
                    })
                    .collect::<Vec<_>>(),
            )
            .into_element()
    };
    let prompt_set = view()
        .flex_row()
        .max_w(nsh_core::geometry::relative(1.0))
        .mx(px(-4.0))
        .px(px(4.0))
        .rounded(px(radius::XS))
        .role(AccessibilityRole::Link)
        .label(set.prompt_set_name.clone())
        .cursor(Cursor::Pointer)
        .focusable(true)
        .on_press(move || nav::push(Route::PromptSet { id: ps_id }, Some(ps_name.clone())))
        .child(prompt_set_name(&set, false));
    row(
        vec![
            share_cell(28.0, false, label(text(set.name.clone())).number_of_lines(2)).into_element(),
            share_cell(22.0, false, prompt_set).into_element(),
            share_cell(22.0, false, runs).into_element(),
            share_cell(16.0, false, progress(&set, false)).into_element(),
            share_cell(
                12.0,
                true,
                body(text(move || relative(created, now_utc())))
                    .text_color(muted_ink)
                    .number_of_lines(1)
                    .tooltip(crate::ui::tooltip::tooltip_bubble(crate::format::date_time(created))),
            )
            .into_element(),
        ],
        last,
    )
    .cursor(Cursor::Pointer)
    .role(AccessibilityRole::Link)
    .label(set.name.clone())
    .on_press(move || nav::push(Route::PreferenceSet { id }, Some(name.clone())))
}

/// Below 1024 px (as Runs): the name; then the prompt set, the models compared and (from 640 px)
/// the date; then the progress at full width. The whole row opens the set.
fn stacked_row(set: Set, last: impl Fn() -> bool + 'static) -> impl IntoElement {
    let created = set.created_at;
    let (id, name) = (set.id, set.name.clone());
    let models = set.runs.iter().map(|r| r.model.as_str()).collect::<Vec<_>>().join(" vs ");
    let facts = view()
        .mt(px(2.0))
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .min_w(px(0.0))
        .child(view().max_w(nsh_core::geometry::relative(0.6)).flex_shrink(1.0).min_w(px(0.0)).child(prompt_set_name(&set, true)))
        .child((!set.runs.is_empty()).then(|| meta(text("·")).accessibility_elements_hidden(true)))
        .child((!set.runs.is_empty()).then(|| {
            meta(text(models)).font_family(ROBOTO_MONO.family).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0))
        }))
        .child((window_width() >= 640.0).then(|| meta(text(move || format!("· {}", relative(created, now_utc())))).flex_none()));
    view()
        .py(px(12.0))
        .cursor(Cursor::Pointer)
        .role(AccessibilityRole::Link)
        .label(set.name.clone())
        .focusable(true)
        .on_press(move || nav::push(Route::PreferenceSet { id }, Some(name.clone())))
        .child(label(text(set.name.clone())))
        .child(facts)
        .child(view().mt(px(10.0)).child(progress(&set, true)))
        .child(
            view()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .h(px(1.0))
                .bg(move || theme().border)
                .opacity(move || if last() { 0.0_f32 } else { 1.0 }),
        )
}

/// Loading: the header band and three rows at the table's pitch (56 px), or three stacked rows.
fn loading_rows() -> impl IntoElement {
    let table = window_width() >= TABLE_FROM;
    let rows = (0..3).map(move |n| {
        let bars: [AnyElement; 5] = [
            bar(160.0, 16.0).into_element(),
            bar(112.0, 16.0).into_element(),
            bar(144.0, 16.0).into_element(),
            view().w(px(112.0)).h(px(6.0)).rounded(px(3.0)).bg(move || theme().secondary).into_element(),
            bar(56.0, 16.0).into_element(),
        ];
        let line = view()
            .border_b_1()
            .border_color(move || if n == 2 { AdaptiveColor::TRANSPARENT } else { theme().border });
        if table {
            line.flex_row()
                .items_center()
                .h(px(56.0))
                .px(px(12.0))
                .children(
                    bars.into_iter()
                        .zip(COLUMNS)
                        .map(|(b, (_, share, end))| {
                            let c = view().basis(px(0.0)).flex_grow(share).flex_row();
                            (if end { c.justify_end() } else { c }).child(b).into_element()
                        })
                        .collect::<Vec<_>>(),
                )
                .into_element()
        } else {
            line.py(px(12.0)).gap(px(8.0)).children(bars.into_iter().collect::<Vec<_>>()).into_element()
        }
    });
    view()
        .role(AccessibilityRole::Progressbar)
        .label("Loading preference sets…")
        .child(table.then(|| view().h(px(32.0)).rounded(px(radius::MD)).bg(move || theme().muted)))
        .children(rows.collect::<Vec<_>>())
}
