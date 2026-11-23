//! New preference set (frontend/src/components/preferences/new-set-dialog.tsx): a 576 px dialog
//! with the name (suggested from the runs until it is typed in) and the runs grouped by prompt
//! set. Picking a run locks the other prompt sets; a run with no finished samples cannot be picked.
//! Create opens the new set's review.

use std::rc::Rc;

use nsh_base::{checkbox as base_checkbox, theme};
use nsh_core::prelude::*;

use super::logic::{group_runs, new_set_errors, runs_hint, suggest_name};
use crate::api::types::{PreferenceSetCreate, PreferenceSetOut as Set, RunOut, RunStatus};
use crate::data::{Load, call};
use crate::format::{count, int};
use crate::nav::{self, Route};
use crate::theme::text::{body, label, meta, mono};
use crate::theme::{muted_ink, radius, token};
use crate::ui::blocks::error_block;
use crate::ui::button::{Variant, button};
use crate::ui::field::{form_group, form_row, text_field};
use crate::ui::icon::{self, icon};
use crate::ui::material::dialog_muted;
use crate::ui::overlay::{DialogWidth, dialog, dialog_footer, dialog_header};
use crate::ui::plasma::plasma;
use crate::ui::status::run_status_pill;

/// The longest name the field takes.
const NAME_MAX: usize = 300;

/// The dialog, shown while `open` holds; `created` hears the new set (the list reads itself again).
pub fn new_set_dialog(open: Signal<bool>, created: Rc<dyn Fn(Set)>) -> impl IntoElement {
    dialog(open, "New preference set", move || form(open, created.clone()).into_element()).width(DialogWidth::Xl)
}

