//! The run's samples (frontend/src/components/runs/samples-section.tsx): a status filter with
//! counts (line tabs from 1024 px, a pop-up button below), a search box, 50 rows a page from the
//! server (a table from 1024 px, stacked rows below), and the sample viewer a row opens. The rows
//! are read again every 3 s while the run goes; the last page stays on screen at 60% while the next
//! one comes.

use std::rc::Rc;
use std::time::Duration;

use nsh_base::{SelectEntry, SelectItem, select, theme};
use nsh_core::prelude::*;

use super::data::{SampleFilter, is_active, no_trace, page_span, sample_label, samples_query, show_status};
use super::step_pair::{Step, step_pair};
use super::viewer::sample_viewer;
use crate::api::types::{Page, RunOut, SampleStatus, SampleSummary};
use crate::data::call;
use crate::format::{DASH, duration, int, int_opt};
use crate::shortcuts;
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::metrics::CONTROL;
use crate::theme::text::{body, meta, title};
use crate::theme::{muted_ink, radius, token};
use crate::ui::blocks::{bar, empty_block, error_block};
use crate::ui::button::{Size, Variant, button};
use crate::ui::field::text_field;
use crate::ui::icon::{self, icon};
use crate::ui::material::{material_float, material_thick};
use crate::ui::page::window_width;
use crate::ui::status::sample_status_pill;
use crate::ui::table::{Column, cell, header, row};

/// The width from which the samples are a table and the filter is line tabs (Tailwind's `lg`).
const TABLE_FROM: f32 = 1024.0;
/// The width from which the table has the Thinking column (Tailwind's `xl`).
const THINKING_FROM: f32 = 1280.0;
/// How often a going run's rows are read again.
const REFRESH: Duration = Duration::from_secs(3);
/// How long the search waits for typing to stop.
const SEARCH_QUIET: Duration = Duration::from_millis(300);

const NUMBER: Column = Column::fixed("#", 88.0);
const PROMPT: Column = Column::flex("Prompt");
const RESPONSE: Column = Column::flex("Response");
const THINKING: Column = Column::fixed("Thinking", 88.0);
const TOKENS: Column = Column::numeric("Tokens", 96.0);
const LATENCY: Column = Column::numeric("Latency", 96.0);
const STATUS: Column = Column::fixed("Status", 120.0);
static PLAIN: [Column; 5] = [NUMBER, PROMPT, RESPONSE, TOKENS, LATENCY];
static WITH_THINKING: [Column; 6] = [NUMBER, PROMPT, RESPONSE, THINKING, TOKENS, LATENCY];
static WITH_STATUS: [Column; 6] = [NUMBER, PROMPT, RESPONSE, TOKENS, LATENCY, STATUS];
static WITH_BOTH: [Column; 7] = [NUMBER, PROMPT, RESPONSE, THINKING, TOKENS, LATENCY, STATUS];

/// What the section asks the server for.
#[derive(Debug, Clone, PartialEq)]
struct Key {
    filter: SampleFilter,
    q: String,
    page: i64,
}

/// The rows shown and how they came.
#[derive(Debug, Clone, PartialEq, Default)]
struct Rows {
    /// The last page the server sent, and what it was asked for.
    data: Option<(Key, Page<SampleSummary>)>,
    error: Option<String>,
}

