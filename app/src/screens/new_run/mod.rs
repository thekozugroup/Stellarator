//! New run (frontend/src/pages/run-new.tsx): the form that starts a run. Source (endpoint, model,
//! prompt set), Requests (seqs, samples per prompt), Sampling, the groups that open (Prompt
//! options, Advanced), the run's name, and the floating bar with the run's size, Cancel and Start
//! run. Starting the run replaces this screen with the run.
//!
//! The screen loads the endpoints, the prompt sets and the runs (which only suggest a model), and
//! the run to copy when it was opened from "Run again". Its first values come from
//! [`logic::initial_state`]. Each field is checked when it loses the focus (the ones the web UI
//! checks on blur) and all of them when the run starts; the first with an error gets the focus.
//!
//! [`open`] shows the screen with an endpoint, a prompt set or a run to start from (the web UI's
//! `/runs/new?endpoint=…&prompt_set=…&from=…`).

mod form;
mod logic;
mod state;

use std::rc::Rc;

use nsh_core::prelude::*;

use self::form::{FORM_WIDTH, form_view};
pub use self::logic::Prefill;
use self::logic::{initial_state, run_summary, server_errors, check_all, to_body};
use self::state::{Data, State};
use crate::api::types::{PromptSetOut as PromptSet, RunOut as Run};
use crate::data::{Load, call};
use crate::nav::{self, Destination, Route};
use crate::theme::text::body;
use crate::theme::{muted_ink, radius};
use crate::ui::action_bar::action_bar;
use crate::ui::blocks::error_block;
use crate::ui::button::{Variant, button};
use crate::ui::page::{is_phone, page};

/// What the shown New run starts from, and which screen entry it is for: a New run opened some
/// other way starts from nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Stash {
    entry: u64,
    prefill: Prefill,
}

/// The entry of a stash kept for the next New run entry made: the screen is made as soon as the
/// route changes, inside `nav::push`, before the new entry's id is known to [`open`].
const NEXT_ENTRY: u64 = u64::MAX;

/// Shows New run starting from `prefill`: a run to copy, an endpoint and a prompt set.
pub fn open(prefill: Prefill) {
    global::<Stash>().set(Stash { entry: NEXT_ENTRY, prefill });
    nav::push(Route::NewRun, None);
    keep_for_return(prefill);
}

/// The prefill of screen entry `entry`: the stash's when it was kept for that entry, or for the
/// next one made (it then belongs to `entry`); nothing otherwise.
fn prefill_for(stash: Stash, entry: u64) -> Option<Prefill> {
    (stash.entry == entry || stash.entry == NEXT_ENTRY).then_some(stash.prefill)
}

/// Keeps `prefill` for the shown entry: the form made again for it (after Edit endpoint, or a
/// change of the window's size class) starts from it.
pub(super) fn keep_for_return(prefill: Prefill) {
    global::<Stash>().set(Stash { entry: nav::current_entry(), prefill });
}

/// What the screen shows in place of the form.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Loading,
    Failed,
    Ready,
}

/// The New run screen.
pub fn screen() -> impl IntoElement {
    let prefill = prefill_for(global::<Stash>().get_untracked(), nav::current_entry());
    if let Some(p) = prefill {
        keep_for_return(p);
    }
    let prefill = prefill.unwrap_or_default();
    let data = Data { endpoints: signal(Load::Loading), sets: signal(Load::Loading), runs: signal(Load::Loading) };
    let source = signal(Load::<Run>::Loading);
    let state = State::new();
    let stage = signal(Stage::Loading);

    // Decides what shows: the form once everything it needs has answered. The runs only suggest a
    // model, so a failure to load them (or the run to copy) does not block the form.
    let settle = move || {
        let loaded = data.endpoints.with_untracked(|l| l.loaded().is_some()) && data.sets.with_untracked(|l| l.loaded().is_some());
        let failed = data.endpoints.with_untracked(|l| l.error().is_some()) || data.sets.with_untracked(|l| l.error().is_some());
        let answered =
            !data.runs.with_untracked(Load::is_loading) && (prefill.from.is_none() || !source.with_untracked(Load::is_loading));
        let next = if loaded && answered {
            Stage::Ready
        } else if failed {
            Stage::Failed
        } else {
            Stage::Loading
        };
        if next == Stage::Ready && stage.get_untracked() != Stage::Ready {
            let endpoints = data.endpoints.with_untracked(|l| l.loaded().cloned().unwrap_or_default());
            let sets = data.sets.with_untracked(|l| l.loaded().cloned().unwrap_or_default());
            let runs = data.runs.with_untracked(|l| l.loaded().cloned());
            let copied = source.with_untracked(|l| l.loaded().cloned());
            state.apply(&initial_state(&endpoints, &sets, runs.as_deref(), copied.as_ref(), prefill));
        }
        if stage.get_untracked() != next {
            stage.set(next);
        }
    };
    let load_endpoints = move || {
        call(|api| async move { api.endpoints().await }, move |r| {
            data.endpoints.set(Load::from(r));
            settle();
        })
    };
    let load_sets = move || {
        call(|api| async move { api.get::<Vec<PromptSet>>("/api/prompt-sets").await }, move |r| {
            data.sets.set(Load::from(r));
            settle();
        })
    };
    load_endpoints();
    load_sets();
    call(|api| async move { api.runs().await }, move |r| {
        data.runs.set(Load::from(r));
        settle();
    });
    if let Some(id) = prefill.from {
        call(move |api| async move { api.get::<Run>(&format!("/api/runs/{id}")).await }, move |r| {
            source.set(Load::from(r));
            settle();
        });
    }
    let retry: Rc<dyn Fn()> = Rc::new(move || {
        data.endpoints.set(Load::Loading);
        data.sets.set(Load::Loading);
        settle();
        load_endpoints();
        load_sets();
    });

    let submit: Rc<dyn Fn()> = Rc::new(move || start(state, data));

    let content = {
        let submit = submit.clone();
        dynamic(move || -> AnyElement {
            match stage.get() {
                Stage::Loading => skeleton().into_element(),
                Stage::Failed => {
                    let retry = retry.clone();
                    view()
                        .max_w(px(FORM_WIDTH))
                        .child(error_block(
                            "Couldn't load your endpoints and prompt sets. Check that the Stellarator server is running.",
                            Some(retry),
                        ))
                        .into_element()
                }
                Stage::Ready => untracked(|| {
                    let source_error = prefill.from.and_then(|id| {
                        source.with_untracked(|l| l.error().map(|e| format!("Couldn't copy the settings of run {id}. {e} Defaults are filled in instead.")))
                    });
                    form_view(state, data, submit.clone(), source_error).into_element()
                }),
            }
        })
    };

    page("New run")
        .bottom_bar(move || {
            if stage.get() != Stage::Ready {
                return view().into_element();
            }
            bar(state, data, submit.clone()).into_element()
        })
        .child(content)
}

