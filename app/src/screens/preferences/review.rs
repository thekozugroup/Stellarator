//! Preference review (frontend/src/pages/preference-review.tsx): one prompt at a time, its
//! responses side by side (from 1024 px), the chosen and rejected marks, and the floating action
//! bar with the status line, Clear pair and the prompt's place in the queue.
//!
//! - The queue is one of three line tabs (Undecided, Decided, All) with their counts. The position
//!   and the tab are kept per screen entry, so leaving and coming back lands on the same prompt.
//! - A pair saves as soon as both marks are set; 450 ms later the next prompt comes. A prompt that
//!   leaves the queue (decided, in Undecided) makes the next one move up into its place.
//! - Prompts are read one at a time and kept; the neighbours are read ahead so J and K are instant,
//!   and the shown prompt stays on screen (its marks off) while the next one loads.
//! - Keys (as the web UI): 1-9 choose, Shift+1-9 reject, J or → next, K or ← previous, U clears,
//!   ? shows them. They stand down while a dialog or the "…" menu is open.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::auto_pair::auto_pair_dialog;
use super::candidate::{Card, candidate_card};
use super::delete::delete_alert;
use super::export::export_dialog;
use super::logic::{
    Empty, Filter, Mark, SaveState, Sel, StatusLine, all_decided_text, chip_label, compares_models, empty_kind, error_sentence,
    last_key, letter, matches_filter, messages, next_selection, past_end, position_text, runs_with_several, status_line, step,
    trace_unit_for,
};
use super::prompt_view::prompt_view;
use crate::api::types::{Page, PairIn, PreferenceItem, PreferenceSetOut as Set};
use crate::data::call;
use crate::format::int;
use crate::nav::{self, Route};
use crate::shortcuts;
use crate::theme::text::{body, title};
use crate::theme::{ink, muted_ink, radius, token};
use crate::ui::action_bar::action_bar_tinted;
use crate::ui::blocks::{bar, empty_block, error_block};
use crate::ui::button::{Button, Size, Variant, button};
use crate::ui::chip::{chip, chip_row};
use crate::ui::choice::line_tabs;
use crate::ui::icon;
use crate::ui::model_text::text_mode_toggle;
use crate::ui::overlay::{DialogWidth, dialog, dialog_header};
use crate::ui::page::{is_phone, page_with, window_width};
use crate::ui::plasma::plasma;
use crate::ui::select::{MenuEntry, menu_destructive, menu_entry, menu_with_open};
use crate::ui::status::{Tone, pill};
use crate::ui::tooltip::kbd;

/// How long a finished pair stays on screen before the next prompt replaces it.
const BEAT: Duration = Duration::from_millis(450);

/// The width from which the responses sit side by side (Tailwind's `lg`).
const COLUMNS_FROM: f32 = 1024.0;

/// What the screen has of its set.
#[derive(Debug, Clone, PartialEq)]
enum SetLoad {
    Loading,
    Loaded(Set),
    NotFound,
    Failed(String),
}

/// A prompt's place: the queue and the position in it.
type Key = (Filter, i64);

/// A prompt read from the API, and whether a change since made it stale (read again when shown).
#[derive(Debug, Clone, PartialEq)]
struct Cached {
    page: Page<PreferenceItem>,
    fresh: bool,
}

/// Where each review entry was (its tab and position), by screen entry id.
#[derive(Debug, Clone, Default, PartialEq)]
struct ReviewMemory(HashMap<u64, Key>);

/// What the review shows under the tabs (the item view is made again only when the prompt changes).
#[derive(Debug, Clone, PartialEq)]
enum Body {
    Failed(String),
    Skeleton,
    Empty(Filter),
    Prompt(i64),
}

