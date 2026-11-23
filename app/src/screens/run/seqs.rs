//! The run's one accent control (frontend/src/components/runs/seqs-control.tsx, seqs-choices.tsx):
//! a periwinkle field for a run that is going or paused, in the throughput card's header over the
//! seqs step line it moves. Each change shows at once and is sent after 400 ms of quiet; a failure
//! puts the old value back and says why. The common values sit beside it as one segmented row.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use nsh_base::theme;
use nsh_core::element::input;
use nsh_core::prelude::*;

use crate::api::types::RunOut;
use crate::data::call;
use crate::format::int;
use crate::theme::text::{body, meta};
use crate::theme::{muted_ink, radius, token};
use crate::ui::button::{Button, Size, Variant};
use crate::ui::icon;
use crate::ui::overlay::toast_error_with;
use crate::ui::page::{column_padding, window_width};
use crate::ui::stepper::{parse_int, seqs_choices};

const MIN: i64 = 1;
const MAX: i64 = 4096;

/// How long the value stays unchanged before it is sent.
const QUIET: Duration = Duration::from_millis(400);

/// A draft as a seqs value: a whole number from 1 to 4,096, or `None`.
pub fn parse_seqs(draft: &str) -> Option<i64> {
    let t = draft.trim();
    if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse::<i64>().ok().filter(|n| (MIN..=MAX).contains(n))
}

/// Whether a draft is something other than a valid value (empty is not invalid: it is unfinished).
pub fn invalid_seqs(draft: &str) -> bool {
    !draft.trim().is_empty() && parse_seqs(draft).is_none()
}

