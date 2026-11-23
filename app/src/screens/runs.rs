//! Runs (frontend/src/pages/runs-list.tsx): every run, newest first, as a table from the API's
//! `GET /api/runs`, read again every 2 s. Columns: the name with the model, endpoint and prompt set
//! it does not already say; progress (the bar and "done of total", failures in red); seqs; status
//! (the dot and the word); created (relative). No runs: the header band keeps the list's frame and
//! a line says what will appear there.

use std::time::Duration;

use chrono::{DateTime, Utc};
use nsh_core::prelude::*;

use std::rc::Rc;

use crate::api::types::{EndpointOut, PromptSetOut as PromptSet, RunOut as Run};
use crate::data::{Load as Loaded, call};
use crate::format::{int, relative};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::{body, label, meta, title};
use crate::theme::{muted_ink, radius, token};
use crate::nav::{self, Destination, Route};
use crate::screens::new_run::{self, Prefill};
use crate::ui::blocks::{bar, error_block};
use crate::ui::button::{Button, Size, Variant, button};
use crate::ui::icon;
use crate::ui::progress::{progress_bar, progress_bar_full};
use crate::ui::icon::icon as glyph;
use crate::ui::page::{is_phone, page, window_width};
use crate::ui::plasma::plasma;
use crate::ui::status::run_status_pill as run_status;
use crate::ui::table::{Column, cell, header, row};

/// How often the list is read again.
const REFRESH: Duration = Duration::from_secs(2);

const NAME: Column = Column::flex("Name");
const PROGRESS: Column = Column::fixed("Progress", 280.0);
const SEQS: Column = Column::numeric("Seqs", 72.0);
const STATUS: Column = Column::fixed("Status", 128.0);
const CREATED: Column = Column::numeric("Created", 112.0);
static COLUMNS: [Column; 5] = [NAME, PROGRESS, SEQS, STATUS, CREATED];

/// The window width from which runs show as a table (Tailwind's `lg`).
const TABLE_FROM: f32 = 1024.0;

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

    let wide = move || window().width.0 >= TABLE_FROM;
    let loading = move || load.with(|l| matches!(l, Load::Loading));
    let failed = move || load.with(|l| matches!(l, Load::Failed(_)));
    let empty = move || load.with(|l| matches!(l, Load::Loaded(runs) if runs.is_empty()));
    let count = move || load.with(|l| match l {
        Load::Loaded(runs) => runs.len(),
        _ => 0,
    });
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
            error_block(format!("Couldn't load runs. {message}"), Some(Rc::new(refresh)))
        })
        .otherwise(move || {
            when(empty, runs_empty)
            .otherwise(move || {
                // A table from 1024 px; one stacked row per run below that (as the web UI).
                when(wide, move || {
                    view()
                        .child(header(&COLUMNS))
                        .child(each(runs, |r: &Run| r.id, move |run, index| {
                            let last = move || index.get() + 1 == count();
                            dynamic(move || run_row(run.get(), now, last))
                        }))
                })
                .otherwise(move || {
                    view().child(each(runs, |r: &Run| r.id, move |run, _| dynamic(move || stacked_row(run.get(), now))))
                })
            })
        })
    });

    // The toolbar's push button; on a phone the "+" in the bar instead.
    let mut frame = page("Runs");
    if !untracked(is_phone) {
        frame = frame.actions(|| {
            button("New run")
                .leading(icon::PLUS)
                .on_press(|| {
                    if crate::platform::probing() {
                        crate::platform::probe_line("press reached nsh: New run button");
                    }
                    new_run::open(Prefill::default())
                })
                .into_element()
        });
    }
    frame
        .bar_actions(|| {
            Button::icon(icon::PLUS, "New run")
                .variant(Variant::Secondary)
                .size(Size::Icon)
                .round()
                .on_press(|| new_run::open(Prefill::default()))
                .into_element()
        })
        .child(content)
}

/// Below 1024 px: the name and the status, then the facts (and from 640 px the date), then the
/// progress; a chevron at the end, as every row that opens a screen. A press opens the run.
fn stacked_row(run: Run, now: Signal<DateTime<Utc>>) -> impl IntoElement {
    let created = run.created_at;
    let (id, name) = (run.id, run.name.clone());
    view()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .py(px(12.0))
        .cursor(Cursor::Pointer)
        .role(AccessibilityRole::Link)
        .label(run.name.clone())
        .on_press(move || nav::push(Route::Run { id }, Some(name.clone())))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    view()
                        .flex_row()
                        .items_start()
                        .gap(px(12.0))
                        .child(
                            view()
                                .flex_1()
                                .min_w(px(0.0))
                                .gap(px(2.0))
                                .child(label(text(run.name.clone())).number_of_lines(1))
                                .child(
                                    meta(text({
                                        let facts = facts(&run).join(" · ");
                                        move || {
                                            if window_width() >= 640.0 {
                                                format!("{facts} · {}", relative(created, now.get()))
                                            } else {
                                                facts.clone()
                                            }
                                        }
                                    }))
                                    .number_of_lines(1),
                                ),
                        )
                        .child(run_status(run.status)),
                )
                .child(
                    view()
                        .mt(px(10.0))
                        .gap(px(6.0))
                        .child(progress_figures(&run))
                        .child(progress_bar_full(run.done, run.failed, run.total)),
                ),
        )
        .child(glyph(icon::CHEVRON_RIGHT, 16.0, muted_ink))
        .child(view().absolute().left(px(0.0)).right(px(0.0)).bottom(px(0.0)).h(px(1.0)).bg(move || nsh_base::theme().border))
}