/// The screen of preference set `id`.
pub fn screen(id: i64) -> impl IntoElement {
    let set = signal(SetLoad::Loading);
    let load_set = move || {
        call(move |api| async move { api.preference_set(id).await }, move |r| {
            set.set(match r {
                Ok(s) => SetLoad::Loaded(s),
                Err(e) if e.status == Some(404) => SetLoad::NotFound,
                Err(e) => SetLoad::Failed(e.message),
            });
        })
    };
    load_set();
    // How many runs the set compares, once loaded: the toolbar and the menu follow it, not the
    // counts that change with each pair.
    let runs = computed(move || set.with(|s| if let SetLoad::Loaded(s) = s { Some(s.runs.len()) } else { None }));
    effect(move || {
        if let SetLoad::Loaded(s) = set.get() {
            nav::set_label(s.name);
        }
    });

    let title_fn = move || -> Option<SharedString> {
        set.with(|s| match s {
            SetLoad::Loading => None,
            SetLoad::Loaded(s) => Some(s.name.clone().into()),
            SetLoad::NotFound => Some("Preference set not found".into()),
            SetLoad::Failed(_) => Some("Preference set".into()),
        })
    };
    let kind = computed(move || set.with(|s| std::mem::discriminant(s)));

    let review = Review::new(id, set, load_set);
    let content = dynamic(move || -> AnyElement {
        let _ = kind.get();
        match set.get_untracked() {
            SetLoad::Loading => loading_review().into_element(),
            SetLoad::NotFound => empty_block(
                vec![body(text("This preference set doesn't exist. It may have been deleted.")).text_color(muted_ink).into_element()],
                Some(button("Open preference sets").variant(Variant::Secondary).on_card().on_press(|| nav::back_to(0)).into_element()),
            )
            .p(px(24.0))
            .rounded(px(radius::XL))
            .bg(move || theme().muted)
            .into_element(),
            SetLoad::Failed(message) => error_block(
                format!("Couldn't load this preference set. {message}"),
                Some(Rc::new(move || {
                    set.set(SetLoad::Loading);
                    load_set();
                })),
            )
            .into_element(),
            SetLoad::Loaded(_) => review.view().into_element(),
        }
    });

    let menu_open = review.menu_open;
    let (auto_open, export_open, delete_open) = (review.auto_open, review.export_open, review.delete_open);
    // Below 768 px the "…" menu in the bar carries Export and Auto-pair too.
    let menu = move |phone: bool| -> AnyElement {
        let Some(run_count) = runs.get() else { return view().into_element() };
        let mut entries = Vec::new();
        if phone {
            entries.push(menu_entry(None, "Export pairs…", move || export_open.set(true)));
            if run_count >= 2 {
                entries.push(menu_entry(None, "Auto-pair by run…", move || auto_open.set(true)));
            }
            entries.push(MenuEntry::Separator);
        }
        entries.push(menu_destructive(Some(icon::TRASH), "Delete preference set…", move || delete_open.set(true)));
        let mut trigger = Button::icon(icon::ELLIPSIS, "More actions").variant(Variant::Secondary).size(Size::Icon).without_tooltip();
        if phone {
            trigger = trigger.round();
        }
        menu_with_open(trigger, entries, Some(224.0), menu_open).into_element()
    };

    let mut frame = page_with(title_fn).meta(move || meta(set));
    if !untracked(is_phone) {
        frame = frame.actions(move || {
            let Some(run_count) = runs.get() else { return view().into_element() };
            view()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child((run_count >= 2).then(|| {
                    button("Auto-pair by run…").variant(Variant::Secondary).on_press(move || auto_open.set(true))
                }))
                .child(button("Export pairs…").on_press(move || export_open.set(true)))
                .child(menu(false))
                .into_element()
        });
    }
    let bar_review = review.clone();
    frame
        .bar_actions(move || dynamic(move || menu(true)).into_element())
        .bottom_bar(move || bar_review.action_bar().into_element())
        .child(content)
}

/// The chips under the title: the runs it compares ("vs" between them), then the prompt set with
/// its tile; each a link. A bar while the set loads.
fn meta(set: Signal<SetLoad>) -> AnyElement {
    // The set without its counts: the chips are made again only when what they show changes.
    let shown = computed(move || match set.get() {
        SetLoad::Loaded(s) => SetLoad::Loaded(Set { prompt_count: 0, decided: 0, ..s }),
        other => other,
    });
    dynamic(move || -> AnyElement {
        match shown.get() {
            SetLoad::Loading => view().w(px(384.0)).max_w(nsh_core::geometry::relative(1.0)).h(px(28.0)).rounded(px(14.0)).bg(move || theme().secondary).into_element(),
            SetLoad::Loaded(s) => {
                let mut chips: Vec<AnyElement> = Vec::new();
                if s.runs.is_empty() {
                    chips.push(chip("Runs deleted").into_element());
                }
                for (n, r) in s.runs.iter().enumerate() {
                    if n > 0 {
                        chips.push(body(text("vs")).text_color(muted_ink).into_element());
                    }
                    let (name, mono) = chip_label(r, &s);
                    let (rid, rname) = (r.id, r.name.clone());
                    let c = chip(name).on_press(move || nav::push(Route::Run { id: rid }, Some(rname.clone())));
                    chips.push(if mono { c.mono() } else { c }.into_element());
                }
                let (ps_id, ps_name) = (s.prompt_set_id, s.prompt_set_name.clone());
                chips.push(
                    view()
                        .ml(px(4.0))
                        .child(
                            chip(s.prompt_set_name.clone())
                                .leading(plasma(&format!("prompt-set-{}", s.prompt_set_id), 5, 20.0, 0.0, false))
                                .on_press(move || nav::push(Route::PromptSet { id: ps_id }, Some(ps_name.clone()))),
                        )
                        .into_element(),
                );
                chip_row(chips).into_element()
            }
            _ => view().into_element(),
        }
    })
    .into_element()
}

/// The review's state, shared by its views, its keys and its dialogs. Every field is a handle.
#[derive(Clone, Copy)]
struct Review {
    id: i64,
    set: Signal<SetLoad>,
    load_set: fn_ptr::LoadSet,
    filter: Signal<Filter>,
    pos: Signal<i64>,
    entry: u64,
    cache: Signal<HashMap<Key, Cached>>,
    /// The last prompt read for real, shown (its marks off) while the next one loads.
    last: Signal<Option<(Filter, Page<PreferenceItem>)>>,
    item_error: Signal<Option<(Key, String)>>,
    inflight: Stored<HashSet<Key>>,
    drafts: Signal<HashMap<i64, Sel>>,
    save_state: Signal<Option<(i64, SaveState)>>,
    seq: Stored<u64>,
    advance: Stored<Option<Spawned<()>>>,
    help_open: Signal<bool>,
    auto_open: Signal<bool>,
    export_open: Signal<bool>,
    delete_open: Signal<bool>,
    menu_open: Signal<bool>,
    /// A zero-height mark 68 px above the prompt: revealing it puts the prompt under the toolbar.
    top_mark: ElementRef,
}

