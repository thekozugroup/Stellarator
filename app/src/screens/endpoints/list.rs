//! The endpoint list (frontend/src/components/endpoints/endpoint-list.tsx, base-url.tsx). From
//! 1024 px a table: the pixel-field tile, the name in ink and the base URL in mono under it; the
//! status as a dot and a word; the models in mono from 1280 px; when it was checked; seqs; runs;
//! Edit and the "…" menu. Below 1024 px one stacked row per endpoint with its facts on a second
//! line. A press on a row opens the details sheet.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::status::{checked_at_text, endpoint_status, endpoint_status_small};
use crate::api::types::EndpointOut as Endpoint;
use crate::data::now_utc;
use crate::format::{DASH, count, int, middle};
use crate::theme::text::{body, label, meta, mono};
use crate::theme::{muted_ink, radius};
use crate::ui::blocks::bar;
use crate::ui::button::{Size, Variant, button};
use crate::ui::icon;
use crate::ui::page::window_width;
use crate::ui::plasma::plasma;
use crate::ui::select::{MenuEntry, menu_button, menu_destructive, menu_entry};
use crate::ui::table::{Column, cell, header, row};
use crate::ui::tooltip::copy_button;

/// What a row's controls do.
#[derive(Clone)]
pub struct Actions {
    pub open: Rc<dyn Fn(Endpoint)>,
    pub edit: Rc<dyn Fn(Endpoint)>,
    pub delete: Rc<dyn Fn(Endpoint)>,
    pub check: Rc<dyn Fn(Endpoint)>,
    /// The endpoints being checked now.
    pub checking: Signal<Vec<i64>>,
}

/// The seed of an endpoint's tile, so the same endpoint always gets the same field.
pub fn endpoint_seed(endpoint: &Endpoint) -> String {
    format!("endpoint-{}", endpoint.id)
}

const ENDPOINT: Column = Column::flex("Endpoint");
const STATUS: Column = Column::fixed("Status", 160.0);
const MODELS: Column = Column::fixed("Models", 176.0);
const CHECKED: Column = Column::fixed("Checked", 112.0);
const SEQS: Column = Column::numeric("Seqs", 64.0);
const RUNS: Column = Column::numeric("Runs", 64.0);
const ACTIONS: Column = Column::fixed("", 96.0);
static WIDE: [Column; 7] = [ENDPOINT, STATUS, MODELS, CHECKED, SEQS, RUNS, ACTIONS];
static NARROW: [Column; 6] = [ENDPOINT, STATUS, CHECKED, SEQS, RUNS, ACTIONS];

/// The width from which the list is a table (Tailwind's `lg`).
const TABLE_FROM: f32 = 1024.0;
/// The width from which the table shows the models (Tailwind's `xl`).
const MODELS_FROM: f32 = 1280.0;

/// The list of `endpoints` (reactive), as a table or stacked rows by the window's width.
pub fn endpoint_list(endpoints: impl Fn() -> Vec<Endpoint> + Copy + 'static, actions: Actions) -> impl IntoElement {
    let count_now = move || endpoints().len();
    let table = move || window_width() >= TABLE_FROM;
    let a1 = actions.clone();
    when(table, move || {
        let actions = a1.clone();
        let models = move || window_width() >= MODELS_FROM;
        view()
            .child(dynamic(move || if models() { header(&WIDE).into_element() } else { header(&NARROW).into_element() }))
            .child(each(endpoints, |e: &Endpoint| e.id, move |ep, index| {
                let actions = actions.clone();
                let last = move || index.get() + 1 == count_now();
                dynamic(move || table_row(ep.get(), models(), actions.clone(), last))
            }))
    })
    .otherwise(move || {
        let actions = actions.clone();
        view().child(each(endpoints, |e: &Endpoint| e.id, move |ep, index| {
            let actions = actions.clone();
            let last = move || index.get() + 1 == count_now();
            dynamic(move || stacked_row(ep.get(), actions.clone(), last))
        }))
    })
}

/// A base URL in mono, shortened in the middle (46 characters, 30 on a phone), and its copy
/// button, which overlaps the lines above and below so it does not make a row taller.
pub fn base_url(url: &str) -> impl IntoElement {
    let short = middle(url, if window_width() >= 640.0 { 46 } else { 30 });
    view()
        .flex_row()
        .items_center()
        .gap(px(2.0))
        .min_w(px(0.0))
        .child(mono(text(short)).text_color(muted_ink).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)))
        .child(view().my(px(-6.0)).child(copy_button(url.to_owned(), "Copy base URL")))
}

/// The models: the first id in mono, and "+N" for the rest; the dash when there are none.
fn models_text(endpoint: &Endpoint) -> AnyElement {
    match endpoint.models.first() {
        None => body(text(DASH)).text_color(muted_ink).into_element(),
        Some(first) => view()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .min_w(px(0.0))
            .child(mono(text(first.clone())).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)))
            .child((endpoint.models.len() > 1).then(|| meta(text(format!("+{}", int(endpoint.models.len() as i64 - 1)))).tabular_nums()))
            .into_element(),
    }
}