/// The samples of `run` (reactive).
pub fn samples_section(run: impl Fn() -> RunOut + Clone + 'static) -> impl IntoElement {
    let run_id = untracked(&run).id;
    let filter = signal(SampleFilter::All);
    let q = signal(String::new());
    let search = signal(String::new());
    let page = signal(1_i64);
    let selected = signal(None::<i64>);
    let viewer_open = signal(false);
    let rows = signal(Rows::default());

    let key = move || Key { filter: filter.get(), q: q.get(), page: page.get() };
    let fetch = move |k: Key| {
        let query = samples_query(k.filter, &k.q, k.page);
        call(
            move |api| async move { api.run_samples(run_id, &query).await },
            move |result| {
                // An answer to a question no longer asked is dropped.
                if untracked(key) != k {
                    return;
                }
                rows.update(|r| match result {
                    Ok(p) => {
                        r.data = Some((k, p));
                        r.error = None;
                    }
                    Err(e) => r.error = Some(e.message),
                });
            },
        );
    };
    effect(move || fetch(key()));
    {
        let run = run.clone();
        let _ = set_interval(REFRESH, move || {
            if is_active(untracked(&run).status) {
                fetch(untracked(key));
            }
        });
    }

    // Typed text goes to the search after 300 ms; a new search starts at the first page.
    let typed = signal(0_u64);
    effect(move || {
        let text = search.get().trim().to_owned();
        if untracked(|| q.get()) == text {
            return;
        }
        let id = typed.get_untracked() + 1;
        typed.set(id);
        let _ = set_timeout(SEARCH_QUIET, move || {
            if typed.get_untracked() == id {
                q.set(text.clone());
                page.set(1);
            }
        });
    });

    let items = move || rows.with(|r| r.data.as_ref().map(|(_, p)| p.items.clone()).unwrap_or_default());
    let neighbor = move |offset: i64| {
        let id = selected.get()?;
        let list = items();
        let i = list.iter().position(|s| s.id == id)? as i64 + offset;
        (0..list.len() as i64).contains(&i).then(|| list[i as usize].id)
    };
    let open_sample = move |id: i64| {
        selected.set(Some(id));
        viewer_open.set(true);
    };
    // Closing the viewer forgets the sample (its row is no longer highlighted).
    effect(move || {
        if !viewer_open.get() {
            selected.set(None);
        }
    });
    // J and K step through the rows of the page while the viewer is open.
    shortcuts::bind("j", move || {
        if viewer_open.get_untracked()
            && let Some(id) = untracked(|| neighbor(1))
        {
            selected.set(Some(id));
        }
    });
    shortcuts::bind("k", move || {
        if viewer_open.get_untracked()
            && let Some(id) = untracked(|| neighbor(-1))
        {
            selected.set(Some(id));
        }
    });

    // A new page starts at the top of the table, not at the pager it was asked from.
    let top_marker = element_ref();
    let go_to_page = move |n: i64| {
        page.set(n);
        selected.set(None);
        spawn(async move {
            if let Some(m) = top_marker.measure().await
                && m.page.y.0 < 0.0
            {
                top_marker.reveal(RevealOptions::default().animated(false));
            }
        });
    };

    let r1 = run.clone();
    let bar = filter_bar(move || r1(), filter, q, search, page, selected);
    let r2 = run.clone();
    let body_rows = dynamic(move || {
        let (data, error) = rows.with(|r| (r.data.clone(), r.error.clone()));
        let want = untracked(key);
        match data {
            None if error.is_some() => view()
                .py(px(16.0))
                .child(error_block(
                    format!("Couldn’t load samples. {}", error.unwrap_or_default()),
                    Some(Rc::new(move || fetch(untracked(key)))),
                ))
                .into_element(),
            None => rows_loading().into_element(),
            Some((k, p)) if p.items.is_empty() => {
                let past = k.page > 1 && p.total > 0;
                let action = if !k.q.is_empty() {
                    Some(
                        button("Clear search")
                            .variant(Variant::Secondary)
                            .size(Size::Sm)
                            .leading(icon::X)
                            .on_press(move || search.set(String::new()))
                            .into_element(),
                    )
                } else if past {
                    Some(button("Go to the first page").variant(Variant::Secondary).size(Size::Sm).on_press(move || page.set(1)).into_element())
                } else {
                    None
                };
                let words = if past { format!("Page {} is past the last page.", int(k.page)) } else { k.filter.empty_text(&k.q) };
                empty_block(vec![body(text(words)).text_color(muted_ink).into_element()], action).into_element()
            }
            Some((k, p)) => {
                let spp = untracked(&r2).params.samples_per_prompt;
                let stale = k != want;
                let list = if window_width() >= TABLE_FROM {
                    sample_table(p.items, spp, k.filter, selected, open_sample).into_element()
                } else {
                    stacked_rows(p.items, spp, k.filter, selected, open_sample).into_element()
                };
                view().opacity(if stale { 0.6_f32 } else { 1.0 }).child(list).into_element()
            }
        }
    });

    let pager = dynamic(move || {
        let (k, total) = rows.with(|r| r.data.as_ref().map(|(k, p)| (k.clone(), p.total)))?;
        if total <= 0 {
            return None;
        }
        let (from, to, pages) = page_span(k.page, total);
        let p = k.page;
        Some(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .border_t_1()
                .border_color(move || theme().border)
                .pt(px(12.0))
                .child(body(text(format!("Showing {}–{} of {}", int(from), int(to), int(total)))).text_color(muted_ink).tabular_nums())
                .child((pages > 1).then(|| {
                    step_pair(
                        Step {
                            label: "Previous page",
                            shortcut: None,
                            disabled: Rc::new(move || p <= 1),
                            on_press: Rc::new(move || go_to_page(p - 1)),
                        },
                        Step {
                            label: "Next page",
                            shortcut: None,
                            disabled: Rc::new(move || p >= pages),
                            on_press: Rc::new(move || go_to_page(p + 1)),
                        },
                    )
                })),
        )
    });

    let r3 = run.clone();
    let viewer = sample_viewer(
        viewer_open,
        selected,
        move || untracked(&r3).params.samples_per_prompt,
        move || neighbor(-1),
        move || neighbor(1),
        move |id| selected.set(Some(id)),
    );

    view()
        .child(view().absolute().top(px(-(crate::theme::metrics::TOOLBAR_HEIGHT + 8.0))).h(px(0.0)).w(px(1.0)).element_ref(&top_marker))
        .child(title(text("Samples")).role(AccessibilityRole::Header))
        .child(bar)
        .child(body_rows)
        .child(pager)
        .child(viewer)
}