fn form(open: Signal<bool>, created: Rc<dyn Fn(Set)>) -> impl IntoElement {
    let runs = signal(Load::<Vec<RunOut>>::Loading);
    // The dialog may close before an answer comes: its signals are written with `try_set`.
    let load_runs = move || {
        call(|api| async move { api.runs().await }, move |r| {
            runs.try_set(Load::from(r));
        })
    };
    load_runs();

    let selected = signal(Vec::<i64>::new());
    let name = signal(String::new());
    let touched = signal(false);
    let err_name = signal(None::<String>);
    let err_runs = signal(None::<String>);
    let err_form = signal(None::<String>);
    let creating = signal(false);
    let name_ref = element_ref();
    // The dialog opens on its first field, as the web UI's does.
    let _ = request_animation_frame(move |_| name_ref.focus());

    let all_runs = move || runs.with(|l| l.loaded().cloned().unwrap_or_default());
    let selected_runs = move || -> Vec<RunOut> {
        let all = all_runs();
        selected.get().iter().filter_map(|id| all.iter().find(|r| r.id == *id).cloned()).collect()
    };
    // Until the name is typed in, it follows the runs picked.
    effect(move || {
        let picked = selected_runs();
        if !touched.get() {
            name.set(suggest_name(&picked.iter().collect::<Vec<_>>()));
        }
    });

    let close = move || open.set(false);
    let submit = move || {
        if creating.get_untracked() {
            return;
        }
        let picked = untracked(selected_runs);
        let refs: Vec<&RunOut> = picked.iter().collect();
        let final_name = name.get_untracked().trim().to_owned();
        let (name_error, runs_error) = new_set_errors(&final_name, &refs);
        err_name.set(name_error.clone());
        err_runs.set(runs_error.clone());
        err_form.set(None);
        if name_error.is_some() {
            name_ref.focus();
            return;
        }
        if runs_error.is_some() {
            return;
        }
        creating.set(true);
        let body = PreferenceSetCreate { name: final_name, run_ids: selected.get_untracked() };
        let created = created.clone();
        call(
            move |api| async move { api.create_preference_set(&body).await },
            move |result| {
                creating.try_set(false);
                match result {
                    Ok(set) => {
                        created(set.clone());
                        open.set(false);
                        nav::push(Route::PreferenceSet { id: set.id }, Some(set.name.clone()));
                    }
                    Err(e) => {
                        if let Some(m) = e.fields.get("name") {
                            err_name.try_set(Some(m.clone()));
                            name_ref.focus();
                        } else if let Some(m) = e.fields.get("run_ids") {
                            err_runs.try_set(Some(m.clone()));
                        } else {
                            err_form.try_set(Some(e.message));
                        }
                    }
                }
            },
        );
    };

    let toggle = move |run: &RunOut, on: bool| {
        err_runs.set(None);
        err_form.set(None);
        let id = run.id;
        selected.update(|s| {
            if on {
                if !s.contains(&id) {
                    s.push(id);
                }
            } else {
                s.retain(|x| *x != id);
            }
        });
    };

    let name_field = text_field(name)
        .placeholder("Qwen3-32B vs Llama-3.1-8B")
        .invalid(move || err_name.with(Option::is_some))
        .element(&name_ref)
        .on_change(move |t: &str| {
            touched.set(true);
            if t.chars().count() > NAME_MAX {
                name.set(t.chars().take(NAME_MAX).collect());
            }
            err_name.set(None);
        })
        .on_submit(submit.clone());

    let runs_box = dynamic(move || -> AnyElement {
        let state = runs.with(|l| match l {
            Load::Loading => 0,
            Load::Failed(_) => 1,
            Load::Loaded(v) if v.is_empty() => 2,
            Load::Loaded(_) => 3,
        });
        match state {
            0 => runs_skeleton().into_element(),
            1 => {
                let message = runs.with(|l| l.error().unwrap_or_default().to_owned());
                error_block(message, Some(Rc::new(move || {
                    runs.set(Load::Loading);
                    load_runs();
                })))
                .into_element()
            }
            2 => view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(4.0))
                .rounded(px(radius::XL))
                .bg(dialog_muted)
                .p(px(16.0))
                .child(body(text("No runs yet.")).text_color(muted_ink))
                .child(
                    label(text("New run"))
                        .underline()
                        .role(AccessibilityRole::Link)
                        .cursor(Cursor::Pointer)
                        .on_press(move || {
                            open.set(false);
                            nav::push(Route::NewRun, None);
                        }),
                )
                .into_element(),
            _ => run_groups(all_runs(), selected, toggle).into_element(),
        }
    });

    let hint = dynamic(move || -> AnyElement {
        match err_runs.get() {
            Some(m) => body(text(m)).px(px(12.0)).text_color(token("negative-ink")).role(AccessibilityRole::Alert).into_element(),
            None => {
                let picked = selected_runs();
                meta(text(runs_hint(&picked.iter().collect::<Vec<_>>()))).px(px(12.0)).into_element()
            }
        }
    });

    let s2 = submit.clone();
    view()
        .gap(px(16.0))
        .min_h(px(0.0))
        .flex_shrink(1.0)
        .child(dialog_header("New preference set", None))
        // The scroll area runs to the dialog's edges so focus rings are not clipped.
        .child(
            scroll().mx(px(-16.0)).flex_shrink(1.0).min_h(px(0.0)).child(
                view()
                    .px(px(16.0))
                    .py(px(4.0))
                    .gap(px(20.0))
                    .child(form_group(vec![form_row("Name", name_field).error(err_name).into_element()]).in_dialog())
                    .child(
                        view()
                            .gap(px(6.0))
                            .min_w(px(0.0))
                            .role(AccessibilityRole::Group)
                            .label("Runs")
                            .child(body(text("Runs")).px(px(12.0)).font_weight(FontWeight::SEMIBOLD).text_color(muted_ink))
                            .child(runs_box)
                            .child(hint),
                    )
                    .child(dynamic(move || err_form.get().map(|m| error_block(m, None)))),
            ),
        )
        .child(dialog_footer(vec![
            button("Cancel").variant(Variant::Secondary).on_press(close),
            button("Create preference set").busy(move || creating.get(), "Creating…").on_press(move || s2()),
        ]))
}

/// The runs, grouped by prompt set under its name and tile, in a 320 px scrolling box.
fn run_groups(runs: Vec<RunOut>, selected: Signal<Vec<i64>>, toggle: impl Fn(&RunOut, bool) + Copy + 'static) -> impl IntoElement {
    let groups = group_runs(&runs);
    // The prompt set of the first run picked: the others cannot be combined with it.
    let locked = move || {
        let first = selected.with(|s| s.first().copied());
        first.and_then(|id| runs.iter().find(|r| r.id == id).map(|r| r.prompt_set_id))
    };
    let locked = Rc::new(locked);
    scroll().max_h(px(320.0)).rounded(px(radius::XL)).bg(dialog_muted).child(
        view().px(px(8.0)).pb(px(8.0)).children(
            groups
                .into_iter()
                .map(|g| {
                    let set_id = g.prompt_set_id;
                    let header = view()
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .pt(px(12.0))
                        .pb(px(6.0))
                        .child(plasma(&format!("prompt-set-{set_id}"), 5, 20.0, radius::XS, false))
                        .child(label(text(g.prompt_set_name.clone())).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)));
                    let rows = g
                        .runs
                        .into_iter()
                        .map(|run| {
                            let locked = locked.clone();
                            run_row(run, selected, move || locked().is_some_and(|l| l != set_id), toggle).into_element()
                        })
                        .collect::<Vec<_>>();
                    view().child(header).child(view().gap(px(2.0)).children(rows)).into_element()
                })
                .collect::<Vec<_>>(),
        ),
    )
}