/// The set loader, as a `Copy` handle.
mod fn_ptr {
    use std::rc::Rc;

    #[derive(Clone, Copy)]
    pub struct LoadSet(pub nsh_core::prelude::Stored<Rc<dyn Fn()>>);

    impl LoadSet {
        pub fn run(&self) {
            let f = self.0.get();
            f();
        }
    }
}

impl Review {
    fn new(id: i64, set: Signal<SetLoad>, load_set: impl Fn() + 'static) -> Review {
        let entry = nav::current_entry();
        let (f, i) = global::<ReviewMemory>().with_untracked(|m| m.0.get(&entry).copied()).unwrap_or_default();
        let r = Review {
            id,
            set,
            load_set: fn_ptr::LoadSet(stored(Rc::new(load_set) as Rc<dyn Fn()>)),
            filter: signal(f),
            pos: signal(i),
            entry,
            cache: signal(HashMap::new()),
            last: signal(None),
            item_error: signal(None),
            inflight: stored(HashSet::new()),
            drafts: signal(HashMap::new()),
            save_state: signal(None),
            seq: stored(0),
            advance: stored(None),
            help_open: signal(false),
            auto_open: signal(false),
            export_open: signal(false),
            delete_open: signal(false),
            menu_open: signal(false),
            top_mark: element_ref(),
        };
        r.watch();
        r.bind_keys();
        r
    }

    fn key(&self) -> Key {
        (self.filter.get(), self.pos.get())
    }

    /// Reads the prompt at `key` (once at a time; a fresh one only when `force`).
    fn fetch(&self, key: Key, force: bool) {
        if self.inflight.with(|s| s.contains(&key)) {
            return;
        }
        if !force && self.cache.with_untracked(|c| c.get(&key).is_some_and(|e| e.fresh)) {
            return;
        }
        self.inflight.with(|s| s.insert(key));
        let r = *self;
        let id = self.id;
        call(
            move |api| async move { api.preference_items(id, key.0.param(), key.1, 1).await },
            move |result| {
                r.inflight.with(|s| s.remove(&key));
                match result {
                    Ok(page) => {
                        r.cache.update(|c| {
                            c.insert(key, Cached { page, fresh: true });
                        });
                        if r.item_error.with_untracked(|e| e.as_ref().is_some_and(|(k, _)| *k == key)) {
                            r.item_error.set(None);
                        }
                    }
                    Err(e) => r.item_error.set(Some((key, e.message))),
                }
            },
        );
    }

    /// The prompt shown and whether it is the last one standing in for the next (reactive).
    fn page(&self) -> Option<(Page<PreferenceItem>, bool)> {
        let key = self.key();
        if let Some(p) = self.cache.with(|c| c.get(&key).map(|e| e.page.clone())) {
            return Some((p, false));
        }
        if self.item_error.with(|e| e.as_ref().is_some_and(|(k, _)| *k == key)) {
            return None;
        }
        self.last.with(|l| l.as_ref().filter(|(f, _)| *f == key.0).map(|(_, p)| (p.clone(), true)))
    }

    fn item(&self) -> Option<PreferenceItem> {
        self.page().and_then(|(p, _)| p.items.into_iter().next())
    }

    fn placeholder(&self) -> bool {
        self.page().is_some_and(|(_, ph)| ph)
    }

    fn total(&self) -> i64 {
        self.page().map(|(p, _)| p.total).unwrap_or(0)
    }

    /// The marks shown for `item`: the draft, else the saved pair.
    fn sel_of(&self, item: &PreferenceItem) -> Sel {
        self.drafts.with(|d| d.get(&item.prompt_id).copied()).unwrap_or_else(|| Sel::of(item))
    }

    fn sel(&self) -> Sel {
        self.item().map(|it| self.sel_of(&it)).unwrap_or_default()
    }

    /// The save state of the shown prompt.
    fn status(&self) -> Option<SaveState> {
        let pid = self.item().map(|it| it.prompt_id)?;
        self.save_state.with(|s| s.as_ref().filter(|(p, _)| *p == pid).map(|(_, st)| st.clone()))
    }

    /// The shown prompt left the queue (it was decided while Undecided is shown).
    fn left(&self) -> bool {
        self.item().is_some_and(|it| !matches_filter(&it, self.filter.get()))
    }

    fn set_position(&self, n: i64) {
        self.pos.set(n);
        let (entry, f) = (self.entry, self.filter.get_untracked());
        global::<ReviewMemory>().update(|m| {
            m.0.insert(entry, (f, n));
        });
    }

    fn set_filter(&self, f: Filter) {
        self.cancel_advance();
        batch(|| {
            self.filter.set(f);
            self.set_position(0);
        });
    }