/// The tabs (or the pop-up button) and the search, on a 44 px bar with a hairline under it.
fn filter_bar(
    run: impl Fn() -> RunOut + Clone + 'static,
    filter: Signal<SampleFilter>,
    q: Signal<String>,
    search: Signal<String>,
    page: Signal<i64>,
    selected: Signal<Option<i64>>,
) -> impl IntoElement {
    let choose = move |f: SampleFilter| {
        filter.set(f);
        page.set(1);
        selected.set(None);
    };
    let wide = window_width() >= TABLE_FROM;
    let from_sm = window_width() >= 640.0;
    let filter_control: AnyElement = if wide { tabs(run, filter, q, choose).into_element() } else { filter_select(run, filter, q, choose).into_element() };
    let field = view()
        .flex_1()
        .min_w(px(0.0))
        .max_w(px(if from_sm { 288.0 } else { f32::MAX }))
        // Escape clears a search (and only then leaves the field).
        .on_key_down(move |e: &nsh_core::keyboard::KeyEvent| {
            if e.key.as_ref() == "Escape" && !search.with_untracked(String::is_empty) {
                search.set(String::new());
            }
        })
        .child(text_field(search).search().placeholder("Search samples").accessibility_label("Search samples"));
    view()
        .h(px(44.0))
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .border_b_1()
        .border_color(move || theme().border)
        .child(view().h_full().min_w(px(0.0)).flex_row().items_center().child(filter_control))
        .child(from_sm.then(|| view().flex_1()))
        .child(field)
}

/// A count after a filter's name: muted, red for failures; none while searching.
fn filter_count(run: &RunOut, f: SampleFilter) -> (String, bool) {
    let n = f.count(run);
    (int(n), f == SampleFilter::Failed && n > 0)
}

/// Line tabs with counts, from 1024 px: the current one ink at 500 with a 2 px underline on the
/// bar's hairline.
fn tabs(
    run: impl Fn() -> RunOut + Clone + 'static,
    filter: Signal<SampleFilter>,
    q: Signal<String>,
    choose: impl Fn(SampleFilter) + Copy + 'static,
) -> impl IntoElement {
    let items: Vec<AnyElement> = SampleFilter::ALL
        .iter()
        .map(|&f| {
            let current = move || filter.get() == f;
            let run = run.clone();
            view()
                .h_full()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .role(AccessibilityRole::Link)
                .label(f.label())
                .accessibility_state(move || AccessibilityState { selected: current(), ..AccessibilityState::default() })
                .focusable(true)
                .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
                .cursor(Cursor::Pointer)
                .on_press(move || choose(f))
                .child(
                    body(text(f.label()))
                        .text_color(move || if current() { theme().foreground } else { theme().muted_foreground })
                        .style_when(current, |s| s.font_weight(FontWeight::MEDIUM)),
                )
                .child(dynamic(move || {
                    if !q.get().is_empty() {
                        return None;
                    }
                    let (n, red) = filter_count(&run(), f);
                    Some(body(text(n)).tabular_nums().text_color(move || if red { token("negative-ink")() } else { muted_ink() }))
                }))
                .child(
                    view()
                        .absolute()
                        .left(px(0.0))
                        .right(px(0.0))
                        .bottom(px(-1.0))
                        .h(px(2.0))
                        .rounded(px(1.0))
                        .bg(move || theme().foreground)
                        .opacity(move || if current() { 1.0_f32 } else { 0.0 }),
                )
                .into_element()
        })
        .collect();
    view().h_full().flex_row().items_stretch().gap(px(20.0)).role(AccessibilityRole::Navigation).label("Filter samples by status").children(items)
}