/// Edit (from 1024 px) and the "…" menu: Edit… (below 1024 px), Check now, Delete….
fn row_actions(endpoint: &Endpoint, actions: Actions, with_edit: bool) -> impl IntoElement {
    let id = endpoint.id;
    let mut entries: Vec<MenuEntry> = Vec::new();
    if !with_edit {
        let (e, a) = (endpoint.clone(), actions.edit.clone());
        entries.push(menu_entry(Some(icon::PENCIL), "Edit…", move || a(e.clone())));
    }
    let (e, a) = (endpoint.clone(), actions.check.clone());
    let checking = actions.checking;
    entries.push(menu_entry(Some(icon::REFRESH), "Check now", move || a(e.clone())).disabled(move || checking.with(|c| c.contains(&id))));
    entries.push(MenuEntry::Separator);
    let (e, a) = (endpoint.clone(), actions.delete.clone());
    entries.push(menu_destructive(Some(icon::TRASH), "Delete…", move || a(e.clone())));
    let edit = with_edit.then(|| {
        let (e, a) = (endpoint.clone(), actions.edit.clone());
        button("Edit")
            .variant(Variant::Ghost)
            .size(Size::Sm)
            .accessibility_label(format!("Edit {}", endpoint.name))
            .on_press(move || a(e.clone()))
    });
    view()
        .flex_row()
        .items_center()
        .justify_end()
        .gap(px(4.0))
        .child(edit)
        .child(menu_button(format!("More actions for {}", endpoint.name), entries, Some(176.0)))
}

/// The name (500, two lines at most) over the base URL.
fn name_block(endpoint: &Endpoint) -> impl IntoElement {
    view()
        .flex_1()
        .min_w(px(0.0))
        .gap(px(2.0))
        .child(label(text(endpoint.name.clone())).number_of_lines(2))
        .child(base_url(&endpoint.base_url))
}

fn table_row(endpoint: Endpoint, models: bool, actions: Actions, last: impl Fn() -> bool + 'static) -> AnyElement {
    let now = now_utc();
    let mut cells = vec![
        cell(
            ENDPOINT,
            view()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .min_w(px(0.0))
                .py(px(4.0))
                .child(plasma(&endpoint_seed(&endpoint), 6, 28.0, radius::SM, false))
                .child(name_block(&endpoint)),
        )
        .into_element(),
        cell(STATUS, view().py(px(4.0)).child(endpoint_status(&endpoint))).into_element(),
    ];
    if models {
        cells.push(cell(MODELS, models_text(&endpoint)).into_element());
    }
    cells.push(cell(CHECKED, checked_at_text(&endpoint, now)).into_element());
    cells.push(cell(SEQS, body(text(int(endpoint.default_concurrency))).tabular_nums()).into_element());
    cells.push(cell(RUNS, body(text(int(endpoint.run_count))).tabular_nums()).into_element());
    cells.push(cell(ACTIONS, view().flex_1().child(row_actions(&endpoint, actions.clone(), true))).into_element());
    let open = actions.open.clone();
    row(cells, last)
        .cursor(Cursor::Pointer)
        .role(AccessibilityRole::Button)
        .label(endpoint.name.clone())
        .on_press(move || open(endpoint.clone()))
        .into_element()
}

/// Below 1024 px: the tile, the name, the base URL and a line of facts; the menu at the end.
fn stacked_row(endpoint: Endpoint, actions: Actions, last: impl Fn() -> bool + 'static) -> AnyElement {
    let mut facts: Vec<AnyElement> = vec![endpoint_status_small(&endpoint).into_element()];
    let mut more = Vec::new();
    if !endpoint.models.is_empty() {
        more.push(count(endpoint.models.len() as i64, "model", "models"));
    }
    more.push(format!("{} seqs", int(endpoint.default_concurrency)));
    more.push(count(endpoint.run_count, "run", "runs"));
    facts.push(meta(text(format!(" · {}", more.join(" · ")))).tabular_nums().number_of_lines(1).into_element());
    let open = actions.open.clone();
    let e = endpoint.clone();
    view()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .min_h(px(56.0))
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .mx(px(-8.0))
                .px(px(8.0))
                .py(px(10.0))
                .rounded(px(radius::MD))
                .role(AccessibilityRole::Button)
                .label(endpoint.name.clone())
                .cursor(Cursor::Pointer)
                .active(|s| s.bg(move || theme().accent))
                .on_press(move || open(e.clone()))
                .child(plasma(&endpoint_seed(&endpoint), 6, 28.0, radius::SM, false))
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(2.0))
                        .child(label(text(endpoint.name.clone())).number_of_lines(2))
                        .child(mono(text(middle(&endpoint.base_url, 40))).text_color(muted_ink).number_of_lines(1))
                        .child(view().flex_row().items_center().min_w(px(0.0)).children(facts)),
                ),
        )
        .child(row_actions(&endpoint, actions, false))
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
        .into_element()
}

/// Loading rows at the real pitch and column positions.
pub fn list_skeleton() -> impl IntoElement {
    let table = window_width() >= TABLE_FROM;
    let rows = (0..3).map(move |_| {
        view()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .h(px(if table { 56.0 } else { 64.0 }))
            .px(px(if table { 12.0 } else { 0.0 }))
            .border_b_1()
            .border_color(move || theme().border)
            .child(view().w(px(28.0)).h(px(28.0)).rounded(px(radius::SM)).bg(move || theme().secondary))
            .child(view().flex_1().gap(px(6.0)).child(bar(160.0, 16.0)).child(bar(224.0, 12.0)))
            .child(table.then(|| bar(80.0, 16.0)))
            .child(table.then(|| bar(64.0, 16.0)))
            .into_element()
    });
    view()
        .role(AccessibilityRole::Progressbar)
        .label("Loading endpoints")
        .child(table.then(|| view().h(px(32.0)).rounded(px(radius::MD)).bg(move || theme().muted)))
        .children(rows.collect::<Vec<_>>())
}