    fn cancel_advance(&self) {
        if let Some(timer) = self.advance.replace(None) {
            timer.cancel();
        }
    }

    /// Everything read so far is stale (after a save, a clear or auto-pair): what is shown is read
    /// again, the rest when it is next shown.
    fn invalidate(&self) {
        self.cache.update(|c| {
            for e in c.values_mut() {
                e.fresh = false;
            }
        });
    }

    /// The set read again (its counts), as the web UI invalidates the list.
    fn refresh_set(&self) {
        self.load_set.run();
    }

    /// Reads what is shown, the neighbours, and keeps the position inside the queue.
    fn watch(&self) {
        let r = *self;
        effect(move || {
            let key = r.key();
            let cached = r.cache.with(|c| c.get(&key).cloned());
            match cached {
                None => r.fetch(key, false),
                Some(e) => {
                    if !e.fresh {
                        r.fetch(key, true);
                    }
                    untracked(|| r.last.set(Some((key.0, e.page.clone()))));
                    let total = e.page.total;
                    // Past the end (the last prompt left the queue, or a stale position).
                    if total > 0 && key.1 >= total {
                        untracked(|| r.set_position(past_end(key.0, total)));
                        return;
                    }
                    for n in [key.1 + 1, key.1 - 1] {
                        if n >= 0 && n < total {
                            untracked(|| r.fetch((key.0, n), false));
                        }
                    }
                }
            }
        });
    }

    /// The prompt at `at` of queue `f` (of `count` prompts) left it: everything after it moved up
    /// by one. The next prompt, when read with the same count, takes its place at once.
    fn shift_queue(&self, f: Filter, at: i64, count: i64) {
        self.cache.update(|c| {
            let next = c.get(&(f, at + 1)).map(|e| e.page.clone());
            let fresh = next.as_ref().is_some_and(|n| n.total == count && !n.items.is_empty());
            let items = if fresh { next.map(|n| n.items).unwrap_or_default() } else { Vec::new() };
            c.insert((f, at), Cached { page: Page { items, total: (count - 1).max(0), offset: at, limit: 1 }, fresh: true });
        });
        self.invalidate();
    }

    /// Next (`forward`) or previous prompt.
    fn go(&self, forward: bool) {
        self.cancel_advance();
        let (item, f, i, total) = untracked(|| (self.item(), self.filter.get(), self.pos.get(), self.total()));
        let gone = item.as_ref().is_some_and(|it| !matches_filter(it, f));
        let Some(target) = step(i, total, gone, forward) else { return };
        if gone {
            self.shift_queue(f, i, total);
        }
        self.save_state.set(None);
        self.set_position(target);
    }

    /// Replaces prompt `item`'s copy in everything read (after a save or a clear).
    fn patch(&self, item: &PreferenceItem) {
        self.cache.update(|c| {
            for e in c.values_mut() {
                for it in e.page.items.iter_mut().filter(|it| it.prompt_id == item.prompt_id) {
                    *it = item.clone();
                }
            }
        });
        self.last.update(|l| {
            if let Some((_, p)) = l {
                for it in p.items.iter_mut().filter(|it| it.prompt_id == item.prompt_id) {
                    *it = item.clone();
                }
            }
        });
    }

    fn next_seq(&self) -> u64 {
        let n = self.seq.get() + 1;
        self.seq.replace(n);
        n
    }

    /// Saves `s` as prompt `prompt_id`'s pair; once saved and still shown, the next prompt comes
    /// after a beat.
    fn save(&self, prompt_id: i64, s: Sel) {
        let (Some(chosen), Some(rejected)) = (s.chosen, s.rejected) else { return };
        let mine = self.next_seq();
        self.cancel_advance();
        self.save_state.set(Some((prompt_id, SaveState::Saving)));
        let (r, id) = (*self, self.id);
        let body = PairIn { chosen_sample_id: chosen, rejected_sample_id: rejected };
        call(
            move |api| async move { api.save_pair(id, prompt_id, &body).await },
            move |result| {
                if mine != r.seq.get() {
                    return;
                }
                match result {
                    Ok(updated) => {
                        r.patch(&updated);
                        r.refresh_set();
                        let shown = untracked(|| r.item().filter(|it| it.prompt_id == prompt_id).map(|it| r.sel_of(&it)));
                        let settled = shown.is_none_or(|x| x == s);
                        r.drafts.update(|d| {
                            if d.get(&prompt_id).is_none_or(|cur| *cur == s) {
                                d.remove(&prompt_id);
                            }
                        });
                        r.save_state.set(Some((prompt_id, SaveState::Saved)));
                        if settled && shown.is_some() {
                            let timer = set_timeout(BEAT, move || {
                                r.advance.replace(None);
                                let (item, f, i, total) = untracked(|| (r.item(), r.filter.get(), r.pos.get(), r.total()));
                                let Some(item) = item.filter(|it| it.prompt_id == prompt_id) else { return };
                                let gone = !matches_filter(&item, f);
                                let more = if gone { total - 1 > 0 } else { i + 1 < total };
                                r.go(true);
                                announce_for_accessibility(if more { "Saved. Next prompt." } else { "Saved." });
                            });
                            r.advance.replace(Some(timer));
                        }
                    }
                    Err(e) => r.save_state.set(Some((prompt_id, SaveState::Error { clear: false, message: e.message }))),
                }
            },
        );
    }

