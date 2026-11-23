//! One sample in full, in a side sheet up to 720 px wide (frontend/src/components/runs/
//! sample-viewer.tsx): the prompt, the thinking trace (open when short), the response and the
//! details, each a card with its copy button; the ‹ › pair and J and K step through the rows of
//! the table's page; Rendered | Raw at the header's end.
//!
//! A sample of a run with tools shows its trace in order between the prompt and the response: for
//! each request that called tools, a line naming it ("Request 1 of 3 · 2.1 s"), its thinking
//! (folded unless short), what the model wrote, and a card per tool call with its result
//! (`ui::trace`); then the last request's thinking and the response. A sample cut off by its rounds
//! of tool calls says it has no answer.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::data::{finish_reason_text, role_label, sample_label};
use super::step_pair::{Step, step_pair};
use crate::api::types::{SampleOut, SampleStatus};
use crate::data::{Load, call};
use crate::format::{DASH, count, date_time, duration, int};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::{body, meta, mono, title};
use crate::theme::{muted_ink, radius, token};
use crate::ui::blocks::error_block;
use crate::ui::icon::{self, icon};
use crate::ui::material::dialog_muted;
use crate::ui::model_text::{ModelTextStyle, model_text_with, text_mode_toggle};
use crate::ui::overlay::{sheet_close, side_sheet_width};
use crate::ui::page::window_width;
use crate::ui::status::{Tone, status_badge};
use crate::ui::tooltip::copy_button;
use crate::ui::trace::{Round, split_final, tool_call_card, trace_rounds};

/// A thinking trace shorter than this opens by itself.
const SHORT_TRACE: usize = 1500;

/// Model text in the sheet: the sheet's 13 px with the reading line height (1.6), in ink or, for a
/// thinking trace, muted.
fn sheet_text(source: String, muted: bool) -> impl IntoElement {
    let style = ModelTextStyle { size: 13.0, line_height: 20.8, muted: Rc::new(move || muted) };
    model_text_with(move || source.clone(), None, style)
}

/// The viewer, open while `open` holds, showing sample `id`. `prev` and `next` are the samples
/// before and after it on the page; `step` shows another.
pub fn sample_viewer(
    open: Signal<bool>,
    id: Signal<Option<i64>>,
    samples_per_prompt: impl Fn() -> i64 + 'static,
    prev: impl Fn() -> Option<i64> + Clone + 'static,
    next: impl Fn() -> Option<i64> + Clone + 'static,
    step: impl Fn(i64) + Clone + 'static,
) -> impl IntoElement {
    let sample = signal(Load::<SampleOut>::Loading);
    let load = move |want: i64| {
        call(
            move |api| async move { api.sample(want).await },
            move |r| {
                if id.get_untracked() == Some(want) {
                    sample.set(Load::from(r));
                }
            },
        );
    };
    effect(move || {
        if let Some(want) = id.get() {
            // A sample already shown stays until another is asked for.
            if sample.with_untracked(|s| s.loaded().map(|x| x.id)) != Some(want) {
                sample.set(Load::Loading);
            }
            load(want);
        }
    });
    let spp = Rc::new(samples_per_prompt);
    side_sheet_width(open, "Sample", 720.0, move || {
        let shown = move || sample.with(|s| s.loaded().filter(|x| Some(x.id) == id.get()).cloned());
        let spp = spp.clone();
        let (p1, p2, n1, n2) = (prev.clone(), prev.clone(), next.clone(), next.clone());
        let (s1, s2) = (step.clone(), step.clone());
        let wide = window_width() >= 640.0;
        let header = view()
            .gap(px(8.0))
            .px(px(if wide { 20.0 } else { 16.0 }))
            .pt(px(14.0))
            .pb(px(12.0))
            // From 640 px the sides are 20 px (the web UI's `sm:px-5` wins over its `pr-14`).
            .pr(px(if wide { 20.0 } else { 56.0 }))
            .border_b_1()
            .border_color(move || theme().border)
            .child(dynamic(move || {
                let s = shown();
                let label = s.as_ref().map(|s| sample_label(s.prompt_idx, s.sample_idx, spp())).unwrap_or_default();
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(12.0))
                    .gap_y(px(4.0))
                    .child(
                        title(text("Sample "))
                            .role(AccessibilityRole::Header)
                            .child(text(label).font_family(ROBOTO_MONO.family).text_size(px(13.0)).letter_spacing(px(0.0))),
                    )
                    .child(s.map(|s| sample_badge(&s)))
            }))
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(step_pair(
                        Step {
                            label: "Previous sample",
                            shortcut: Some("k"),
                            disabled: Rc::new(move || p1().is_none()),
                            on_press: Rc::new(move || {
                                if let Some(i) = p2() {
                                    s1(i)
                                }
                            }),
                        },
                        Step {
                            label: "Next sample",
                            shortcut: Some("j"),
                            disabled: Rc::new(move || n1().is_none()),
                            on_press: Rc::new(move || {
                                if let Some(i) = n2() {
                                    s2(i)
                                }
                            }),
                        },
                    ))
                    .child(view().flex_1())
                    .child(text_mode_toggle()),
            );
        let content = dynamic(move || -> AnyElement {
            if let Load::Failed(message) = sample.get() {
                let retry = move || {
                    if let Some(want) = id.get_untracked() {
                        sample.set(Load::Loading);
                        load(want);
                    }
                };
                return error_block(format!("Couldn’t load this sample. {message}"), Some(Rc::new(retry))).into_element();
            }
            match shown() {
                Some(s) => sample_body(s).into_element(),
                None => loading().into_element(),
            }
        });
        view()
            .flex_1()
            .min_h(px(0.0))
            .child(header)
            .child(scroll().flex_1().min_h(px(0.0)).child(view().p(px(if wide { 16.0 } else { 12.0 })).child(content)))
            .child(sheet_close(open))
            .into_element()
    })
}

