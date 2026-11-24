//! A run (frontend/src/pages/run-detail.tsx): the breadcrumb, the display title, the chips (status,
//! model, endpoint, prompt set, when it was made) and the actions; why it stopped and the way out
//! (Edit endpoint when the endpoint is the cause); the facts as tiles; throughput over seqs with the
//! seqs control, and beside it the sequences in flight while it goes; the samples with their
//! viewer; the export dialog.
//!
//! The run is read every second while it goes, every 10 s while paused, never once finished. A
//! failed read after a good one keeps the figures and says so ("Lost contact…").

mod actions;
mod chart;
pub mod chart_math;
pub mod data;
mod in_flight;
mod progress;
mod samples;
mod seqs;
mod step_pair;
mod tools_chip;
mod viewer;

use std::rc::Rc;
use std::time::Duration;

use nsh_base::theme;
use nsh_core::prelude::*;

use self::actions::RunActions;
use self::chart::{LiveNow, throughput_chart};
use self::data::{ActionFacts, EndpointProblem, Notice, checks_endpoint, endpoint_problem, is_active, poll_every, run_notice};
use self::in_flight::in_flight_list;
use self::progress::run_progress;
use self::samples::samples_section;
use self::seqs::{SeqsChange, seqs_control};
use crate::api::types::{EndpointOut, RunLive, RunOut, RunStatus};
use crate::data::{call, now_utc};
use crate::format::{date_time, int, relative, time};
use crate::nav::{self, Destination, Route};
use crate::screens::endpoints::{EditFocus, open_edit};
use crate::screens::export::run::run_export_dialog;
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::shape::sidebar_column;
use crate::theme::text::{body, title};
use crate::theme::{muted_ink, radius, token};
use crate::ui::blocks::{bar, error_block};
use crate::ui::button::{Size, Variant, button};
use crate::ui::chip::{chip, chip_row};
use crate::ui::page::{column_padding, page, page_with, window_width};
use crate::ui::plasma::plasma;
use crate::ui::status::run_status_badge;
use crate::ui::tooltip::tooltip_bubble;

/// What the screen has of the run.
#[derive(Debug, Clone, PartialEq)]
struct Live {
    /// The last good answer and when it came (ms).
    data: Option<(RunLive, i64)>,
    /// The last read failed: its message and whether the run is gone (404).
    error: Option<(String, bool)>,
}

/// Which frame the screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    Loading,
    Missing,
    Failed,
    Loaded,
}

/// The Run screen for run `id`.
pub fn screen(id: i64) -> impl IntoElement {
    let live = signal(Live { data: None, error: None });
    // A seqs value shown before the server confirms it.
    let seqs_shown = signal(None::<i64>);
    let fetching = signal(false);

    let fetch = move || {
        if fetching.get_untracked() {
            return;
        }
        fetching.set(true);
        call(
            move |api| async move { api.run_live(id).await },
            move |result| {
                fetching.set(false);
                live.update(|l| match result {
                    Ok(data) => {
                        l.data = Some((data, nsh_core::time::unix_ms() as i64));
                        l.error = None;
                    }
                    Err(e) => l.error = Some((e.message, e.status == Some(404))),
                });
            },
        );
    };
    fetch();
    // Every second while it goes, every 10 s while paused, never once finished.
    let tick = signal(0_u32);
    let _ = set_interval(Duration::from_secs(1), move || {
        let n = tick.get_untracked() + 1;
        tick.set(n);
        let status = live.with_untracked(|l| l.data.as_ref().map(|(d, _)| d.run.status));
        if let Some(every) = status.and_then(poll_every)
            && n % every == 0
        {
            fetch();
        }
    });

    // The run as shown: the server's, with a seqs change it has not confirmed yet.
    let run = computed(move || {
        live.with(|l| l.data.as_ref().map(|(d, _)| d.run.clone())).map(|mut r| {
            if let Some(n) = seqs_shown.get() {
                r.concurrency = n;
            }
            r
        })
    });
    let frame = computed(move || {
        live.with(|l| match (&l.data, &l.error) {
            (Some(_), _) => Frame::Loaded,
            (None, Some((_, true))) => Frame::Missing,
            (None, Some(_)) => Frame::Failed,
            (None, None) => Frame::Loading,
        })
    });
    effect(move || {
        if let Some(name) = run.with(|r| r.as_ref().map(|r| r.name.clone())) {
            nav::set_label(name);
        }
    });
    // The window's title (the page's in a browser): the progress while the run goes, then its name.
    effect(move || {
        if let Some(title) = run.with(|r| r.as_ref().map(window_title)) {
            set_window_title(title);
        }
    });
    cleanup(|| set_window_title("Stellarator".into()));

    // Made again only when the frame changes: the screen is made untracked, so what it reads
    // while it is made (the width, the run) does not make it again and drop the open sample, the
    // search or a dialog.
    dynamic(move || {
        let shown = frame.get();
        untracked(|| screen_for(shown, id, live, run, seqs_shown, fetch))
    })
}