    /// Clears the shown prompt's marks, and its saved pair.
    fn clear(&self) {
        let (cur, placeholder) = untracked(|| (self.item(), self.placeholder()));
        let Some(cur) = cur.filter(|_| !placeholder) else { return };
        self.cancel_advance();
        let pid = cur.prompt_id;
        self.drafts.update(|d| {
            d.remove(&pid);
        });
        if cur.chosen_sample_id.is_none() {
            self.next_seq();
            self.save_state.set(None);
            return;
        }
        let mine = self.next_seq();
        self.save_state.set(Some((pid, SaveState::Clearing)));
        let (r, id) = (*self, self.id);
        call(
            move |api| async move { api.clear_pair(id, pid).await },
            move |result| {
                if mine != r.seq.get() {
                    return;
                }
                match result {
                    Ok(()) => {
                        r.patch(&PreferenceItem { chosen_sample_id: None, rejected_sample_id: None, source: None, ..cur });
                        r.refresh_set();
                        r.save_state.set(Some((pid, SaveState::Cleared)));
                    }
                    Err(e) => r.save_state.set(Some((pid, SaveState::Error { clear: true, message: e.message }))),
                }
            },
        );
    }

    /// Marks response `n` (0-based) of the shown prompt.
    fn pick(&self, n: usize, mark: Mark) {
        let (cur, sel, placeholder) = untracked(|| (self.item(), self.sel(), self.placeholder()));
        let Some(cur) = cur.filter(|_| !placeholder) else { return };
        let Some(cand) = cur.candidates.get(n) else { return };
        self.cancel_advance();
        let next = next_selection(sel, mark, cand.sample_id, &cur.candidates);
        if next == sel {
            return;
        }
        let saved = next == Sel::of(&cur);
        self.drafts.update(|d| {
            if saved {
                d.remove(&cur.prompt_id);
            } else {
                d.insert(cur.prompt_id, next);
            }
        });
        if next.complete() && !saved {
            self.save(cur.prompt_id, next);
        } else {
            self.save_state.set(None);
        }
    }

    fn retry(&self) {
        let Some(SaveState::Error { clear, .. }) = untracked(|| self.status()) else { return };
        if clear {
            self.clear();
        } else if let Some(item) = untracked(|| self.item()) {
            self.save(item.prompt_id, untracked(|| self.sel()));
        }
    }

    /// Whether the keys stand down: a dialog or the menu is open.
    fn covered(&self) -> bool {
        untracked(|| {
            self.help_open.get() || self.auto_open.get() || self.export_open.get() || self.delete_open.get() || self.menu_open.get()
        })
    }

    fn bind_keys(&self) {
        const CHOOSE: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
        const REJECT: [&str; 9] = ["shift+1", "shift+2", "shift+3", "shift+4", "shift+5", "shift+6", "shift+7", "shift+8", "shift+9"];
        let r = *self;
        let loaded = move || untracked(|| r.set.with(|s| matches!(s, SetLoad::Loaded(_))));
        let guard = move |f: Rc<dyn Fn()>| {
            move || {
                if loaded() && !r.covered() {
                    f();
                }
            }
        };
        for (n, key) in CHOOSE.iter().enumerate() {
            shortcuts::bind(key, guard(Rc::new(move || r.pick(n, Mark::Chosen))));
        }
        for (n, key) in REJECT.iter().enumerate() {
            shortcuts::bind(key, guard(Rc::new(move || r.pick(n, Mark::Rejected))));
        }
        shortcuts::bind("shift+/", guard(Rc::new(move || r.help_open.set(true))));
        for key in ["j", "right"] {
            shortcuts::bind(key, guard(Rc::new(move || r.go(true))));
        }
        for key in ["k", "left"] {
            shortcuts::bind(key, guard(Rc::new(move || r.go(false))));
        }
        shortcuts::bind("u", guard(Rc::new(move || r.clear())));
    }

    /// What is under the tabs now.
    fn body_kind(&self) -> Body {
        let key = self.key();
        if let Some((k, message)) = self.item_error.get() {
            if k == key && !self.cache.with(|c| c.contains_key(&key)) {
                return Body::Failed(message);
            }
        }
        let Some((page, _)) = self.page() else { return Body::Skeleton };
        match page.items.first() {
            Some(item) => Body::Prompt(item.prompt_id),
            None if key.1 < page.total => Body::Skeleton,
            None => Body::Empty(key.0),
        }
    }