/// Below 1024 px the same choice as a pop-up button: the filter and its count.
fn filter_select(
    run: impl Fn() -> RunOut + Clone + 'static,
    filter: Signal<SampleFilter>,
    q: Signal<String>,
    choose: impl Fn(SampleFilter) + Copy + 'static,
) -> impl IntoElement {
    let r1 = run.clone();
    let trigger = view()
        .h(px(CONTROL))
        .min_w(px(112.0))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .pl(px(12.0))
        .pr(px(8.0))
        .rounded(px(radius::MD))
        .bg(move || theme().secondary)
        .hover(|s| s.bg(move || theme().accent))
        .cursor(Cursor::Pointer)
        .child(body(text(move || filter.get().label())).number_of_lines(1))
        .child(dynamic(move || {
            (q.get().is_empty()).then(|| body(text(int(filter.get().count(&r1())))).tabular_nums().text_color(muted_ink))
        }))
        .child(view().flex_1())
        .child(icon(icon::CHEVRONS_UP_DOWN, 14.0, move || theme().muted_foreground));
    let options: Vec<SelectItem> = SampleFilter::ALL.iter().map(|f| SelectItem::new(f.label(), f.label())).collect();
    let r2 = run.clone();
    select()
        .items(options)
        .value(move || nsh_base::SelectValue::from(Some(SharedString::from(filter.get().label()))))
        .on_change(move |next: &Option<SharedString>| {
            if let Some(f) = next.as_ref().and_then(|v| SampleFilter::ALL.iter().find(|f| f.label() == v.as_ref())) {
                choose(*f);
            }
        })
        .accessibility_label("Filter samples by status")
        .offset(px(4.0))
        .popup_style(|s| material_float(material_thick(s.rounded(px(radius::LG)).p(px(6.0)).min_w(px(128.0)))))
        .item_class("min-h-8 rounded-sm px-2.5 py-1.5 text-[13px] text-popover-foreground focus:bg-accent selected:font-medium")
        .render_item(move |entry: &SelectEntry| {
            let f = SampleFilter::ALL.iter().copied().find(|f| f.label() == entry.value.as_ref()).unwrap_or_default();
            let run = r2.clone();
            view().flex_row().items_center().gap(px(8.0)).child(body(text(f.label()))).child(dynamic(move || {
                if !q.get().is_empty() {
                    return None;
                }
                let (n, red) = filter_count(&run(), f);
                Some(body(text(n)).tabular_nums().text_color(move || if red { token("negative-ink")() } else { muted_ink() }))
            }))
        })
        .trigger(trigger)
        .self_start()
}

/// Rows loading: six at the table's pitch with bars where the values go.
fn rows_loading() -> impl IntoElement {
    let wide = window_width() >= TABLE_FROM;
    let rows: Vec<AnyElement> = (0..6)
        .map(|_| {
            view()
                .h(px(44.0))
                .flex_row()
                .items_center()
                .gap(px(16.0))
                .px(px(12.0))
                .border_b_1()
                .border_color(move || theme().border)
                .child(bar(40.0, 12.0))
                .child(view().flex_grow(1.0).basis(px(0.0)).child(view().h(px(16.0)).rounded(px(radius::XS)).bg(move || theme().secondary)))
                .child(wide.then(|| view().flex_grow(0.75).basis(px(0.0)).child(view().h(px(16.0)).rounded(px(radius::XS)).bg(move || theme().secondary))))
                .child(view().flex_grow(if wide { 0.75 } else { 1.0 }).basis(px(0.0)))
                .child(bar(48.0, 16.0))
                .child(bar(56.0, 16.0))
                .into_element()
        })
        .collect();
    view().pt(px(if wide { 12.0 } else { 0.0 })).role(AccessibilityRole::Progressbar).label("Loading samples").children(rows)
}

/// The response cell's words: a failed row's error, else the response, else the dash.
fn response_text(s: &SampleSummary) -> String {
    if s.status == SampleStatus::Failed
        && let Some(e) = s.error.clone().filter(|e| !e.is_empty())
    {
        return e;
    }
    s.response_preview.clone().filter(|r| !r.is_empty()).unwrap_or_else(|| DASH.into())
}

