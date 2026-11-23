//! Prompt sets (frontend/src/pages/prompt-sets.tsx, components/prompts/prompt-set-table.tsx): every
//! set, newest first, from `GET /api/prompt-sets`. From 1024 px a table (name with its tile, the
//! source, status, prompts, runs, created); below it one stacked row per set with its facts on a
//! second line. A set that is importing makes the list read again every second. No sets: a card
//! that says where prompts come from, with the Import prompts button.

use std::rc::Rc;

use chrono::{DateTime, Utc};
use nsh_base::theme;
use nsh_core::prelude::*;

use super::source::source_parts;
use super::status::{IMPORTING_REFRESH, any_importing, set_status, set_status_small};
use crate::api::types::PromptSetOut as PromptSet;
use crate::data::{Load, call, now_utc};
use crate::format::{count, int, relative};
use crate::nav::{self, Route};
use crate::theme::text::{body, label, meta, mono};
use crate::theme::{muted_ink, radius};
use crate::ui::blocks::{bar, empty_block, error_block};
use crate::ui::button::{Button, Size, Variant, button};
use crate::ui::icon::{self, icon as glyph};
use crate::ui::page::{is_phone, page, window_width};
use crate::ui::plasma::plasma;
use crate::ui::table::{Column, cell, header, row};

const NAME: Column = Column::share("Name", 1.1);
const SOURCE: Column = Column::flex("Source");
const STATUS: Column = Column::fixed("Status", 112.0);
const PROMPTS: Column = Column::numeric("Prompts", 96.0);
const RUNS: Column = Column::numeric("Runs", 80.0);
const CREATED: Column = Column::numeric("Created", 112.0);
static COLUMNS: [Column; 6] = [NAME, SOURCE, STATUS, PROMPTS, RUNS, CREATED];

/// The width from which the sets show as a table (Tailwind's `lg`).
const TABLE_FROM: f32 = 1024.0;

/// The seed of a set's tile, so the same set always gets the same field.
pub fn set_seed(id: i64) -> String {
    format!("prompt-set-{id}")
}

/// The Prompt sets screen.
pub fn screen() -> impl IntoElement {
    let load = signal(Load::<Vec<PromptSet>>::Loading);
    let now = signal(now_utc());
    let reload = move || {
        call(
            |api| async move { api.prompt_sets().await },
            move |r| {
                now.set(now_utc());
                load.set(Load::from(r));
            },
        )
    };
    reload();
    // While a set imports its row changes every second; the task ends with the screen.
    let _ = set_interval(IMPORTING_REFRESH, move || {
        if load.with_untracked(|l| l.loaded().is_some_and(|sets| any_importing(sets))) {
            reload();
        }
    });

    let sets = move || load.with(|l| l.loaded().cloned().unwrap_or_default());
    let has_sets = move || load.with(|l| l.loaded().is_some_and(|v| !v.is_empty()));
    let import = || nav::push(Route::Import, None);

    let content = dynamic(move || -> AnyElement {
        let state = load.with(|l| match l {
            Load::Loading => 0,
            Load::Failed(_) => 1,
            Load::Loaded(v) if v.is_empty() => 2,
            Load::Loaded(_) => 3,
        });
        match state {
            0 => skeleton().into_element(),
            1 => {
                let message = load.with(|l| l.error().unwrap_or_default().to_owned());
                error_block(
                    format!("Couldn’t load prompt sets. {message}"),
                    Some(Rc::new(move || {
                        load.set(Load::Loading);
                        reload();
                    })),
                )
                .into_element()
            }
            2 => empty_page(import).into_element(),
            _ => set_list(sets, now).into_element(),
        }
    });

    let mut frame = page("Prompt sets");
    if !untracked(is_phone) {
        frame = frame.actions(move || {
            let empty = load.with(|l| l.loaded().is_some_and(Vec::is_empty));
            if empty { view().into_element() } else { import_button().on_press(import).into_element() }
        });
    }
    frame
        .bar_actions(move || {
            dynamic(move || {
                has_sets().then(|| {
                    Button::icon(icon::PLUS, "Import prompts").variant(Variant::Secondary).size(Size::Icon).round().on_press(import)
                })
            })
            .into_element()
        })
        .child(content)
}

/// "Import prompts" with the upload glyph.
pub fn import_button() -> Button {
    button("Import prompts").leading(icon::UPLOAD)
}

/// No sets: the tile, what to do, and the button.
fn empty_page(import: impl Fn() + Copy + 'static) -> impl IntoElement {
    empty_block(
        vec![
            plasma("prompt-set-empty", 8, 56.0, radius::MD, false).into_element(),
            body(text("No prompt sets yet. Import from Hugging Face, a file or pasted text."))
                .text_color(muted_ink)
                .mt(px(16.0))
                .into_element(),
        ],
        Some(import_button().on_press(import).into_element()),
    )
    .gap(px(16.0))
    .p(px(24.0))
    .rounded(px(radius::XL))
    .bg(move || theme().muted)
}