    /// The tabs, the prompt and its responses, and the dialogs.
    fn view(self) -> impl IntoElement {
        let r = self;
        let set = move || r.set.with(|s| if let SetLoad::Loaded(s) = s { Some(s.clone()) } else { None });
        // The tabs follow the queue; a press on one changes it as the web UI's (position 0).
        let tab = signal(untracked(|| r.filter.get()));
        effect(move || {
            let t = tab.get();
            if t != r.filter.get_untracked() {
                r.set_filter(t);
            }
        });
        effect(move || {
            let f = r.filter.get();
            if f != tab.get_untracked() {
                tab.set(f);
            }
        });
        let count = move |f: Filter| -> Rc<dyn Fn() -> String> { Rc::new(move || int(set().map(|s| f.count(&s)).unwrap_or(0))) };
        let tabs = line_tabs(Filter::ALL.iter().map(|&f| (f, f.label(), Some(count(f)))).collect(), tab);

        let kind = computed(move || r.body_kind());
        // A new prompt starts at its top: once it changes, the region comes into view when it was
        // scrolled past.
        let shown_prompt = computed(move || match kind.get() {
            Body::Prompt(p) => Some(p),
            _ => None,
        });
        let first = stored(true);
        effect(move || {
            let p = shown_prompt.get();
            if p.is_none() {
                return;
            }
            if first.replace(false) {
                return;
            }
            let mark = r.top_mark;
            spawn(async move {
                if let Some(m) = mark.measure().await {
                    if m.page.y.0 < 0.0 {
                        mark.reveal(RevealOptions::default().animated(false));
                    }
                }
            });
        });

        let body = dynamic(move || -> AnyElement {
            match kind.get() {
                Body::Failed(message) => error_block(
                    format!("Couldn't load this prompt. {message}"),
                    Some(Rc::new(move || {
                        r.item_error.set(None);
                        let key = untracked(|| r.key());
                        r.fetch(key, true);
                    })),
                )
                .into_element(),
                Body::Skeleton => review_skeleton().into_element(),
                Body::Empty(f) => match untracked(set) {
                    Some(s) => review_empty(&s, f, move |f| r.set_filter(f)).into_element(),
                    None => view().into_element(),
                },
                Body::Prompt(_) => match untracked(|| r.item()) {
                    Some(item) => r.prompt(item).into_element(),
                    None => view().into_element(),
                },
            }
        });

        let help_open = r.help_open;
        let shortcuts_dialog = dialog(help_open, "Keyboard shortcuts", move || {
            shortcuts_table(untracked(|| r.item()).map(|it| it.candidates.len()).unwrap_or(2)).into_element()
        })
        .width(DialogWidth::Sm);

        view()
            .child(view().mb(px(24.0)).child(tabs))
            .child(
                view()
                    .child(view().absolute().top(px(if is_phone() { -60.0 } else { -68.0 })).h(px(0.0)).w(px(1.0)).element_ref(&r.top_mark))
                    .child(body),
            )
            .child(shortcuts_dialog)
            // Made once: each reads the set as it opens, and the export's options outlive it.
            .child(auto_pair_dialog(Rc::new(set), r.auto_open, Rc::new(move || {
                r.invalidate();
                r.refresh_set();
            })))
            .child(export_dialog(r.id, r.export_open))
            .child(delete_alert(Rc::new(set), r.delete_open))
    }

    /// The prompt's header, the prompt and its responses.
    fn prompt(self, item: PreferenceItem) -> impl IntoElement {
        let r = self;
        let set = untracked(|| r.set.with(|s| if let SetLoad::Loaded(s) = s { Some(s.clone()) } else { None }));
        let multi_model = set.as_ref().is_some_and(compares_models);
        let several = runs_with_several(&item.candidates);
        let unit = trace_unit_for(&item.candidates);
        let help_open = r.help_open;
        let header = view()
            .mb(px(12.0))
            .flex_row()
            .items_center()
            .justify_between()
            .gap_x(px(16.0))
            .child(title(text(format!("Prompt {}", int(item.prompt_idx + 1)))).tabular_nums().role(AccessibilityRole::Header))
            .child(
                view().flex_row().items_center().gap(px(8.0)).child(text_mode_toggle()).child(
                    Button::icon(icon::KEYBOARD, "Keyboard shortcuts")
                        .muted()
                        .tooltip("Keyboard shortcuts (?)")
                        .on_press(move || help_open.set(true)),
                ),
            );
        let cards: Vec<AnyElement> = item
            .candidates
            .iter()
            .enumerate()
            .map(|(n, c)| {
                let sid = c.sample_id;
                candidate_card(Card {
                    candidate: c.clone(),
                    label: letter(n),
                    position: n + 1,
                    mark: Rc::new(move || r.sel().mark_of(sid)),
                    show_model: multi_model,
                    show_sample: several.contains(&c.run_id),
                    trace_unit: unit,
                    disabled: Rc::new(move || r.placeholder()),
                    on_mark: Rc::new(move |m| r.pick(n, m)),
                })
                .into_element()
            })
            .collect();
        let grid = if window_width() >= COLUMNS_FROM {
            let mut rows = Vec::new();
            let mut cards = cards.into_iter();
            while let Some(a) = cards.next() {
                let b = cards.next();
                rows.push(
                    view()
                        .flex_row()
                        .items_start()
                        .gap(px(12.0))
                        .child(view().flex_1().basis(px(0.0)).min_w(px(0.0)).child(a))
                        .child(view().flex_1().basis(px(0.0)).min_w(px(0.0)).child(b))
                        .into_element(),
                );
            }
            view().gap(px(12.0)).children(rows)
        } else {
            view().gap(px(12.0)).children(cards)
        };
        view().child(header).child(prompt_view(messages(&item))).child(grid.mt(px(12.0)))
    }