/// The sample's status in the header: a capsule in its state's tint.
fn sample_badge(s: &SampleOut) -> View {
    if s.status == SampleStatus::Done && crate::ui::status::is_truncated(s.finish_reason.as_deref()) {
        return status_badge(Tone::Attention, None, "Truncated");
    }
    match s.status {
        SampleStatus::Failed => status_badge(Tone::Negative, None, "Failed"),
        SampleStatus::Running => status_badge(Tone::Positive, None, "In flight"),
        SampleStatus::Done => status_badge(Tone::Plain, None, "Done"),
        SampleStatus::Pending => status_badge(Tone::Plain, None, "Pending"),
    }
}

/// A card: the sheet's `muted` (the ground in light), 16 px corners, 8 px padding.
fn card() -> View {
    view().rounded(px(radius::XL)).bg(dialog_muted).p(px(8.0))
}

/// The model text's well inside a card: the ground, 8 px corners, concentric with the card.
fn well() -> View {
    view().mt(px(4.0)).rounded(px(radius::SM)).bg(move || theme().background).px(px(12.0)).py(px(10.0))
}

/// A card's header row: its title (and meta), and the copy button at its end.
fn card_header(heading: impl IntoElement, copy: Option<(String, &'static str)>) -> impl IntoElement {
    view()
        .min_h(px(32.0))
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .pl(px(8.0))
        .pr(px(2.0))
        .child(heading)
        .child(copy.map(|(value, label)| copy_button(value, label)))
}

/// A card title at 13 px 600, with mono meta after it.
fn card_title(words: &'static str, meta_words: Option<String>) -> Text {
    body(text(words))
        .font_weight(FontWeight::SEMIBOLD)
        .role(AccessibilityRole::Header)
        .child(meta_words.map(|m| mono(text(format!("  {m}"))).font_weight(FontWeight::NORMAL).text_color(muted_ink)))
}

/// A message's text: its content when it is text, else its JSON.
fn content_of(m: &serde_json::Value) -> (String, String) {
    let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("").to_owned();
    let content = match m.get("content") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    (role, content)
}

/// The prompt as the copy button copies it: each message, with its role when there are several.
pub fn prompt_text(messages: &[serde_json::Value]) -> String {
    let many = messages.len() > 1;
    messages
        .iter()
        .map(|m| {
            let (role, content) = content_of(m);
            if many { format!("{}:\n{content}", role_label(&role)) } else { content }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The cards of a loaded sample.
fn sample_body(s: SampleOut) -> impl IntoElement {
    let many = s.messages.len() > 1;
    let messages: Vec<AnyElement> = s
        .messages
        .iter()
        .map(|m| {
            let (role, content) = content_of(m);
            view()
                .gap(px(4.0))
                .child(many.then(|| meta(text(role_label(&role)))))
                .child(sheet_text(content, false))
                .into_element()
        })
        .collect();
    let prompt = card()
        .child(card_header(card_title("Prompt", None), Some((prompt_text(&s.messages), "Copy prompt"))))
        .child(well().gap(px(12.0)).children(messages));

    let mut thinking = s.reasoning.clone().filter(|r| !r.is_empty()).map(|r| thinking_card(r, s.reasoning_tokens, "Thinking trace").into_element());

    // A run with tools: the requests that called tools, then the last one's line before its
    // thinking. A sample cut off by its rounds has no last request of its own.
    let mut steps: Vec<AnyElement> = Vec::new();
    let mut request_line = None;
    if let Some(trace) = s.trace.as_ref().filter(|t| t.iter().any(|m| matches!(m, crate::api::types::TraceMessage::Tool(_)))) {
        let (rounds, last) = split_final(trace_rounds(trace));
        let total = rounds.len() + usize::from(last.is_some());
        steps = rounds.into_iter().flat_map(|r| round_cards(r, total)).collect();
        match last {
            Some(last) => request_line = Some(round_line(&last, total)),
            None => thinking = None,
        }
    }
    let cut_by_rounds = s.finish_reason.as_deref() == Some("tool_rounds");

    let response = s.response.clone().filter(|r| !r.is_empty());
    let response_body: AnyElement = match &response {
        Some(r) => {
            sheet_text(r.clone(), false).into_element()
        }
        None if s.status == SampleStatus::Failed => {
            body(text(s.error.clone().unwrap_or_else(|| "The request failed.".into()))).text_color(token("negative-ink")).into_element()
        }
        None if s.status == SampleStatus::Done && s.finish_reason.as_deref() == Some("length") => {
            body(text("No answer: max tokens hit while thinking.")).text_color(token("coral-ink")).into_element()
        }
        None if s.status == SampleStatus::Done && cut_by_rounds => {
            body(text("No answer: the sample used all its rounds of tool calls.")).text_color(token("coral-ink")).into_element()
        }
        None if s.status == SampleStatus::Done => body(text("The model returned an empty response.")).text_color(muted_ink).into_element(),
        None => body(text("Not finished yet.")).text_color(muted_ink).into_element(),
    };
    let response_card = card()
        .child(card_header(card_title("Response", None), response.map(|r| (r, "Copy response"))))
        .child(well().child(response_body));

    let details = card().child(card_header(card_title("Details", None), None)).child(facts(&s));

    view()
        .gap(px(8.0))
        .child(prompt)
        .children(steps)
        .child(request_line)
        .child(thinking)
        .child(response_card)
        .child(details)
}

/// The line over a request's cards: "Request 2 of 3 · 1.4 s · 212 tokens", 12 px muted.
fn round_line(round: &Round, total: usize) -> AnyElement {
    let mut facts = vec![format!("Request {} of {}", int(round.n as i64), int(total as i64)), duration(Some(round.latency_ms as f64))];
    if let Some(t) = round.completion_tokens {
        facts.push(count(t, "token", "tokens"));
    }
    meta(text(facts.join(" · "))).tabular_nums().px(px(8.0)).pt(px(if round.n > 1 { 8.0 } else { 4.0 })).into_element()
}

/// One request that called tools: the line that names it, its thinking, what the model wrote, and
/// a card per call with its result.
fn round_cards(round: Round, total: usize) -> Vec<AnyElement> {
    let mut out = vec![round_line(&round, total)];
    if let Some(r) = round.reasoning.clone() {
        out.push(thinking_card(r, round.reasoning_tokens, "Thinking").into_element());
    }
    if !round.text.trim().is_empty() {
        let words = round.text.clone();
        out.push(
            card()
                .child(card_header(card_title("Assistant", None), Some((words.clone(), "Copy text"))))
                .child(well().child(sheet_text(words, false)))
                .into_element(),
        );
    }
    // Many calls in a request open folded, so a long trace stays short to scroll.
    let open = round.calls.len() <= 4;
    out.extend(round.calls.into_iter().map(|c| tool_call_card(c, open).into_element()));
    out
}

/// The thinking trace: a card that opens and closes, open by itself when short. `tokens` sizes it
/// (its characters without them); `heading` names it.
fn thinking_card(trace: String, tokens: Option<i64>, heading: &'static str) -> impl IntoElement {
    let open = signal(trace.chars().count() < SHORT_TRACE);
    let words = match tokens {
        Some(n) => count(n, "token", "tokens"),
        None => format!("{} characters", int(trace.chars().count() as i64)),
    };
    let trigger = view()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .mx(px(-4.0))
        .px(px(4.0))
        .rounded(px(radius::XS))
        .role(AccessibilityRole::Button)
        .label(heading)
        .accessibility_state(move || AccessibilityState { expanded: Some(open.get()), ..AccessibilityState::default() })
        .focusable(true)
        .focus_visible(|st| st.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .on_press(move || open.update(|o| *o = !*o))
        .child(icon(icon::CHEVRON_RIGHT, 16.0, move || theme().muted_foreground).rotate(move || if open.get() { 90.0_f32.to_radians() } else { 0.0 }))
        .child(body(text(heading)).font_weight(FontWeight::SEMIBOLD))
        .child(mono(text(words)).text_color(muted_ink));
    let copy = trace.clone();
    card()
        .child(card_header(trigger, Some((copy, "Copy thinking trace"))))
        .child(when(move || open.get(), move || {
            well().child(sheet_text(trace.clone(), true))
        }))
}

/// The details: finish reason, tokens, latency, time to first token, attempts, when it ended. A
/// failed request has no finish reason, tokens or timings: those rows are left out.
fn facts(s: &SampleOut) -> impl IntoElement {
    let failed = s.status == SampleStatus::Failed;
    let finish = finish_reason_text(s.finish_reason.as_deref());
    let tokens: Vec<String> = [
        s.prompt_tokens.map(|n| format!("{} prompt", int(n))),
        s.completion_tokens.map(|n| format!("{} completion", int(n))),
        s.reasoning_tokens.map(|n| format!("{} reasoning", int(n))),
    ]
    .into_iter()
    .flatten()
    .collect();
    // (label, value, known, mono, warn)
    let all: Vec<(&'static str, String, bool, bool, bool)> = vec![
        (
            "Finish reason",
            finish.as_ref().map(|f| f.0.clone()).unwrap_or_else(|| DASH.into()),
            finish.is_some(),
            false,
            finish.as_ref().is_some_and(|f| f.1),
        ),
        ("Tokens", if tokens.is_empty() { DASH.into() } else { tokens.join(" · ") }, !tokens.is_empty(), true, false),
        ("Latency", duration(s.latency_ms.map(|m| m as f64)), s.latency_ms.is_some(), true, false),
        ("Time to first token", duration(s.ttft_ms.map(|m| m as f64)), s.ttft_ms.is_some(), true, false),
        ("Attempts", int(s.attempts), true, true, false),
        (
            if failed { "Failed" } else { "Completed" },
            s.completed_at.map(date_time).unwrap_or_else(|| DASH.into()),
            s.completed_at.is_some(),
            true,
            false,
        ),
    ];
    let rows: Vec<AnyElement> = all
        .into_iter()
        .filter(|(_, _, known, _, _)| *known || !failed)
        .map(|(k, v, _, is_mono, warn)| {
            let value = if is_mono {
                mono(text(v)).line_height(px(20.0)).into_element()
            } else {
                body(text(v)).text_color(move || if warn { token("coral-ink")() } else { theme().foreground }).into_element()
            };
            view()
                .flex_row()
                .items_start()
                .gap(px(16.0))
                .child(body(text(k)).text_color(muted_ink).pt(px(1.0)).w(px(160.0)).flex_none())
                .child(view().flex_1().min_w(px(0.0)).child(value))
                .into_element()
        })
        .collect();
    view().gap(px(8.0)).px(px(8.0)).pt(px(4.0)).pb(px(8.0)).children(rows)
}

/// The cards' frames with bars while the sample loads.
fn loading() -> impl IntoElement {
    let bars = || {
        let line = |share: f32| {
            view()
                .flex_row()
                .child(view().flex_grow(share).basis(px(0.0)).h(px(16.0)).rounded(px(radius::XS)).bg(move || theme().secondary))
                .child(view().flex_grow(1.0 - share).basis(px(0.0)))
        };
        well().gap(px(8.0)).child(line(1.0)).child(line(11.0 / 12.0)).child(line(2.0 / 3.0))
    };
    view()
        .gap(px(8.0))
        .role(AccessibilityRole::Progressbar)
        .label("Loading sample")
        .child(card().child(card_header(card_title("Prompt", None), None)).child(bars()))
        .child(card().child(card_header(card_title("Response", None), None)).child(bars()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_copies_with_its_roles_when_it_has_several_messages() {
        let one = vec![serde_json::json!({"role": "user", "content": "Hi"})];
        assert_eq!(prompt_text(&one), "Hi");
        let two = vec![serde_json::json!({"role": "system", "content": "Be brief."}), serde_json::json!({"role": "user", "content": "Hi"})];
        assert_eq!(prompt_text(&two), "System:\nBe brief.\n\nUser:\nHi");
    }
}
