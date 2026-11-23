//! Auto-pair by run (frontend/src/components/preferences/auto-pair-dialog.tsx): a 448 px dialog
//! choosing the run whose responses are chosen and the one whose are rejected (picking the run on
//! the other side swaps them). When some decided prompts were paired by hand, a checkbox offers to
//! replace them, and the button turns destructive and says how many.

use std::rc::Rc;

use nsh_base::{checkbox as base_checkbox, theme};
use nsh_core::prelude::*;

use super::logic::{auto_pair_label, auto_pair_message};
use crate::api::types::{AutoPairIn, PreferenceSetOut as Set};
use crate::data::call;
use crate::format::count;
use crate::theme::text::{body, meta};
use crate::theme::{ink, radius, token};
use crate::ui::blocks::error_block;
use crate::ui::button::{Variant, button};
use crate::ui::field::{form_group, form_item, form_row};
use crate::ui::icon::{self, icon};
use crate::ui::overlay::{DialogWidth, dialog, dialog_footer, dialog_header, toast_info, toast_success};
use crate::ui::select::{option, select_field};

/// How many decided items one read asks for while counting hand-picked pairs.
const PAGE: i64 = 200;

/// The dialog, shown while `open` holds; it reads the set (`set`) as it opens. `done` runs after
/// pairs were made.
pub fn auto_pair_dialog(set: Rc<dyn Fn() -> Option<Set>>, open: Signal<bool>, done: Rc<dyn Fn()>) -> impl IntoElement {
    dialog(open, "Auto-pair by run", move || match untracked(|| set()) {
        Some(s) => form(s, open, done.clone()).into_element(),
        None => view().into_element(),
    })
    .width(DialogWidth::Md)
}

fn form(set: Set, open: Signal<bool>, done: Rc<dyn Fn()>) -> impl IntoElement {
    let id = set.id;
    let first = set.runs.first().map(|r| SharedString::from(r.id.to_string()));
    let second = set.runs.get(1).map(|r| SharedString::from(r.id.to_string()));
    let chosen = signal(first.clone());
    let rejected = signal(second.clone());
    let overwrite = signal(false);
    let pairing = signal(false);
    let error = signal(None::<String>);
    // Pairs picked by hand among the decided prompts (what "replace" would overwrite): `None` while
    // counting; 0 when nothing is decided.
    let manual = signal(if set.decided > 0 { None } else { Some(0_i64) });
    if set.decided > 0 {
        count_hand_picked(id, 0, 0, manual);
    }

    // Picking the run already on the other side swaps the two.
    let last = stored((first, second));
    effect(move || {
        let (c, r) = (chosen.get(), rejected.get());
        let (pc, pr) = last.get();
        if c != pc && c.is_some() && c == r {
            rejected.set(pc.clone());
            last.replace((c, pc));
            return;
        }
        if r != pr && r.is_some() && r == c {
            chosen.set(pr.clone());
            last.replace((pr, r));
            return;
        }
        last.replace((c, r));
    });

    let replacing = move || overwrite.get() && manual.get().is_some_and(|m| m != 0);
    let submit = {
        let done = done.clone();
        move || {
            if pairing.get_untracked() {
                return;
            }
            let (Some(c), Some(r)) = (chosen.get_untracked(), rejected.get_untracked()) else { return };
            let body = AutoPairIn {
                chosen_run_id: c.parse().unwrap_or_default(),
                rejected_run_id: r.parse().unwrap_or_default(),
                overwrite: overwrite.get_untracked() && manual.get_untracked() != Some(0),
            };
            pairing.set(true);
            error.set(None);
            let done = done.clone();
            call(
                move |api| async move { api.auto_pair(id, &body).await },
                move |result| {
                    pairing.try_set(false);
                    match result {
                        Ok(out) => {
                            match auto_pair_message(out.created, out.skipped) {
                                Some(m) => toast_success(m),
                                None => toast_info("No new pairs"),
                            }
                            done();
                            open.set(false);
                        }
                        Err(e) => {
                            error.try_set(Some(e.message));
                        }
                    }
                },
            );
        }
    };

    let runs = set.runs.clone();
    let option_runs = set.runs.clone();
    let options = move || option_runs.iter().map(|r| option(r.id.to_string(), auto_pair_label(r))).collect::<Vec<_>>();
    let title_of = move |v: &str| runs.iter().find(|r| r.id.to_string() == v).map(auto_pair_label).unwrap_or_default();
    let title_of2 = title_of.clone();
    let chosen_field = select_field(options(), chosen, title_of, "", "Chosen run", on_dialog_control);
    let rejected_field = select_field(options(), rejected, title_of2, "", "Rejected run", on_dialog_control);

    let replace_row = move || -> Option<AnyElement> {
        let m = manual.get();
        if m == Some(0) {
            return None;
        }
        let words = m.map(|n| count(n, "pair", "pairs")).unwrap_or_else(|| "pairs".into());
        Some(form_item(overwrite_checkbox(overwrite, format!("Replace {words} picked by hand"))).py(px(10.0)).into_element())
    };

    let mut rows = vec![
        form_row("Chosen run", chosen_field).stacked().into_element(),
        form_row("Rejected run", rejected_field).stacked().into_element(),
    ];
    rows.push(dynamic(replace_row).into_element());

    let s2 = submit.clone();
    view()
        .gap(px(16.0))
        .child(dialog_header("Auto-pair by run", None))
        .child(
            form_group(rows)
                .footnote(meta(text("Each prompt pairs the first finished response of each run.")))
                .in_dialog(),
        )
        .child(dynamic(move || error.get().map(|m| error_block(m, None))))
        .child(dynamic(move || {
            let r = replacing();
            let label = if r { format!("Replace {}", count(manual.get().unwrap_or(0), "pair", "pairs")) } else { "Auto-pair prompts".into() };
            let s3 = s2.clone();
            dialog_footer(vec![
                button("Cancel").variant(Variant::Secondary).on_press(move || open.set(false)),
                button(label)
                    .variant(if r { Variant::Destructive } else { Variant::Primary })
                    .busy(move || pairing.get(), "Pairing…")
                    .on_press(move || s3()),
            ])
        }))
}