    /// The floating bar: the status line, Clear pair, and the prompt's place with its chevrons. Only
    /// while a prompt shows.
    fn action_bar(self) -> impl IntoElement {
        let r = self;
        dynamic(move || -> Option<AnyElement> {
            if !matches!(r.body_kind(), Body::Prompt(_)) {
                return None;
            }
            let status = view().flex_1().basis(px(256.0)).min_w(px(0.0)).child(dynamic(move || status_view(r)));
            let clear = dynamic(move || {
                (!r.sel().is_empty()).then(|| button("Clear pair").variant(Variant::Ghost).tooltip("Clear pair (U)").on_press(move || r.clear()))
            });
            let nav_row = view()
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .role(AccessibilityRole::Navigation)
                .label("Prompts")
                .child(
                    Button::icon(icon::CHEVRON_LEFT, "Previous prompt")
                        .variant(Variant::Secondary)
                        .tooltip("Previous prompt (K)")
                        .disabled(move || r.pos.get() == 0)
                        .on_press(move || r.go(false)),
                )
                .child(
                    body(text(move || position_text(r.pos.get(), r.total(), r.filter.get())))
                        .px(px(6.0))
                        .number_of_lines(1)
                        .tabular_nums()
                        .text_color(muted_ink),
                )
                .child(
                    Button::icon(icon::CHEVRON_RIGHT, "Next prompt")
                        .variant(Variant::Secondary)
                        .tooltip("Next prompt (J)")
                        .disabled(move || !r.left() && r.pos.get() + 1 >= r.total())
                        .on_press(move || r.go(true)),
                );
            let end = view().ml(nsh_core::geometry::auto()).flex_row().flex_wrap().items_center().gap(px(8.0)).child(clear).child(nav_row);
            let row = view()
                .flex_1()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_x(px(16.0))
                .gap_y(px(8.0))
                .child(status)
                .child(end);
            Some(action_bar_tinted(vec![row.into_element()], "review-bar-tint").into_element())
        })
    }
}

/// The status line: what the marks are, whether they saved, or what failed with Retry.
fn status_view(r: Review) -> AnyElement {
    let Some(item) = r.item() else { return view().into_element() };
    let line = status_line(&item, r.sel(), r.status().as_ref());
    let letter_span = |l: String| body(text(l)).font_weight(FontWeight::SEMIBOLD).text_color(ink);
    match line {
        StatusLine::None => view().into_element(),
        StatusLine::Error { clear, message } => view()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_x(px(12.0))
            .gap_y(px(6.0))
            .role(AccessibilityRole::Alert)
            .child(body(text(error_sentence(clear, &message))).min_w(px(0.0)).flex_shrink(1.0).text_color(token("negative-ink")))
            .child(button("Retry").variant(Variant::Secondary).size(Size::Sm).on_press(move || r.retry()))
            .into_element(),
        StatusLine::Saving => body(text("Saving…")).text_color(muted_ink).into_element(),
        StatusLine::Clearing => body(text("Clearing pair…")).text_color(muted_ink).into_element(),
        StatusLine::Cleared => body(text("Pair cleared.")).text_color(muted_ink).into_element(),
        StatusLine::Pair { chosen, rejected, saved, by_rule } => {
            let sentence = body(text(""))
                .child(letter_span(chosen))
                .child(body(text(" chosen over ")))
                .child(letter_span(rejected))
                .child(by_rule.then(|| body(text(", paired by auto-pair")).text_color(muted_ink)))
                .child(body(text(".")));
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_x(px(8.0))
                .child(saved.then(|| pill(Tone::Positive, "Saved")))
                .child(sentence)
                .into_element()
        }
        StatusLine::ChosenOnly(l) => body(text(""))
            .child(letter_span(l))
            .child(body(text(" is chosen. ")))
            .child(body(text("Pick a rejected response.")).text_color(muted_ink))
            .into_element(),
        StatusLine::RejectedOnly(l) => body(text(""))
            .child(letter_span(l))
            .child(body(text(" is rejected. ")))
            .child(body(text("Pick a chosen response.")).text_color(muted_ink))
            .into_element(),
    }
}

/// The keyboard shortcuts: one row per action, the keys at its end, in a 10 px card.
fn shortcuts_table(candidates: usize) -> impl IntoElement {
    let last = last_key(candidates);
    let dash = || body(text("–")).text_color(muted_ink).into_element();
    let k = |s: &str| kbd(s.to_owned()).into_element();
    let rows: Vec<(Vec<AnyElement>, &str)> = vec![
        (vec![k("1"), dash(), k(&last)], "Choose response"),
        (vec![k("Shift"), k("1"), dash(), k(&last)], "Reject response"),
        (vec![k("J"), k("→")], "Next prompt"),
        (vec![k("K"), k("←")], "Previous prompt"),
        (vec![k("U")], "Clear pair"),
        (vec![k("?")], "Keyboard shortcuts"),
    ];
    let lines = rows
        .into_iter()
        .enumerate()
        .map(|(n, (keys, action))| {
            view()
                .min_h(px(40.0))
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(24.0))
                .border_t_1()
                .border_color(move || if n == 0 { AdaptiveColor::TRANSPARENT } else { theme().border })
                .child(body(text(action)))
                .child(view().flex_row().items_center().gap(px(4.0)).children(keys))
                .into_element()
        })
        .collect::<Vec<_>>();
    view()
        .gap(px(16.0))
        .child(dialog_header("Keyboard shortcuts", None))
        .child(view().rounded(px(radius::MD)).bg(crate::ui::material::dialog_muted).px(px(12.0)).children(lines))
}