/// The screen for `frame`: the skeleton, the missing run, the failure or the run.
fn screen_for(
    frame: Frame,
    id: i64,
    live: Signal<Live>,
    run: Memo<Option<RunOut>>,
    seqs_shown: Signal<Option<i64>>,
    fetch: impl Fn() + Copy + 'static,
) -> AnyElement {
    match frame {
        Frame::Loading => skeleton().into_element(),
        Frame::Missing => missing().into_element(),
        Frame::Failed => {
            let message = live.with_untracked(|l| l.error.as_ref().map(|e| e.0.clone()).unwrap_or_default());
            page("Run")
                .child(error_block(
                    format!("Couldn’t load this run. {message}"),
                    Some(Rc::new(move || {
                        live.update(|l| l.error = None);
                        fetch();
                    })),
                ))
                .into_element()
        }
        Frame::Loaded => loaded(id, live, run, seqs_shown, Rc::new(fetch)).into_element(),
    }
}

/// "120 of 400 · Distill gsm8k · Stellarator" while the run goes, else "Distill gsm8k ·
/// Stellarator".
fn window_title(run: &RunOut) -> String {
    let progress = if is_active(run.status) { format!("{} of {} · ", int(run.done), int(run.total)) } else { String::new() };
    format!("{progress}{} · Stellarator", run.name)
}

fn set_window_title(title: String) {
    let mut attributes = nsh_core::desktop::WindowAttributes::default();
    attributes.title = Some(title);
    nsh_core::element::desktop::main_window().set(attributes);
}

