//! Runs (frontend/src/pages/runs-list.tsx): every run, newest first, as a table from the API's
//! `GET /api/runs`, read again every 2 s. Columns: the name with the model, endpoint and prompt set
//! it does not already say; progress (the bar and "done of total", failures in red); seqs; status
//! (the dot and the word); created (relative). No runs: the header band keeps the list's frame and
//! a line says what will appear there.

use std::time::Duration;

use chrono::{DateTime, Utc};
use nsh_core::prelude::*;

use super::Screen;
use crate::api::Run;
use crate::format::{int, relative};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::{body, label, meta};
use crate::theme::{muted_ink, token};
use crate::ui::button::{Variant, button};
use crate::ui::icon;
use crate::ui::progress::progress_bar;
use crate::ui::shell::page;
use crate::ui::status::run_status;
use crate::ui::table::{Column, cell, header, row};

/// How often the list is read again.
const REFRESH: Duration = Duration::from_secs(2);

const NAME: Column = Column::flex("Name");
const PROGRESS: Column = Column::fixed("Progress", 280.0);
const SEQS: Column = Column::numeric("Seqs", 72.0);
const STATUS: Column = Column::fixed("Status", 128.0);
const CREATED: Column = Column::numeric("Created", 112.0);
static COLUMNS: [Column; 5] = [NAME, PROGRESS, SEQS, STATUS, CREATED];

/// The width of the progress bar in its cell (`w-20`).
const BAR_WIDTH: f32 = 80.0;

/// What the list has.
#[derive(Debug, Clone, PartialEq)]
enum Load {
    Loading,
    Loaded(Vec<Run>),
    Failed(String),
}

/// The Runs screen.
pub fn screen() -> impl IntoElement {
    let load = signal(Load::Loading);
    let now = signal(now_utc());
    let refresh = move || {
        spawn(async move {
            let result = match crate::api() {
                Ok(api) => api.runs().await.map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            now.set(now_utc());
            load.set(match result {
                Ok(runs) => Load::Loaded(runs),
                Err(message) => Load::Failed(message),
            });
        });
    };
    refresh();
    // Ends with the screen (the task belongs to it).
    let _ = set_interval(REFRESH, refresh);

    let loading = move || load.with(|l| matches!(l, Load::Loading));
    let failed = move || load.with(|l| matches!(l, Load::Failed(_)));
    let empty = move || load.with(|l| matches!(l, Load::Loaded(runs) if runs.is_empty()));
    let runs = move || load.with(|l| match l {
        Load::Loaded(runs) => runs.clone(),
        _ => Vec::new(),
    });

    let content = when(loading, || header(&COLUMNS)).otherwise(move || {
        when(failed, move || {
            let message = load.with(|l| match l {
                Load::Failed(m) => m.clone(),
                _ => String::new(),
            });
            view()
                .gap(px(12.0))
                .items_start()
                .child(body(text(format!("Couldn't load runs. {message}"))).text_color(token("negative-ink")))
                .child(button(Variant::Secondary, None, "Retry", refresh))
        })
        .otherwise(move || {
            when(empty, || {
                view().gap(px(16.0)).child(header(&COLUMNS)).child(
                    body(text("No runs yet. Each run you start shows here with its progress and status, newest first."))
                        .text_color(muted_ink)
                        .px(px(12.0)),
                )
            })
            .otherwise(move || {
                view()
                    .child(header(&COLUMNS))
                    .child(each(runs, |r: &Run| r.id, move |run, _| dynamic(move || run_row(run.get(), now))))
            })
        })
    });

    page(
        "Runs",
        Some(button(Variant::Primary, Some(icon::PLUS), "New run", || super::open(Screen::NewRun)).into_element()),
        content,
    )
}

/// One run's row.
fn run_row(run: Run, now: Signal<DateTime<Utc>>) -> impl IntoElement {
    let created = run.created_at;
    row(vec![
        cell(NAME, name_cell(&run)).into_element(),
        cell(PROGRESS, progress_cell(&run)).into_element(),
        cell(SEQS, body(text(int(run.concurrency))).tabular_nums()).into_element(),
        cell(STATUS, run_status(run.status)).into_element(),
        cell(CREATED, body(text(move || relative(created, now.get()))).text_color(muted_ink).tabular_nums().number_of_lines(1))
            .into_element(),
    ])
}

/// The name at 500, then in 12 px muted the model (mono), the endpoint and the prompt set, minus
/// what the name already says: a default name ("<model> on <set>") leaves only the endpoint.
fn name_cell(run: &Run) -> impl IntoElement {
    let lower = run.name.to_lowercase();
    let mut facts: Vec<(String, bool)> = Vec::new();
    if !lower.contains(&run.model.to_lowercase()) {
        facts.push((run.model.clone(), true));
    }
    facts.push((run.endpoint_name.clone(), false));
    if !lower.contains(&run.prompt_set_name.to_lowercase()) {
        facts.push((run.prompt_set_name.clone(), false));
    }
    let mut spans: Vec<AnyElement> = vec![meta(text("  ")).into_element()];
    for (i, (fact, mono)) in facts.into_iter().enumerate() {
        if i > 0 {
            spans.push(meta(text(" · ")).into_element());
        }
        let span = meta(text(fact));
        spans.push(if mono { span.font_family(ROBOTO_MONO.family).into_element() } else { span.into_element() });
    }
    label(text(run.name.clone())).number_of_lines(1).flex_1().min_w(px(0.0)).children(spans)
}

/// The bar, then "1,284 of 5,000" and the failures in red when there are any.
fn progress_cell(run: &Run) -> impl IntoElement {
    let failed = (run.failed > 0)
        .then(|| body(text(format!("   {} failed", int(run.failed)))).text_color(token("negative-ink")).into_element());
    view()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .child(progress_bar(run.done, run.failed, run.total, BAR_WIDTH))
        .child(
            body(text(int(run.done)))
                .tabular_nums()
                .number_of_lines(1)
                .child(body(text(format!(" of {}", int(run.total)))).text_color(muted_ink))
                .child(failed),
        )
}

/// The current time, from nsh's clock (the same on native targets and the web).
fn now_utc() -> DateTime<Utc> {
    DateTime::from_timestamp_millis(nsh_core::time::unix_ms() as i64).unwrap_or_default()
}