/// The sets, as a table or stacked rows by the window's width.
fn set_list(sets: impl Fn() -> Vec<PromptSet> + Copy + 'static, now: Signal<DateTime<Utc>>) -> impl IntoElement {
    let count_now = move || sets().len();
    let table = move || window_width() >= TABLE_FROM;
    when(table, move || {
        view().child(header(&COLUMNS)).child(each(sets, |s: &PromptSet| s.id, move |set, index| {
            let last = move || index.get() + 1 == count_now();
            dynamic(move || table_row(set.get(), now, last))
        }))
    })
    .otherwise(move || {
        view().child(each(sets, |s: &PromptSet| s.id, move |set, index| {
            let last = move || index.get() + 1 == count_now();
            dynamic(move || stacked_row(set.get(), now, last))
        }))
    })
}

/// The source of a set: the kind in words, or the dataset ID or file name in mono, then
/// "· main/train" in mono.
fn source_text(set: &PromptSet, small: bool) -> AnyElement {
    let p = source_parts(&set.source_kind, &set.source);
    let mut children: Vec<AnyElement> = Vec::new();
    match &p.id {
        Some(id) => children.push(mono(text(id.clone())).text_color(muted_ink).into_element()),
        None => children.push(text(p.kind).into_element()),
    }
    if let Some(whr) = &p.whr {
        children.push(mono(text(format!(" · {whr}"))).text_color(muted_ink).into_element());
    }
    let line = if small { meta(text("")) } else { body(text("")) };
    line.text_color(muted_ink).number_of_lines(1).children(children).into_element()
}

fn table_row(set: PromptSet, now: Signal<DateTime<Utc>>, last: impl Fn() -> bool + 'static) -> AnyElement {
    let (id, name) = (set.id, set.name.clone());
    let created = set.created_at;
    row(
        vec![
            cell(
                NAME,
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .min_w(px(0.0))
                    .py(px(4.0))
                    .child(plasma(&set_seed(set.id), 6, 28.0, radius::SM, false))
                    .child(label(text(set.name.clone())).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0))),
            )
            .into_element(),
            cell(SOURCE, source_text(&set, false)).into_element(),
            cell(STATUS, set_status(set.status)).into_element(),
            cell(PROMPTS, body(text(int(set.prompt_count))).tabular_nums()).into_element(),
            cell(RUNS, body(text(int(set.run_count))).tabular_nums()).into_element(),
            cell(CREATED, body(text(move || relative(created, now.get()))).text_color(muted_ink).tabular_nums().number_of_lines(1))
                .into_element(),
        ],
        last,
    )
    .cursor(Cursor::Pointer)
    .role(AccessibilityRole::Link)
    .label(set.name.clone())
    .on_press(move || nav::push(Route::PromptSet { id }, Some(name.clone())))
    .into_element()
}

/// Below 1024 px: the tile, the name (two lines), the source, then the facts; a chevron at the end.
fn stacked_row(set: PromptSet, now: Signal<DateTime<Utc>>, last: impl Fn() -> bool + 'static) -> AnyElement {
    let (id, name) = (set.id, set.name.clone());
    let created = set.created_at;
    let mut facts: Vec<AnyElement> = Vec::new();
    if set.status != crate::api::types::PromptSetStatus::Ready {
        facts.push(set_status_small(set.status).into_element());
        facts.push(meta(text(" · ")).into_element());
    }
    facts.push(
        meta(text(format!("{} · {}", count(set.prompt_count, "prompt", "prompts"), count(set.run_count, "run", "runs"))))
            .tabular_nums()
            .into_element(),
    );
    facts.push(meta(text(move || format!(" · {}", relative(created, now.get())))).tabular_nums().number_of_lines(1).into_element());
    view()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .min_h(px(56.0))
        .mx(px(-8.0))
        .px(px(8.0))
        .py(px(10.0))
        .rounded(px(radius::MD))
        .role(AccessibilityRole::Link)
        .label(set.name.clone())
        .cursor(Cursor::Pointer)
        .active(|s| s.bg(move || theme().accent))
        .on_press(move || nav::push(Route::PromptSet { id }, Some(name.clone())))
        .child(plasma(&set_seed(set.id), 6, 28.0, radius::SM, false))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .gap(px(2.0))
                .child(label(text(set.name.clone())).number_of_lines(2))
                .child(source_text(&set, true))
                .child(view().flex_row().flex_wrap().items_center().children(facts)),
        )
        .child(glyph(icon::CHEVRON_RIGHT, 16.0, muted_ink))
        .child(
            view()
                .absolute()
                .left(px(8.0))
                .right(px(8.0))
                .bottom(px(0.0))
                .h(px(1.0))
                .bg(move || theme().border)
                .opacity(move || if last() { 0.0_f32 } else { 1.0 }),
        )
        .into_element()
}

/// Loading rows at the real pitch: on a phone the bars stack, as the web UI's grid does.
fn skeleton() -> impl IntoElement {
    let table = window_width() >= TABLE_FROM;
    let rows = (0..4).map(move |_| {
        let bars: Vec<AnyElement> = [(192.0, 20.0), (208.0, 16.0), (56.0, 16.0), (48.0, 16.0), (32.0, 16.0), (64.0, 16.0)]
            .into_iter()
            .map(|(w, h)| bar(w, h).into_element())
            .collect();
        let v = view().border_b_1().border_color(move || theme().border);
        if table {
            v.flex_row().items_center().gap(px(24.0)).h(px(44.0)).px(px(12.0)).children(bars).into_element()
        } else {
            v.items_start().gap(px(6.0)).py(px(12.0)).children(bars).into_element()
        }
    });
    view().role(AccessibilityRole::Progressbar).label("Loading prompt sets").children(rows.collect::<Vec<_>>())
}