/// A pop-up button in a dialog's group: the raised tone in light, a lighter tone in dark.
fn on_dialog_control() -> AdaptiveColor {
    token("on-dialog-control")()
}

/// Counts the decided items paired by hand, 200 at a time, into `manual`.
fn count_hand_picked(id: i64, offset: i64, so_far: i64, manual: Signal<Option<i64>>) {
    call(
        move |api| async move { api.preference_items(id, "decided", offset, PAGE).await },
        move |result| {
            let Ok(page) = result else { return };
            let n = so_far + page.items.iter().filter(|it| it.source.as_deref() == Some("manual")).count() as i64;
            if offset + PAGE >= page.total || page.items.is_empty() {
                manual.try_set(Some(n));
            } else {
                count_hand_picked(id, offset + PAGE, n, manual);
            }
        },
    );
}

/// "Replace 3 pairs picked by hand" with its hint; checked, the box and the words turn red.
fn overwrite_checkbox(on: Signal<bool>, words: String) -> impl IntoElement {
    let red = move || on.get();
    base_checkbox()
        .checked(move || on.get())
        .on_change(move |next: &bool| on.set(*next))
        .accessibility_label(words.clone())
        .flex_row()
        .items_start()
        .gap(px(12.0))
        .cursor(Cursor::Pointer)
        .indicator_style(move |s| {
            s.mt(px(2.0))
                .w(px(16.0))
                .h(px(16.0))
                .items_center()
                .justify_center()
                .rounded(px(radius::XS))
                .border_1()
                .border_color(move || theme().input)
                .bg(move || theme().background)
                .checked(|s| s.bg(move || theme().destructive).border_color(move || theme().destructive))
        })
        .indicator(view().opacity(move || if on.get() { 1.0_f32 } else { 0.0 }).child(icon(icon::CHECK, 14.0, move || theme().primary_foreground)))
        .child(
            view()
                .gap(px(2.0))
                .child(body(text(words)).text_color(move || if red() { token("negative-ink")() } else { ink() }))
                .child(meta(text("Earlier auto-pairs are always replaced."))),
        )
}