/// What the row says besides the name: the model, the endpoint and the prompt set, minus what the
/// name already says.
fn facts(run: &Run) -> Vec<String> {
    let lower = run.name.to_lowercase();
    let mut out = Vec::new();
    if !lower.contains(&run.model.to_lowercase()) {
        out.push(run.model.clone());
    }
    out.push(run.endpoint_name.clone());
    if !lower.contains(&run.prompt_set_name.to_lowercase()) {
        out.push(run.prompt_set_name.clone());
    }
    out
}

/// One run's row.
fn run_row(run: Run, now: Signal<DateTime<Utc>>, last: impl Fn() -> bool + 'static) -> impl IntoElement {
    let created = run.created_at;
    row(vec![
        cell(NAME, name_cell(&run)).into_element(),
        cell(PROGRESS, progress_cell(&run)).into_element(),
        cell(SEQS, body(text(int(run.concurrency))).tabular_nums()).into_element(),
        cell(STATUS, run_status(run.status)).into_element(),
        cell(CREATED, body(text(move || relative(created, now.get()))).text_color(muted_ink).tabular_nums().number_of_lines(1))
            .into_element(),
    ], last)
    .cursor(Cursor::Pointer)
    .role(AccessibilityRole::Link)
    .label(run.name.clone())
    .on_press({
        let (id, name) = (run.id, run.name.clone());
        move || nav::push(Route::Run { id }, Some(name.clone()))
    })
}

/// The name at 500, then in 12 px muted the model (mono), the endpoint and the prompt set, minus
/// what the name already says: a default name ("<model> on <set>") leaves only the endpoint.
fn name_cell(run: &Run) -> impl IntoElement {
    let facts: Vec<(String, bool)> = facts(run).into_iter().map(|f| {
        let mono = f == run.model;
        (f, mono)
    }).collect();
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
    view()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .child(progress_bar(run.done, run.failed, run.total, BAR_WIDTH))
        .child(progress_figures(run))
}

/// "1,284 of 5,000", the total muted, and "3 failed" in red when there are failures.
fn progress_figures(run: &Run) -> impl IntoElement {
    let failed = (run.failed > 0)
        .then(|| body(text(format!("   {} failed", int(run.failed)))).text_color(token("negative-ink")).into_element());
    body(text(int(run.done)))
        .tabular_nums()
        .number_of_lines(1)
        .child(body(text(format!(" of {}", int(run.total)))).text_color(muted_ink))
        .child(failed)
}

/// The current time, from nsh's clock (the same on native targets and the web).
fn now_utc() -> DateTime<Utc> {
    DateTime::from_timestamp_millis(nsh_core::time::unix_ms() as i64).unwrap_or_default()
}

// ------------------------------------------------------------------ No runs yet

/// No runs: the list keeps its frame (the header band, from 1024 px) and says what will appear
/// there, then the steps to a first run (frontend/src/pages/runs-list.tsx `RunsEmpty`).
fn runs_empty() -> impl IntoElement {
    let table = move || window().width.0 >= TABLE_FROM;
    view()
        .gap(px(16.0))
        .child(dynamic(move || table().then(|| header(&COLUMNS))))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(16.0))
                .px(move || px(if table() { 12.0 } else { 0.0 }))
                .child(plasma("runs-empty", 8, 56.0, radius::MD, false))
                .child(
                    body(text("No runs yet. Each run you start shows here with its progress and status, newest first."))
                        .text_color(muted_ink)
                        .max_w(px(432.0))
                        .flex_shrink(1.0)
                        .min_w(px(0.0)),
                ),
        )
        .child(setup_checklist())
}

/// One step of the checklist.
struct Step {
    title: &'static str,
    body: &'static str,
    action: &'static str,
    done: bool,
    done_text: String,
    go: fn(),
}