/// The screen with the run loaded.
fn loaded(id: i64, live: Signal<Live>, run: Memo<Option<RunOut>>, seqs_shown: Signal<Option<i64>>, fetch: Rc<dyn Fn()>) -> impl IntoElement {
    let the_run = move || run.get().expect("loaded");
    let export_open = signal(false);
    let actions = RunActions::new();
    // A seqs change on its way outlives the panels, which are made again as the run starts or stops.
    let seqs_change = SeqsChange::new();

    // A run the server sent back after a change: shown at once, then read again.
    let updated: Rc<dyn Fn(RunOut)> = {
        let fetch = fetch.clone();
        Rc::new(move |r: RunOut| {
            live.update(|l| {
                if let Some((d, _)) = &mut l.data {
                    d.run = r.clone();
                }
            });
            fetch();
        })
    };

    // Whether the endpoint caused a finished run's failures.
    let endpoint = signal(None::<Option<EndpointOut>>);
    let first_failed = signal(None::<Option<String>>);
    let asked = signal(false);
    effect(move || {
        let Some(r) = run.get() else { return };
        if !checks_endpoint(&r) || asked.get_untracked() {
            return;
        }
        asked.set(true);
        let endpoint_id = r.endpoint_id.unwrap_or_default();
        call(move |api| async move { api.endpoint(endpoint_id).await }, move |res| endpoint.set(Some(res.ok())));
        call(
            move |api| async move { api.run_samples(id, "status=failed&limit=1").await },
            move |res| first_failed.set(Some(res.ok().and_then(|p| p.items.into_iter().next()).and_then(|s| s.error))),
        );
    });
    let problem = computed(move || {
        run.with(|r| r.as_ref().map(|r| endpoint_problem(r, endpoint.get().as_ref(), first_failed.get().as_ref()))).unwrap_or(EndpointProblem::None)
    });

    // The chips: status, model, endpoint, prompt set, when it was made.
    let chip_facts = computed(move || {
        run.with(|r| {
            r.as_ref().map(|r| {
                (
                    r.id,
                    r.status,
                    r.model.clone(),
                    r.endpoint_id,
                    r.endpoint_name.clone(),
                    r.prompt_set_id,
                    r.prompt_set_name.clone(),
                    r.created_at,
                    r.tools.clone(),
                )
            })
        })
    });
    // What the actions depend on: only the fields their buttons read, so a running run's counts
    // (every second) do not make them again, nor close their open "…" menu.
    let action_facts = computed(move || {
        let elsewhere = !matches!(problem.get(), EndpointProblem::None);
        run.with(|r| r.as_ref().map(|r| ActionFacts::of(r, elsewhere)))
    });

    let on_export: Rc<dyn Fn()> = Rc::new(move || export_open.set(true));
    let actions_updated = updated.clone();
    let mut frame = page_with(move || run.with(|r| r.as_ref().map(|r| SharedString::from(r.name.clone()))))
        .wide()
        .meta(move || chip_facts.get().map(|f| run_chips(f).into_element()).unwrap_or_else(|| view().into_element()))
        .actions(move || {
            let Some(facts) = action_facts.get() else { return view().into_element() };
            actions.buttons(&facts, on_export.clone(), actions_updated.clone())
        });

    // Lost contact: the figures stay, with when they were read.
    let lost = {
        let fetch = fetch.clone();
        dynamic(move || {
            let (failed, at) = live.with(|l| (l.error.is_some(), l.data.as_ref().map(|(_, t)| *t)));
            if !failed {
                return None;
            }
            let when = time(chrono::DateTime::from_timestamp_millis(at.unwrap_or_default()).unwrap_or_default());
            let fetch = fetch.clone();
            Some(notice(
                false,
                format!("Lost contact with the Stellarator server. Showing figures from {when}."),
                Some(button("Retry").variant(Variant::Secondary).on_card().size(Size::Sm).on_press(move || fetch()).into_element()),
            ))
        })
    };

    let reason = dynamic(move || {
        let r = run.get()?;
        let error = match problem.get() {
            EndpointProblem::Error(e) => Some(e),
            _ => None,
        };
        Some(match run_notice(&r, error.as_deref())? {
            Notice::Plain(words) => body(text(words)).text_color(muted_ink).mb(px(16.0)).into_element(),
            Notice::Card { text: words, problem, fix, primary } => {
                let action = fix.map(|(endpoint_id, focus)| edit_endpoint_button(endpoint_id, focus, primary));
                notice(problem, words, action)
            }
        })
    });

    let live_data = move || live.with(|l| l.data.as_ref().map(|(d, _)| d.clone())).expect("loaded");
    let tiles = run_progress(move || {
        let mut d = live_data();
        if let Some(r) = run.get() {
            d.run = r;
        }
        d
    });

    // Which panels show: the chart while the run can still go (it holds the seqs control) or when
    // it drew throughput; In flight while it goes.
    let panels_shape = computed(move || {
        let (status, has_throughput) = live.with(|l| {
            l.data
                .as_ref()
                .map(|(d, _)| (d.run.status, d.metrics.len() >= 2 && d.metrics.iter().any(|m| m.tokens_per_s > 0.0)))
                .unwrap_or((RunStatus::Completed, false))
        });
        let finished = status == RunStatus::Completed;
        (!finished || has_throughput, is_active(status), finished)
    });
    let panels = {
        let updated = updated.clone();
        dynamic(move || {
            let (show_chart, active, finished) = panels_shape.get();
            if !show_chart {
                return view().into_element();
            }
            let chart_panel = throughput_panel(id, live, run, seqs_shown, seqs_change, finished, updated.clone());
            let in_flight = active.then(|| in_flight_panel(live, run));
            let side_by_side = active && window_width() >= 1280.0;
            if side_by_side {
                view()
                    .mt(px(8.0))
                    .flex_row()
                    .items_stretch()
                    .gap(px(8.0))
                    .child(view().flex_grow(3.0).basis(px(0.0)).min_w(px(0.0)).child(chart_panel))
                    .child(view().flex_grow(2.0).basis(px(0.0)).min_w(px(0.0)).child(in_flight))
                    .into_element()
            } else {
                view().mt(px(8.0)).gap(px(8.0)).child(chart_panel).child(in_flight).into_element()
            }
        })
    };

    let shape = computed(move || {
        run.with(|r| {
            r.as_ref()
                .map(|r| {
                    let code = r.tools.as_ref().is_some_and(|t| t.mode == crate::api::types::ToolMode::Code);
                    (r.params.samples_per_prompt == 1, r.tools.is_some(), code)
                })
                .unwrap_or((true, false, false))
        })
    });
    let export = dynamic(move || {
        let (single, tools, code) = shape.get();
        run_export_dialog(export_open, id, single, tools, code).into_element()
    });

    frame = frame
        .child(lost)
        .child(reason)
        .child(tiles)
        .child(panels)
        .child(view().mt(px(32.0)).child(samples_section(the_run)))
        .child(export)
        .child(actions.dialogs(move || run.get(), updated));
    frame
}