/// The floating bar: the run's size at the start, Cancel and Start run at the end.
fn bar(state: State, data: Data, submit: Rc<dyn Fn()>) -> impl IntoElement {
    let summary = view().flex_1().min_w(px(0.0)).child(dynamic(move || {
        let text_summary = run_summary(data.selected_set(&state).as_ref(), &state.get(logic::Field::Samples), &state.get(logic::Field::Concurrency))?;
        Some(body(text(text_summary)).text_color(muted_ink).tabular_nums())
    }));
    let buttons = view()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .child(button("Cancel").variant(Variant::Secondary).on_press(|| nav::open(Destination::Runs)))
        .child(button("Start run").busy(move || state.starting.get(), "Starting…").on_press(move || submit()));
    // On a phone the size has a line of its own, the buttons at the end of the next. Read
    // tracked: the page makes the bar again when the width crosses 768 px.
    let children = if is_phone() {
        vec![view().flex_1().min_w(px(0.0)).gap(px(8.0)).child(summary).child(view().flex_row().justify_end().child(buttons)).into_element()]
    } else {
        vec![summary.into_element(), buttons.into_element()]
    };
    view().max_w(px(FORM_WIDTH)).child(action_bar(children))
}

/// Checks every field and, when they pass, starts the run; the screen becomes the run's.
fn start(state: State, data: Data) {
    let _ = data;
    if state.starting.get_untracked() {
        return;
    }
    state.form_error.set(None);
    let form = state.snapshot();
    let found = check_all(&form);
    if !found.is_empty() {
        state.show_errors(&found);
        return;
    }
    state.clear_errors();
    let body = match to_body(&form) {
        Ok(body) => body,
        Err(message) => {
            state.form_error.set(Some(message));
            return;
        }
    };
    state.starting.set(true);
    call(move |api| async move { api.post::<Run>("/api/runs", &body).await }, move |result| {
        state.starting.set(false);
        match result {
            Ok(run) => nav::reset_to(Route::Run { id: run.id }, Some(run.name)),
            Err(e) if !e.fields.is_empty() => {
                let (mapped, other) = server_errors(&e.fields);
                state.show_errors(&mapped);
                state.form_error.set((!other.is_empty()).then(|| other.join(" ")));
            }
            Err(e) => state.form_error.set(Some(e.message)),
        }
    });
}

/// The form's place while the endpoints and prompt sets load: the Source group's rows at their
/// real size.
fn skeleton() -> impl IntoElement {
    let rows = ["Endpoint", "Model", "Prompt set"].into_iter().enumerate().map(|(i, label)| {
        view()
            .h(px(44.0))
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(12.0))
            .child(body(text(label)))
            .child(view().w(px(192.0)).h(px(32.0)).rounded(px(radius::MD)).bg(move || nsh_base::theme().secondary))
            .child((i > 0).then(|| {
                view().absolute().top(px(0.0)).left(px(12.0)).right(px(12.0)).h(px(1.0)).bg(move || nsh_base::theme().border)
            }))
            .into_element()
    });
    view()
        .max_w(px(FORM_WIDTH))
        .rounded(px(radius::XL))
        .bg(move || nsh_base::theme().muted)
        .label("Loading endpoints and prompt sets")
        .children(rows.collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefill_reaches_the_screen_made_inside_push() {
        let run = Prefill { from: Some(4), endpoint: None, prompt_set: None };
        // Kept for the next entry before the push: the screen made for entry 9 takes it.
        assert_eq!(prefill_for(Stash { entry: NEXT_ENTRY, prefill: run }, 9), Some(run));
        // Kept for entry 9 (made again after a size change): only entry 9 takes it.
        assert_eq!(prefill_for(Stash { entry: 9, prefill: run }, 9), Some(run));
        assert_eq!(prefill_for(Stash { entry: 9, prefill: run }, 12), None);
    }
}