/// The seqs control of run `run_id`. `concurrency` is the value the screen shows (reactive);
/// `show` sets the value the screen shows before the server confirms it (`None`: the server's);
/// `saved` gets the run the server sent back.
pub fn seqs_control(
    run_id: i64,
    concurrency: impl Fn() -> i64 + 'static,
    show: impl Fn(Option<i64>) + 'static,
    saved: impl Fn(RunOut) + 'static,
) -> impl IntoElement {
    let concurrency: Rc<dyn Fn() -> i64> = Rc::new(concurrency);
    let show = Rc::new(show);
    let saved = Rc::new(saved);
    let draft = signal(untracked(|| concurrency()).to_string());
    let editing = signal(false);
    // A change waiting out the quiet time (its number, so a newer one cancels it), and a request
    // on its way.
    let waiting = signal(0_u64);
    let next_wait = Rc::new(Cell::new(0_u64));
    let sending = signal(false);
    // The last value the server confirmed.
    let settled = Rc::new(Cell::new(untracked(|| concurrency())));

    // Follow the server while nothing is being typed or sent.
    {
        let settled = settled.clone();
        let concurrency = concurrency.clone();
        effect(move || {
            let c = concurrency();
            if !editing.get() && waiting.get() == 0 && !sending.get() {
                draft.set(c.to_string());
                settled.set(c);
            }
        });
    }

    let send: Rc<dyn Fn(i64)> = {
        let settled = settled.clone();
        let show = show.clone();
        Rc::new(move |n: i64| {
            sending.set(true);
            let settled = settled.clone();
            let show = show.clone();
            let saved = saved.clone();
            call(
                move |api| async move { api.update_run(run_id, &serde_json::json!({ "concurrency": n })).await },
                move |result| {
                    match result {
                        Ok(run) => {
                            settled.set(run.concurrency);
                            show(None);
                            saved(run);
                        }
                        Err(e) => {
                            let back = settled.get();
                            draft.set(back.to_string());
                            show(None);
                            toast_error_with(format!("Couldn’t change seqs. {}", e.message), format!("Seqs stays at {}.", int(back)));
                        }
                    }
                    sending.set(false);
                },
            );
        })
    };

    let apply: Rc<dyn Fn(i64)> = Rc::new(move |n: i64| {
        let next = n.clamp(MIN, MAX);
        // Waiting first: while it waits the field does not follow the shown value (which is about
        // to become this one), so the last confirmed value stays the settled one.
        let id = next_wait.get() + 1;
        next_wait.set(id);
        waiting.set(id);
        draft.set(next.to_string());
        // The hint and the chart show the new value straight away.
        show(Some(next));
        let settled = settled.clone();
        let send = send.clone();
        let show = show.clone();
        let _ = set_timeout(QUIET, move || {
            if waiting.get_untracked() != id {
                return;
            }
            // Decided before the wait ends: the end of the wait lets the field follow the shown
            // value again, which would make it the settled one.
            let changed = next != settled.get();
            if changed {
                send(next);
            }
            waiting.set(0);
            if !changed {
                show(None);
            }
        });
    });

    // The number typed so far (its leading digits, as `parseInt`), else the run's.
    let current = {
        let concurrency = concurrency.clone();
        move || parse_int(&draft.get()).unwrap_or_else(|| concurrency())
    };
    let invalid = move || draft.with(|d| invalid_seqs(d));

    let field_ref = element_ref();
    field_ref.on_focus_in(move || editing.set(true));
    {
        let concurrency = concurrency.clone();
        field_ref.on_focus_out(move || {
            editing.set(false);
            let d = draft.get_untracked();
            if invalid_seqs(&d) || d.trim().is_empty() {
                draft.set(untracked(|| concurrency()).to_string());
            }
        });
    }
    let typed = apply.clone();
    let field = input()
        .value(move || draft.get())
        .on_change(move |t: &str| {
            draft.set(t.to_owned());
            if let Some(n) = parse_seqs(t) {
                typed(n);
            }
        })
        .on_submit(move |_: &str| field_ref.blur())
        .element_ref(&field_ref)
        .keyboard_type(KeyboardType::Numeric)
        .w(px(64.0))
        .h(px(32.0))
        .px(px(8.0))
        .rounded(px(radius::MD))
        .border_1()
        .border_color(move || if invalid() { theme().destructive } else { token("periwinkle")() })
        .bg(move || theme().background)
        .text_size(px(13.0))
        .line_height(px(18.0))
        .font_weight(FontWeight::MEDIUM)
        .text_center()
        .tabular_nums()
        .text_color(token("periwinkle-ink"))
        .caret_color(move || resolved(token("periwinkle")()))
        .focus(move |s| {
            s.ring(px(3.0))
                .ring_color(move || if invalid() { theme().destructive.opacity(0.2) } else { token("ring")().opacity(0.25) })
        })
        .label("Seqs")
        .auto_correct(false);

    let (c1, c2, c3, c4) = (current.clone(), current.clone(), current.clone(), current.clone());
    let down = apply.clone();
    let up = apply.clone();
    let pick = apply.clone();
    let stepper = view()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .child(
            Button::icon(icon::MINUS, "Decrease seqs")
                .variant(Variant::Secondary)
                .on_card()
                .size(Size::Icon)
                .disabled(move || c1() <= MIN)
                .on_press(move || down(c2() - 1)),
        )
        .child(field)
        .child(
            Button::icon(icon::PLUS, "Increase seqs")
                .variant(Variant::Secondary)
                .on_card()
                .size(Size::Icon)
                .disabled(move || c3() >= MAX)
                .on_press(move || up(c4() + 1)),
        );

    view()
        .gap(px(6.0))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .child(body(text("Seqs")).text_color(muted_ink).mr(px(4.0)))
                .child(stepper)
                // Below 640 px the common values take a row of their own, as wide as the card's
                // content (`w-full`).
                .child(view().child(seqs_choices(move || current(), move |n| pick(n))).w(move || {
                    let w = window_width();
                    if w < 640.0 { Length::from(px(w - 2.0 * column_padding() - 32.0)) } else { Length::Auto }
                })),
        )
        .child(dynamic(move || {
            invalid().then(|| meta(text(format!("Enter a whole number from {MIN} to {}.", int(MAX)))).text_color(token("negative-ink")))
        }))
}

/// `color` in the appearance shown.
fn resolved(color: AdaptiveColor) -> nsh_core::color::Hsla {
    if dark() { color.dark } else { color.light }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seqs_are_whole_numbers_from_1_to_4096() {
        assert_eq!(parse_seqs("16"), Some(16));
        assert_eq!(parse_seqs(" 4096 "), Some(4096));
        assert_eq!(parse_seqs("0"), None);
        assert_eq!(parse_seqs("4097"), None);
        assert_eq!(parse_seqs("1.5"), None);
        assert_eq!(parse_seqs("-2"), None);
        assert!(invalid_seqs("abc"));
        assert!(!invalid_seqs(""));
        assert!(!invalid_seqs("8"));
    }
}