/// Edit endpoint: the Endpoints screen's edit dialog for the run's endpoint, with the field that
/// fixes it focused. Primary when it is what a finished run needs next.
fn edit_endpoint_button(endpoint_id: i64, focus: EditFocus, primary: bool) -> AnyElement {
    let b = button("Edit endpoint").size(Size::Sm).on_press(move || open_edit(endpoint_id, focus));
    // A secondary button on the `muted` card takes the ground (light) or the raised tone (dark).
    if primary { b.into_element() } else { b.variant(Variant::Secondary).on_card().into_element() }
}

/// A notice above the facts: a `muted` card with a dot and the words in the signal's ink, red for
/// a failure and coral for a pause; its action a small button inset 8 px.
fn notice(problem: bool, words: String, action: Option<AnyElement>) -> AnyElement {
    let ink = if problem { token("negative-ink") } else { token("coral-ink") };
    let dot = move || if problem { theme().destructive } else { token("coral")() };
    let has_action = action.is_some();
    view()
        .mb(px(8.0))
        .min_h(px(44.0))
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_x(px(12.0))
        .gap_y(px(8.0))
        .rounded(px(radius::XL))
        .bg(move || theme().muted)
        .py(px(8.0))
        .pl(px(16.0))
        .pr(px(if has_action { 8.0 } else { 16.0 }))
        .role(AccessibilityRole::Alert)
        .child(
            view()
                .flex_1()
                .basis(px(256.0))
                .min_w(px(0.0))
                .flex_row()
                .items_start()
                .gap(px(10.0))
                .child(view().mt(px(6.0)).w(px(6.0)).h(px(6.0)).rounded(px(3.0)).flex_none().bg(dot))
                .child(body(text(words)).text_color(ink).flex_1().min_w(px(0.0))),
        )
        .child(action)
        .into_element()
}

/// The chips under the title: the status badge, the model (mono), the endpoint and the prompt set
/// (each a link with its pixel tile), the tools (a popover), then when the run was made, as plain
/// text.
#[allow(clippy::type_complexity)]
fn run_chips(
    (run_id, status, model, endpoint_id, endpoint_name, prompt_set_id, prompt_set_name, created, tools): (
        i64,
        RunStatus,
        String,
        Option<i64>,
        String,
        i64,
        String,
        chrono::DateTime<chrono::Utc>,
        Option<crate::api::types::ToolSnapshot>,
    ),
) -> impl IntoElement {
    let _ = run_id;
    let with_tip = |el: AnyElement, tip: String| view().flex_none().tooltip(tooltip_bubble(tip)).child(el).into_element();
    let endpoint_chip = match endpoint_id {
        Some(eid) => with_tip(
            chip(endpoint_name.clone())
                .leading(plasma(&format!("endpoint-{eid}"), 5, 20.0, 0.0, false))
                .on_press(move || open_edit(eid, EditFocus::None))
                .into_element(),
            format!("Endpoint: {endpoint_name}"),
        ),
        None => with_tip(chip(endpoint_name.clone()).into_element(), "Endpoint (deleted)".into()),
    };
    let set_name = prompt_set_name.clone();
    let mut chips = vec![
        run_status_badge(status).into_element(),
        with_tip(chip(model).mono().into_element(), "Model".into()),
        endpoint_chip,
        with_tip(
            chip(prompt_set_name.clone())
                .leading(plasma(&format!("prompt-set-{prompt_set_id}"), 5, 20.0, 0.0, false))
                .on_press(move || nav::push(Route::PromptSet { id: prompt_set_id }, Some(set_name.clone())))
                .into_element(),
            format!("Prompt set: {prompt_set_name}"),
        ),
    ];
    if let Some(snapshot) = tools {
        chips.push(tools_chip::tools_chip(snapshot).into_element());
    }
    chips.push(
        view()
            .tooltip(tooltip_bubble(date_time(created)))
            .child(body(text(format!("Created {}", relative(created, now_utc())))).text_color(muted_ink).number_of_lines(1))
            .into_element(),
    );
    chip_row(chips)
}