/// One run: the checkbox, its name over its model, and on the right how many samples are done
/// and its samples per prompt (or its status while it runs). The whole row toggles it.
fn run_row(
    run: RunOut,
    selected: Signal<Vec<i64>>,
    blocked: impl Fn() -> bool + 'static,
    toggle: impl Fn(&RunOut, bool) + 'static,
) -> impl IntoElement {
    let id = run.id;
    let empty = run.done == 0;
    let checked = move || selected.with(|s| s.contains(&id));
    let blocked = Rc::new(blocked);
    let b1 = blocked.clone();
    let off = move || empty || b1();
    let off2 = off.clone();
    let off3 = off.clone();
    let off4 = off.clone();
    let c1 = checked;
    let c2 = checked;
    let right: AnyElement = if empty {
        body(text("No finished samples")).text_color(muted_ink).into_element()
    } else {
        view()
            .items_end()
            .gap(px(2.0))
            .child(body(text(format!("{} done", int(run.done)))).tabular_nums())
            .child(if run.status == RunStatus::Completed {
                meta(text(format!("{} per prompt", count(run.params.samples_per_prompt, "sample", "samples")))).tabular_nums().into_element()
            } else {
                run_status_pill(run.status).into_element()
            })
            .into_element()
    };
    let r = run.clone();
    base_checkbox()
        .checked(move || c1())
        .disabled(move || off2())
        .on_change(move |next: &bool| toggle(&r, *next))
        .accessibility_label(run.name.clone())
        .flex_row()
        .items_start()
        .gap(px(12.0))
        .min_h(px(44.0))
        .px(px(8.0))
        .py(px(8.0))
        .rounded(px(radius::SM))
        .cursor(Cursor::Pointer)
        .bg(move || if c2() { theme().accent } else { AdaptiveColor::TRANSPARENT })
        .hover(move |s| {
            let off4 = off4.clone();
            s.bg(move || if off4() && !c2() { AdaptiveColor::TRANSPARENT } else { theme().accent })
        })
        .opacity(move || if off3() { 0.6_f32 } else { 1.0 })
        .disabled_style(|s| s.cursor(Cursor::NotAllowed))
        .indicator_style(|s| {
            s.mt(px(2.0))
                .w(px(16.0))
                .h(px(16.0))
                .items_center()
                .justify_center()
                .rounded(px(radius::XS))
                .border_1()
                .border_color(move || theme().input)
                .bg(move || theme().background)
                .checked(|s| s.bg(move || theme().primary).border_color(move || theme().primary))
        })
        .indicator(view().opacity(move || if checked() { 1.0_f32 } else { 0.0 }).child(icon(icon::CHECK, 14.0, move || theme().primary_foreground)))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .gap(px(2.0))
                .child(body(text(run.name.clone())))
                .child(mono(text(run.model.clone())).text_color(muted_ink)),
        )
        .child(view().flex_none().items_end().child(right))
}

/// Loading: three rows of bars in the box.
fn runs_skeleton() -> impl IntoElement {
    let ground = move || theme().background;
    view()
        .gap(px(4.0))
        .rounded(px(radius::XL))
        .bg(dialog_muted)
        .p(px(8.0))
        .role(AccessibilityRole::Progressbar)
        .label("Loading runs…")
        .children(
            (0..3)
                .map(move |_| {
                    view()
                        .h(px(48.0))
                        .flex_row()
                        .items_center()
                        .gap(px(12.0))
                        .px(px(8.0))
                        .child(view().w(px(16.0)).h(px(16.0)).rounded(px(radius::XS)).bg(ground))
                        .child(view().w(px(192.0)).h(px(16.0)).rounded(px(radius::XS)).bg(ground))
                        .child(view().flex_1())
                        .child(view().w(px(64.0)).h(px(16.0)).rounded(px(radius::XS)).bg(ground))
                        .into_element()
                })
                .collect::<Vec<_>>(),
        )
}
