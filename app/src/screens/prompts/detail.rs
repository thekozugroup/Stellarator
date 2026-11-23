//! A prompt set (frontend/src/pages/prompt-set-detail.tsx): the breadcrumb and display title, the
//! chip row (status while it is not ready, the source, when it was created), New run and the "…"
//! menu (Rename…, Delete…), the tiles, the import's progress or failure, then the prompts: a search
//! field, Rendered or Raw, the rows, and Previous and Next at 50 a page. A set that is importing
//! reads again every second (its prompts every two).

use std::rc::Rc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use nsh_base::theme;
use nsh_core::prelude::*;

use super::dialogs::{Open, dialogs};
use super::list::set_seed;
use super::row::{prompt_row, rows_skeleton};
use super::source::source_parts;
use super::status::{IMPORTING_REFRESH, status_label, status_tone};
use crate::api::types::{Page, PromptOut, PromptSetOut as PromptSet, PromptSetStatus};
use crate::data::{Load, call, now_utc};
use crate::format::{int, relative};
use crate::nav;
use crate::theme::text::body;
use crate::theme::{ink, muted_ink, radius, token};
use crate::ui::blocks::{bar, error_block};
use crate::ui::button::{Button, Size, Variant, button};
use crate::ui::chip::{chip, chip_row};
use crate::ui::field::text_field;
use crate::ui::icon;
use crate::ui::model_text::text_mode_toggle;
use crate::ui::page::{column_padding, is_phone, page_with, window_width};
use crate::ui::plasma::plasma;
use crate::ui::progress::progress_bar_full;
use crate::ui::select::{menu_destructive, menu_entry, menu_with};
use crate::ui::status::status_badge;
use crate::ui::tiles::{tile, tiles};

/// Prompts per page.
const PAGE_SIZE: i64 = 50;
/// Prompts are read again this often while the set imports.
const PROMPTS_REFRESH: Duration = Duration::from_secs(2);
/// The search waits this long after the last key.
const SEARCH_DELAY: Duration = Duration::from_millis(300);

/// "1–50 of 213 matches" or "1–50": what the page shows of the total.
pub fn range_text(first: i64, last: i64, total: i64, searching: bool) -> String {
    if searching {
        format!("{}–{} of {} matches", int(first), int(last), int(total))
    } else {
        format!("{}–{}", int(first), int(last))
    }
}

/// The first and last prompt numbers of page `page` of `total` (0 and 0 when there are none).
pub fn page_bounds(page: i64, total: i64) -> (i64, i64) {
    if total == 0 { (0, 0) } else { ((page - 1) * PAGE_SIZE + 1, total.min(page * PAGE_SIZE)) }
}

/// The number of pages of `total` prompts (at least one).
pub fn page_count(total: i64) -> i64 {
    ((total + PAGE_SIZE - 1) / PAGE_SIZE).max(1)
}