/// A card (`Panel`) whose body has `body_top` points above it: a 15 px title (or any heading), a
/// muted description and actions in the header row.
fn card(heading: AnyElement, description: Option<AnyElement>, actions: Option<AnyElement>, content: AnyElement, body_top: f32) -> View {
    view()
        .rounded(px(radius::XL))
        .bg(move || theme().muted)
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_x(px(16.0))
                .gap_y(px(8.0))
                .px(px(16.0))
                .pt(px(14.0))
                .pb(px(4.0))
                .child(view().min_w(px(0.0)).child(heading).child(description))
                .child(actions.map(|a| view().flex_row().flex_wrap().items_center().gap(px(8.0)).child(a))),
        )
        .child(view().px(px(16.0)).pb(px(16.0)).pt(px(body_top)).child(content))
}

/// Throughput: the chart, with the seqs control (or, once finished, the figure) in its header.
fn throughput_panel(
    id: i64,
    live: Signal<Live>,
    run: Memo<Option<RunOut>>,
    seqs_shown: Signal<Option<i64>>,
    seqs_change: SeqsChange,
    finished: bool,
    updated: Rc<dyn Fn(RunOut)>,
) -> AnyElement {
    let description = dynamic(move || {
        let tokens = run.with(|r| r.as_ref().map(|r| r.completion_tokens).unwrap_or(0));
        (tokens > 0).then(|| body(text(format!("{} output tokens", int(tokens)))).text_color(muted_ink))
    });
    let actions: AnyElement = if finished {
        // A finished run's seqs can no longer change: a plain figure.
        body(text("Seqs "))
            .text_color(muted_ink)
            .child(
                text(move || format!(" {}", int(run.with(|r| r.as_ref().map(|r| r.concurrency).unwrap_or(0)))))
                    .font_family(ROBOTO_MONO.family)
                    .text_color(move || theme().foreground),
            )
            .into_element()
    } else {
        seqs_control(
            id,
            seqs_change,
            move || run.with(|r| r.as_ref().map(|r| r.concurrency).unwrap_or(1)),
            move |n| seqs_shown.set(n),
            // The control clears `seqs_shown` once no newer change waits.
            move |r| updated(r),
        )
        .into_element()
    };
    let chart = throughput_chart(
        move || live.with(|l| l.data.as_ref().map(|(d, _)| d.metrics.clone()).unwrap_or_default()),
        move || {
            let lost = live.with(|l| l.error.is_some());
            live.with(|l| {
                let (d, at) = l.data.as_ref()?;
                let r = run.get()?;
                (is_active(r.status) && !lost).then_some(LiveNow { tps: d.tokens_per_s, seqs: r.concurrency as f64, active: d.in_flight as f64, at: *at })
            })
        },
    );
    card(title(text("Throughput")).role(AccessibilityRole::Header).into_element(), Some(description.into_element()), Some(actions), chart.into_element(), 8.0)
        .into_element()
}

/// The panel's width at the window's width: the page column (wide) less its padding, two fifths of
/// it beside the chart from 1280 px.
fn in_flight_width() -> f32 {
    let w = window_width();
    let main = if w >= 768.0 { w - sidebar_column() } else { w };
    let content = main.min(1536.0) - 2.0 * column_padding();
    if w >= 1280.0 { (content - 8.0) * 2.0 / 5.0 } else { content }
}