/// First run: the runs table is empty, so its frame holds the three steps to a first run, each
/// marked done from real data. The next step to take is the one button; the step marks show
/// progress (frontend/src/components/runs/setup-checklist.tsx).
fn setup_checklist() -> impl IntoElement {
    let endpoints = signal(Loaded::<Vec<EndpointOut>>::Loading);
    let sets = signal(Loaded::<Vec<PromptSet>>::Loading);
    let load = move || {
        call(|api| async move { api.endpoints().await }, move |r| endpoints.set(Loaded::from(r)));
        call(|api| async move { api.get::<Vec<PromptSet>>("/api/prompt-sets").await }, move |r| sets.set(Loaded::from(r)));
    };
    load();
    let retry: Rc<dyn Fn()> = Rc::new(move || {
        endpoints.set(Loaded::Loading);
        sets.set(Loaded::Loading);
        load();
    });

    dynamic(move || -> AnyElement {
        let failed = endpoints.with(|l| l.error().is_some()) || sets.with(|l| l.error().is_some());
        if failed {
            return view()
                .max_w(px(640.0))
                .child(error_block(
                    "Couldn't check your endpoints and prompt sets. Is the Stellarator server running?",
                    Some(retry.clone()),
                ))
                .into_element();
        }
        let n_endpoints = endpoints.with(|l| l.loaded().map(Vec::len));
        let n_sets = sets.with(|l| l.loaded().map(Vec::len));
        let loading = n_endpoints.is_none() || n_sets.is_none();
        let (n_endpoints, n_sets) = (n_endpoints.unwrap_or(0) as i64, n_sets.unwrap_or(0) as i64);
        let steps = [
            Step {
                title: "Add an endpoint",
                body: "Any OpenAI-compatible server: vLLM, SGLang, llama.cpp, Ollama or a hosted API.",
                action: "Add an endpoint",
                done: n_endpoints > 0,
                done_text: if n_endpoints == 1 { "1 endpoint added".to_owned() } else { format!("{} endpoints added", int(n_endpoints)) },
                go: || nav::open(Destination::Endpoints),
            },
            Step {
                title: "Import prompts",
                body: "From Hugging Face, a file or pasted text.",
                action: "Import prompts",
                done: n_sets > 0,
                done_text: if n_sets == 1 { "1 prompt set imported".to_owned() } else { format!("{} prompt sets imported", int(n_sets)) },
                go: || nav::push(Route::Import, None),
            },
            Step {
                title: "Start a run",
                body: "",
                action: "New run",
                done: false,
                done_text: String::new(),
                go: || new_run::open(Prefill::default()),
            },
        ];
        let next = steps.iter().position(|s| !s.done);
        let rows: Vec<AnyElement> =
            steps.into_iter().enumerate().map(|(i, step)| step_row(i, step, next == Some(i), loading)).collect();
        view()
            .max_w(px(640.0))
            .rounded(px(radius::XL))
            .bg(move || nsh_base::theme().muted)
            .p(px(16.0))
            .role(AccessibilityRole::Group)
            .label("Set up your first run")
            .child(title(text("Set up your first run")).role(AccessibilityRole::Header))
            .child(view().mt(px(8.0)).children(rows))
            .into_element()
    })
}

/// A step: its mark (the number, or a check once done), its title, what it says, and the action.
fn step_row(index: usize, step: Step, is_next: bool, loading: bool) -> AnyElement {
    let mark = view()
        .w(px(24.0))
        .h(px(24.0))
        .mt(px(1.0))
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(12.0))
        .bg(move || {
            if step.done {
                token("positive-tint")()
            } else if is_next {
                token("periwinkle-tint")()
            } else {
                nsh_base::theme().secondary
            }
        })
        .child(if step.done {
            glyph(icon::CHECK, 14.0, token("positive-ink")).into_element()
        } else {
            text(format!("{}", index + 1))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .tabular_nums()
                .text_color(move || if is_next { token("periwinkle-ink")() } else { muted_ink() })
                .into_element()
        });
    let go = step.go;
    let action: Option<AnyElement> = (!loading && !step.done).then(|| {
        if is_next {
            view().mt(px(12.0)).self_start().child(button(step.action).variant(Variant::Secondary).on_card().on_press(go)).into_element()
        } else {
            view()
                .mt(px(6.0))
                .self_start()
                .min_h(px(18.0))
                .justify_center()
                .role(AccessibilityRole::Link)
                .label(step.action)
                .focusable(true)
                .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
                .cursor(Cursor::Pointer)
                .on_press(go)
                .child(body(text(step.action)).text_color(muted_ink).underline())
                .into_element()
        }
    });
    let said = if loading {
        Some(view().mt(px(4.0)).child(bar(256.0, 16.0)).into_element())
    } else if step.done || !step.body.is_empty() {
        let line = if step.done { step.done_text.clone() } else { step.body.to_owned() };
        Some(body(text(line)).text_color(muted_ink).mt(px(2.0)).into_element())
    } else {
        None
    };
    view()
        .flex_row()
        .gap(px(12.0))
        .py(px(12.0))
        .child((index > 0).then(|| {
            view().absolute().top(px(0.0)).left(px(0.0)).right(px(0.0)).h(px(1.0)).bg(move || nsh_base::theme().border)
        }))
        .child(mark)
        .child(view().flex_1().min_w(px(0.0)).child(label(text(step.title))).child(said).child(action))
        .into_element()
}