/// An empty queue keeps its frame (DESIGN.md, Do): a muted card with the pixel field, one sentence
/// and the action that fills it.
fn review_empty(set: &Set, filter: Filter, on_filter: impl Fn(Filter) + Copy + 'static) -> impl IntoElement {
    let (sentence, action): (AnyElement, Button) = match empty_kind(set, filter) {
        Empty::NothingToCompare => (
            body(text("No prompt has two finished responses yet. Prompts appear here once a run has two responses to compare."))
                .text_color(muted_ink)
                .into_element(),
            button("Open runs").variant(Variant::Secondary).on_card().on_press(|| nav::open(nav::Destination::Runs)),
        ),
        Empty::NoneDecided => (
            body(text("No prompt is decided yet.")).text_color(muted_ink).into_element(),
            button("Show undecided").variant(Variant::Secondary).on_card().on_press(move || on_filter(Filter::Undecided)),
        ),
        Empty::AllDecided => (
            title(text(all_decided_text(set.prompt_count))).into_element(),
            button("Review decided prompts").variant(Variant::Secondary).on_card().on_press(move || on_filter(Filter::Decided)),
        ),
    };
    empty_block(
        vec![plasma(&format!("preference-set-{}", set.id), 8, 56.0, radius::MD, false).into_element(), view().mt(px(16.0)).child(sentence).into_element()],
        Some(action.into_element()),
    )
    .gap(px(16.0))
    .p(px(24.0))
    .rounded(px(radius::XL))
    .bg(move || theme().muted)
}

/// Loading a prompt: the heading, the prompt card and two response cards at their real shapes.
fn review_skeleton() -> impl IntoElement {
    let ground = move || theme().background;
    let gbar = move |w: f32, h: f32, corner: f32| view().w(px(w)).max_w(nsh_core::geometry::relative(1.0)).h(px(h)).rounded(px(corner)).bg(ground);
    let card = move || {
        view()
            .rounded(px(radius::XL))
            .bg(move || theme().muted)
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .py(px(12.0))
                    .pr(px(12.0))
                    .pl(px(20.0))
                    .child(gbar(28.0, 28.0, radius::SM))
                    .child(gbar(160.0, 16.0, radius::XS))
                    .child(view().flex_1())
                    .child(gbar(176.0, 32.0, radius::MD)),
            )
            .child(
                view().gap(px(8.0)).px(px(8.0)).pb(px(8.0)).child(view().h(px(36.0)).rounded(px(radius::SM)).bg(ground)).child(
                    view()
                        .gap(px(8.0))
                        .rounded(px(radius::SM))
                        .bg(ground)
                        .px(px(12.0))
                        .py(px(12.0))
                        .child(view().h(px(16.0)).rounded(px(radius::XS)).bg(move || theme().secondary))
                        .child(view().h(px(16.0)).rounded(px(radius::XS)).bg(move || theme().secondary))
                        .child(view().h(px(16.0)).w(nsh_core::geometry::relative(0.75)).rounded(px(radius::XS)).bg(move || theme().secondary)),
                ),
            )
    };
    let cards = if window_width() >= COLUMNS_FROM {
        view()
            .flex_row()
            .gap(px(12.0))
            .child(view().flex_1().basis(px(0.0)).child(card()))
            .child(view().flex_1().basis(px(0.0)).child(card()))
    } else {
        view().gap(px(12.0)).child(card()).child(card())
    };
    view()
        .role(AccessibilityRole::Progressbar)
        .label("Loading prompt…")
        .child(view().mb(px(12.0)).child(bar(96.0, 20.0)))
        .child(
            view()
                .gap(px(8.0))
                .rounded(px(radius::XL))
                .bg(move || theme().muted)
                .px(px(20.0))
                .py(px(16.0))
                .child(gbar(576.0, 16.0, radius::XS).w_full().max_w(px(576.0)))
                .child(view().w(nsh_core::geometry::relative(0.667)).max_w(px(448.0)).h(px(16.0)).rounded(px(radius::XS)).bg(ground)),
        )
        .child(cards.mt(px(12.0)))
}

/// Loading the set: the tabs' row of bars over the prompt's skeleton.
fn loading_review() -> impl IntoElement {
    view()
        .role(AccessibilityRole::Progressbar)
        .label("Loading preference set…")
        .child(
            view()
                .mb(px(24.0))
                .h(px(40.0))
                .flex_row()
                .items_center()
                .gap(px(20.0))
                .border_b_1()
                .border_color(move || theme().border)
                .child(bar(96.0, 16.0))
                .child(bar(80.0, 16.0))
                .child(bar(48.0, 16.0)),
        )
        .child(review_skeleton())
}