/// The Prompt set screen for set `id`.
pub fn screen(id: i64) -> impl IntoElement {
    let set = signal(Load::<PromptSet>::Loading);
    let prompts = signal(None::<Page<PromptOut>>);
    // The search and the page the shown prompts answer (they lead `query` and `page` while the
    // next read is on its way): an empty page says why from these.
    let answered = signal((String::new(), 1_i64));
    let prompts_error = signal(None::<String>);
    let fetching = signal(false);
    let ticket = signal(0_u64);
    let page = signal(1_i64);
    let query = signal(String::new());
    let draft = signal(String::new());
    let draft_ticket = signal(0_u64);
    let now = signal(now_utc());
    let open = Open { rename: signal(false), delete: signal(false), stop: signal(false) };

    let fetch_prompts = move || {
        let n = ticket.get_untracked() + 1;
        ticket.set(n);
        fetching.set(true);
        let (p, q) = (page.get_untracked(), query.get_untracked());
        let asked = q.clone();
        call(
            move |api| async move { api.prompts(id, (p - 1) * PAGE_SIZE, PAGE_SIZE, &q).await },
            move |result| {
                if ticket.get_untracked() != n {
                    return;
                }
                fetching.set(false);
                match result {
                    Ok(data) => {
                        answered.set((asked, p));
                        prompts.set(Some(data));
                        prompts_error.set(None);
                    }
                    Err(e) => prompts_error.set(Some(e.message)),
                }
            },
        );
    };
    let fetch_set = move || {
        call(
            move |api| async move { api.prompt_set(id).await },
            move |result| {
                let before = set.with_untracked(|l| l.loaded().map(|s| s.status));
                now.set(now_utc());
                if let Ok(s) = &result {
                    nav::set_label(s.name.clone());
                    // The import just ended: read the prompts once more.
                    if before == Some(PromptSetStatus::Importing) && s.status != PromptSetStatus::Importing {
                        fetch_prompts();
                    }
                }
                set.set(Load::from(result));
            },
        );
    };
    fetch_set();
    fetch_prompts();
    let importing = move || set.with_untracked(|l| l.loaded().is_some_and(|s| s.status == PromptSetStatus::Importing));
    let _ = set_interval(IMPORTING_REFRESH, move || {
        if importing() {
            fetch_set();
        }
    });
    let _ = set_interval(PROMPTS_REFRESH, move || {
        if importing() {
            fetch_prompts();
        }
    });

    // A new search or page: read the prompts again.
    let go_to_page = move |p: i64| {
        page.set(p.max(1));
        fetch_prompts();
    };
    let apply_search = move |q: String| {
        if query.get_untracked() == q {
            return;
        }
        query.set(q);
        page.set(1);
        fetch_prompts();
    };
    let on_search = move |typed: &str| {
        let n = draft_ticket.get_untracked() + 1;
        draft_ticket.set(n);
        let typed = typed.trim().to_owned();
        let _ = set_timeout(SEARCH_DELAY, move || {
            if draft_ticket.get_untracked() == n {
                apply_search(typed);
            }
        });
    };
    let clear_search = move || {
        draft.set(String::new());
        draft_ticket.update(|n| *n += 1);
        apply_search(String::new());
    };

    // The structure is chosen by whether the set has loaded or failed; what changes every second
    // lives in narrower dynamics below, so the search field is never made again.
    let failed = move || set.with(|l| l.error().is_some());

    let heading = move || {
        set.with(|l| match l {
            Load::Loaded(s) => Some(SharedString::from(s.name.clone())),
            Load::Failed(_) => Some(SharedString::from("Prompt set")),
            Load::Loading => None,
        })
    };
    let meta = move || -> AnyElement {
        match set.with(|l| l.loaded().cloned()) {
            // On a phone the row wraps within the column: the page gives its meta no width of its own.
            Some(s) => view()
                .max_w(move || px(if is_phone() { (window_width() - 2.0 * column_padding()).max(0.0) } else { 100_000.0 }))
                .child(chip_row_of(&s, now))
                .into_element(),
            None => bar(288.0, 20.0).into_element(),
        }
    };
    let actions = move || -> AnyElement {
        match set.with(|l| l.loaded().map(|s| s.status)) {
            None => view().into_element(),
            Some(status) => {
                let new_run = (status == PromptSetStatus::Ready).then(|| {
                    button("New run").on_press(move || {
                        crate::screens::new_run::open(crate::screens::new_run::Prefill { prompt_set: Some(id), ..Default::default() })
                    })
                });
                let menu = (status != PromptSetStatus::Importing).then(|| {
                    menu_with(
                        Button::icon(icon::ELLIPSIS, "More actions").variant(Variant::Secondary).size(Size::Icon).without_tooltip(),
                        vec![
                            menu_entry(None, "Rename…", move || open.rename.set(true)),
                            crate::ui::select::MenuEntry::Separator,
                            menu_destructive(None, "Delete…", move || open.delete.set(true)),
                        ],
                        Some(176.0),
                    )
                });
                view().flex_row().items_center().gap(px(8.0)).child(new_run).child(menu).into_element()
            }
        }
    };

    let top = move || {
        let s = set.with(|l| l.loaded().cloned())?;
        Some(view().child(facts(&s)).child(progress_panel(&s, open)))
    };

    let range = move || {
        let data = prompts.with(|p| p.as_ref().map(|d| d.total))?;
        let q = query.get();
        if !(data > PAGE_SIZE || (!q.is_empty() && data > 0)) {
            return None;
        }
        let (first, last) = page_bounds(page.get(), data);
        Some(body(text(range_text(first, last, data, !q.is_empty()))).text_color(muted_ink).tabular_nums())
    };

    let list = move || -> AnyElement {
        let has_data = prompts.with(|p| p.is_some());
        if !has_data {
            return match prompts_error.get() {
                Some(m) => error_block(format!("Couldn’t load prompts. {m}"), Some(Rc::new(fetch_prompts))).into_element(),
                None => rows_skeleton(6).into_element(),
            };
        }
        let data = prompts.get().unwrap_or_else(|| Page { items: Vec::new(), total: 0, offset: 0, limit: PAGE_SIZE });
        if data.items.is_empty() {
            // Why this page is empty, as its own search and page say (Clear search or Go to page 1
            // leaves it on screen, dimmed, until the next page comes).
            let (q, p) = answered.get();
            return view()
                .opacity(move || if fetching.get() { 0.6_f32 } else { 1.0 })
                .transition_opacity()
                .child(empty_list(&q, data.total, p, importing(), clear_search, go_to_page))
                .into_element();
        }
        let n = data.items.len();
        let rows: Vec<AnyElement> = data.items.iter().enumerate().map(|(i, p)| prompt_row(p, i + 1 == n)).collect();
        view()
            .opacity(move || if fetching.get() { 0.6_f32 } else { 1.0 })
            .transition_opacity()
            .child(
                view()
                    .flex_row()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .py(px(8.0))
                    .rounded(px(radius::MD))
                    .bg(move || theme().muted)
                    .child(crate::theme::text::meta(text("#")).text_color(muted_ink).w(px(48.0)).flex_none())
                    .child(crate::theme::text::meta(text("Prompt")).text_color(muted_ink)),
            )
            .children(rows)
            .into_element()
    };

    let pager = move || {
        let total = prompts.with(|p| p.as_ref().map(|d| d.total))?;
        if total <= PAGE_SIZE {
            return None;
        }
        let pages = page_count(total);
        let current = page.get().min(pages);
        Some(
            view()
                .mt(px(16.0))
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .role(AccessibilityRole::Navigation)
                .label("Pages")
                .child(
                    button("Previous")
                        .variant(Variant::Secondary)
                        .leading(icon::CHEVRON_LEFT)
                        .disabled(move || page.get() <= 1)
                        .on_press(move || go_to_page(page.get_untracked() - 1)),
                )
                .child(body(text(format!("Page {} of {}", int(current), int(pages)))).text_color(muted_ink).tabular_nums())
                .child(
                    button("Next")
                        .variant(Variant::Secondary)
                        .trailing(icon::CHEVRON_RIGHT)
                        .disabled(move || page.get() >= pages)
                        .on_press(move || go_to_page(page.get_untracked() + 1)),
                ),
        )
    };

    // Made each time the set is shown again after a failed read (a Retry that worked): the search
    // keeps what was typed, as it is the screen's `draft`.
    let loaded_body = move || {
        let search = view()
            .w(move || px(if window_width() >= 640.0 { 320.0 } else { (window_width() - 2.0 * column_padding()).max(0.0) }))
            .child(
                text_field(draft)
                    .search()
                    .placeholder("Search prompts…")
                    .accessibility_label("Search prompts")
                    .on_change(on_search),
            );
        let toolbar = view()
            .mb(px(12.0))
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_x(px(16.0))
            .gap_y(px(8.0))
            .child(search)
            .child(view().flex_row().flex_wrap().items_center().gap(px(12.0)).child(dynamic(range)).child(text_mode_toggle()));
        view().child(dynamic(top)).child(toolbar).child(dynamic(list)).child(dynamic(pager))
    };
    let shown = view()
        .child(when(failed, move || {
            let message = set.with(|l| l.error().unwrap_or_default().to_owned());
            error_block(
                message,
                Some(Rc::new(move || {
                    set.set(Load::Loading);
                    fetch_set();
                    // The prompts failed with the set (the server was away): read them again too,
                    // so the set does not come back to a second Retry.
                    if prompts.with_untracked(Option::is_none) {
                        fetch_prompts();
                    }
                })),
            )
        }))
        .child(when(move || !failed(), loaded_body));

    // From 1024 px the actions sit in the toolbar; below, under the title with the chips.
    page_with(heading).meta(meta).actions(actions).child(shown).child(dialogs(set, open))
}