/// From 1024 px: the table.
fn sample_table(
    items: Vec<SampleSummary>,
    spp: i64,
    filter: SampleFilter,
    selected: Signal<Option<i64>>,
    open: impl Fn(i64) + Copy + 'static,
) -> impl IntoElement {
    let status = show_status(filter, &items);
    let thinking = window_width() >= THINKING_FROM && items.iter().any(no_trace);
    let columns: &'static [Column] = match (thinking, status) {
        (false, false) => &PLAIN,
        (true, false) => &WITH_THINKING,
        (false, true) => &WITH_STATUS,
        (true, true) => &WITH_BOTH,
    };
    let n = items.len();
    let rows: Vec<AnyElement> = items
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let id = s.id;
            let mut cells = vec![
                cell(NUMBER, mono_meta(sample_label(s.prompt_idx, s.sample_idx, spp))).into_element(),
                cell(PROMPT, body(text(non_empty(&s.prompt_preview))).number_of_lines(1).flex_1().min_w(px(0.0))).into_element(),
                cell(RESPONSE, body(text(response_text(&s))).text_color(muted_ink).number_of_lines(1).flex_1().min_w(px(0.0))).into_element(),
            ];
            if thinking {
                cells.push(cell(THINKING, body(text(if no_trace(&s) { "None" } else { "" })).text_color(muted_ink)).into_element());
            }
            let tokens_known = s.completion_tokens.is_some();
            cells.push(
                cell(
                    TOKENS,
                    body(text(int_opt(s.completion_tokens))).tabular_nums().text_color(move || if tokens_known { theme().foreground } else { muted_ink() }),
                )
                .into_element(),
            );
            cells.push(cell(LATENCY, body(text(duration(s.latency_ms.map(|m| m as f64)))).tabular_nums().text_color(muted_ink)).into_element());
            if status {
                cells.push(cell(STATUS, sample_status_pill(s.status, s.finish_reason.as_deref())).into_element());
            }
            row(cells, move || i + 1 == n)
                .bg(move || if selected.get() == Some(id) { theme().accent } else { AdaptiveColor::TRANSPARENT })
                .cursor(Cursor::Pointer)
                .role(AccessibilityRole::Link)
                .label(s.prompt_preview.clone())
                .on_press(move || open(id))
                .into_element()
        })
        .collect();
    view().pt(px(12.0)).child(header(columns)).children(rows)
}

/// Below 1024 px: one stacked row per sample, its facts on the first line.
fn stacked_rows(
    items: Vec<SampleSummary>,
    spp: i64,
    filter: SampleFilter,
    selected: Signal<Option<i64>>,
    open: impl Fn(i64) + Copy + 'static,
) -> impl IntoElement {
    let status = show_status(filter, &items);
    let rows: Vec<AnyElement> = items
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let id = s.id;
            view()
                .py(px(12.0))
                .cursor(Cursor::Pointer)
                .role(AccessibilityRole::Link)
                .label(s.prompt_preview.clone())
                .on_press(move || open(id))
                .child(
                    view()
                        .absolute()
                        .top(px(0.0))
                        .bottom(px(0.0))
                        .left(px(-12.0))
                        .right(px(-12.0))
                        .rounded(px(radius::MD))
                        .bg(move || theme().accent)
                        .opacity(move || if selected.get() == Some(id) { 1.0_f32 } else { 0.0 }),
                )
                .child((i > 0).then(|| view().absolute().top(px(0.0)).left(px(0.0)).right(px(0.0)).h(px(1.0)).bg(move || theme().border)))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .child(mono_meta(sample_label(s.prompt_idx, s.sample_idx, spp)))
                        .child(status.then(|| sample_status_pill(s.status, s.finish_reason.as_deref())))
                        .child(view().flex_1())
                        .child(s.completion_tokens.map(|t| mono_meta(format!("{} tokens", int(t))))),
                )
                .child(body(text(non_empty(&s.prompt_preview))).number_of_lines(1).mt(px(4.0)))
                .child(body(text(response_text(&s))).text_color(muted_ink).number_of_lines(1))
                .into_element()
        })
        .collect();
    view().children(rows)
}

fn non_empty(s: &str) -> String {
    if s.is_empty() { DASH.into() } else { s.to_owned() }
}

/// 12 px mono in muted ink: sample numbers, token counts.
fn mono_meta(words: impl Into<SharedString>) -> Text {
    meta(text(words.into())).font_family(ROBOTO_MONO.family).number_of_lines(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_rows_response_is_its_error() {
        let s: SampleSummary = serde_json::from_value(serde_json::json!({
            "id": 1, "prompt_idx": 0, "sample_idx": 0, "status": "failed", "prompt_preview": "",
            "response_preview": null, "has_reasoning": false, "finish_reason": null, "completion_tokens": null,
            "latency_ms": null, "error": "Connection refused"
        }))
        .unwrap();
        assert_eq!(response_text(&s), "Connection refused");
        assert_eq!(non_empty(&s.prompt_preview), DASH);
    }
}