/// In flight: the log of the requests going now, its count in the title. Beside the chart (from
/// 1280 px) it takes the chart's height and scrolls; under it, at most 448 px.
fn in_flight_panel(live: Signal<Live>, run: Memo<Option<RunOut>>) -> AnyElement {
    let count = dynamic(move || {
        let n = live.with(|l| l.data.as_ref().map(|(d, _)| d.in_flight).unwrap_or(0));
        text(format!("  {}", int(n))).font_weight(FontWeight::NORMAL).tabular_nums().text_color(muted_ink)
    });
    let heading = title(text("In flight")).role(AccessibilityRole::Header).child(count);
    let list = dynamic(move || {
        let (seqs, n) = live.with(|l| l.data.as_ref().map(|(d, _)| (d.sequences.clone(), d.in_flight)).unwrap_or_default());
        let spp = run.with(|r| r.as_ref().map(|r| r.params.samples_per_prompt).unwrap_or(1));
        in_flight_list(seqs, n, spp, in_flight_width() - 32.0 >= 384.0)
    });
    let beside = window_width() >= 1280.0;
    let region = scroll().child(view().px(px(16.0)).pt(px(4.0)).pb(px(24.0)).child(list));
    let body_view = if beside {
        // Its height does not count: the row is as tall as the chart's card.
        view().flex_1().min_h(px(0.0)).child(region.absolute().inset(px(0.0)))
    } else {
        view().child(region.max_h(px(448.0)))
    };
    view()
        .h_full()
        .rounded(px(radius::XL))
        .bg(move || theme().muted)
        .child(view().px(px(16.0)).pt(px(14.0)).pb(px(4.0)).child(heading))
        .child(body_view)
        // The last 24 px fade into the card, so a row cut at the edge reads as more below.
        .child(
            view()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .h(px(24.0))
                .rounded_b(px(radius::XL))
                .bg_linear_to_b()
                .gradient_from(move || theme().muted.opacity(0.0))
                .gradient_to(move || theme().muted)
                .pointer_events(PointerEvents::None),
        )
        .into_element()
}

/// Loading: the frame at its real size, bars for the title and chips, the tiles and the chart.
fn skeleton() -> impl IntoElement {
    let chips = || {
        view()
            .flex_row()
            .flex_wrap()
            .gap(px(8.0))
            .children(
                [80.0, 160.0, 128.0, 112.0]
                    .iter()
                    .map(|w| view().w(px(*w)).h(px(28.0)).rounded(px(14.0)).bg(move || theme().secondary).into_element())
                    .collect::<Vec<_>>(),
            )
            .into_element()
    };
    let tile = |span: f32| {
        view()
            .flex_grow(span)
            .basis(px((span - 1.0) * 8.0))
            .min_w(px(0.0))
            .gap(px(8.0))
            .p(px(16.0))
            .rounded(px(radius::XL))
            .bg(move || theme().muted)
            .child(bar(80.0, 28.0))
            .child(bar(96.0, 12.0))
            .into_element()
    };
    let w = window_width();
    let rows = if w >= 1024.0 {
        vec![view().flex_row().gap(px(8.0)).children(vec![tile(2.0), tile(1.0), tile(1.0), tile(1.0)]).into_element()]
    } else if w >= 640.0 {
        vec![
            view().flex_row().child(tile(1.0)).into_element(),
            view().flex_row().gap(px(8.0)).children(vec![tile(1.0), tile(1.0), tile(1.0)]).into_element(),
        ]
    } else {
        vec![view().flex_row().child(tile(1.0)).into_element(), view().flex_row().gap(px(8.0)).children(vec![tile(1.0), tile(1.0)]).into_element()]
    };
    page_with(|| None).wide().meta(chips).child(
        view()
            .gap(px(8.0))
            .role(AccessibilityRole::Progressbar)
            .label("Loading run")
            .children(rows)
            .child(view().h(px(if w >= 640.0 { 320.0 } else { 260.0 })).rounded(px(radius::XL)).bg(move || theme().muted)),
    )
}

/// A run that does not exist (deleted, or a wrong address): a pixel tile, one sentence, the way
/// back.
fn missing() -> impl IntoElement {
    page("Run not found").child(
        view()
            .max_w(px(640.0))
            .items_start()
            .gap(px(16.0))
            .rounded(px(radius::XL))
            .bg(move || theme().muted)
            .p(px(24.0))
            .child(plasma("run-missing", 8, 56.0, radius::MD, false))
            .child(body(text("There is no run with this address. It may have been deleted.")).text_color(muted_ink).max_w(px(560.0)))
            .child(
                button("See all runs")
                    .variant(Variant::Secondary)
                    .on_card()
                    .on_press(|| nav::open(Destination::Runs)),
            ),
    )
}