/// The chips and the date under the title.
fn chip_row_of(set: &PromptSet, now: Signal<DateTime<Utc>>) -> impl IntoElement {
    let parts = source_parts(&set.source_kind, &set.source);
    let mut row: Vec<AnyElement> = Vec::new();
    if set.status != PromptSetStatus::Ready {
        row.push(status_badge(status_tone(set.status), None, status_label(set.status)).into_element());
    }
    let first = parts.id.clone().unwrap_or_else(|| parts.kind.to_owned());
    let mut main = chip(first).leading(plasma(&set_seed(set.id), 5, 20.0, radius::XS, false));
    if parts.id.is_some() {
        main = main.mono();
    }
    row.push(main.into_element());
    if parts.id.is_some() {
        row.push(chip(parts.kind).into_element());
    }
    if let Some(whr) = &parts.whr {
        row.push(chip(whr.clone()).mono().into_element());
    }
    let created = set.created_at;
    // Read when the row is made too: a rename replaces the set without a fetch, and the web UI's
    // re-render after it reads the clock again.
    let made = now_utc();
    row.push(
        body(text(move || format!("Created {}", relative(created, now.get().max(made)))))
            .text_color(muted_ink)
            .px(px(4.0))
            .into_element(),
    );
    chip_row(row)
}

/// The tiles: Prompts, Runs (once ready) and Rows read (when it differs from the prompts).
fn facts(set: &PromptSet) -> impl IntoElement {
    let (prompts, runs, rows_read) = (set.prompt_count, set.run_count, set.rows_read);
    let ready = set.status == PromptSetStatus::Ready;
    let mut list = vec![tile("Prompts", move || int(prompts)).into_element()];
    if ready {
        list.push(tile("Runs", move || int(runs)).into_element());
    }
    if ready && rows_read > 0 && rows_read != prompts {
        list.push(tile("Rows read", move || int(rows_read)).into_element());
    }
    view().mb(px(24.0)).child(tiles(list))
}

/// The importing panel with Stop import, or the failed panel with Delete set; nothing when ready.
fn progress_panel(set: &PromptSet, open: Open) -> Option<AnyElement> {
    match set.status {
        PromptSetStatus::Importing => {
            let total = set.rows_total;
            let read = int(set.rows_read);
            let mut line = format!("Importing… {read}");
            if let Some(t) = total {
                line.push_str(&format!(" of {}", int(t)));
            }
            line.push_str(" rows");
            let detail = set.status_detail.clone().filter(|d| !d.is_empty());
            let bar_label = total.map(|t| format!("{} of {} rows read", read, int(t)));
            Some(
                view()
                    .mb(px(24.0))
                    .gap(px(12.0))
                    .p(px(16.0))
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
                            .child(
                                body(text(line))
                                    .tabular_nums()
                                    
                                    .child(detail.map(|d| body(text(format!(" · {d}"))).text_color(muted_ink))),
                            )
                            .child(button("Stop import").variant(Variant::Secondary).on_card().on_press(move || open.stop.set(true))),
                    )
                    .child(total.filter(|t| *t > 0).map(|t| {
                        view()
                            .role(AccessibilityRole::Progressbar)
                            .label(bar_label.unwrap_or_default())
                            .child(progress_bar_full(set.rows_read.min(t), 0, t))
                    }))
                    .into_element(),
            )
        }
        PromptSetStatus::Failed => Some(
            view()
                .mb(px(24.0))
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_x(px(16.0))
                .gap_y(px(8.0))
                .p(px(16.0))
                .rounded(px(radius::XL))
                .bg(token("coral-tint"))
                .role(AccessibilityRole::Alert)
                .child(
                    body(text(set.status_detail.clone().unwrap_or_else(|| "The import failed.".into())))
                        .text_color(token("coral-ink"))
                        .min_w(px(0.0))
                        .flex_shrink(1.0),
                )
                .child(button("Delete set").variant(Variant::Secondary).on_card().on_press(move || open.delete.set(true)))
                .into_element(),
        ),
        PromptSetStatus::Ready => None,
    }
}

/// The card that says why there are no rows: a search with no match, a page past the end, or an
/// empty set.
fn empty_list(
    q: &str,
    total: i64,
    page: i64,
    importing: bool,
    clear_search: impl Fn() + Copy + 'static,
    go_to_page: impl Fn(i64) + Copy + 'static,
) -> impl IntoElement {
    let link = |label: &'static str, f: Rc<dyn Fn()>| {
        body(text(label))
            .text_color(ink)
            .underline()
            .role(AccessibilityRole::Button)
            .focusable(true)
            .cursor(Cursor::Pointer)
            .on_press(move || f())
    };
    let content: AnyElement = if !q.is_empty() {
        view()
            .flex_row()
            .flex_wrap()
            .child(body(text(format!("No prompt contains “{q}”. "))).text_color(muted_ink))
            .child(link("Clear search", Rc::new(clear_search)))
            .into_element()
    } else if total > 0 {
        view()
            .flex_row()
            .flex_wrap()
            .child(body(text(format!("There is no page {}. ", int(page)))).text_color(muted_ink))
            .child(link("Go to page 1", Rc::new(move || go_to_page(1))))
            .into_element()
    } else {
        body(text(if importing { "No prompts yet." } else { "This prompt set has no prompts." })).text_color(muted_ink).into_element()
    };
    view().p(px(24.0)).rounded(px(radius::XL)).bg(move || theme().muted).child(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_says_which_prompts_it_shows() {
        assert_eq!(page_bounds(1, 0), (0, 0));
        assert_eq!(page_bounds(1, 7473), (1, 50));
        assert_eq!(page_bounds(150, 7473), (7451, 7473));
        assert_eq!(page_bounds(3, 7473), (101, 150));
        assert_eq!(page_bounds(2, 60), (51, 60));
    }

    #[test]
    fn pages_round_up_and_there_is_always_one() {
        assert_eq!(page_count(0), 1);
        assert_eq!(page_count(50), 1);
        assert_eq!(page_count(51), 2);
        assert_eq!(page_count(7473), 150);
    }

    #[test]
    fn the_range_names_matches_while_searching() {
        assert_eq!(range_text(1, 50, 7473, false), "1–50");
        assert_eq!(range_text(1, 12, 12, true), "1–12 of 12 matches");
        assert_eq!(range_text(1001, 1050, 2000, true), "1,001–1,050 of 2,000 matches");
    }
}
