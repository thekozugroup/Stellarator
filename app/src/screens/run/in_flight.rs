//! The requests in flight (frontend/src/components/runs/in-flight.tsx; DESIGN.md, "Log rows"),
//! oldest first: the sample number and how long it has been going in mono, the state word in its
//! color, then the prompt. Under it, in muted text, the newest words the model streamed (two lines
//! at most) and a retry's attempt. The rows share columns: each as wide as its widest entry. From a
//! 384 px panel the token count sits at the end of each row; in a narrower one, under it.
//!
//! A run with tools: while a sample's tools run its state word is "tool" (in ink) and the line
//! under it says which ("calling lookup…") and which request of the sample it is ("round 2"); a
//! later request that thinks or answers says its round too. In code mode the word is "code" and
//! the line says "running code · lookup.search…" (the tool calls the code has in progress) with
//! how many calls it has made so far ("3 calls"), then the round.

use nsh_core::prelude::*;

use super::data::{clock, sample_label};
use crate::api::types::{LiveSequence, Phase};
use crate::format::{count, duration, int};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::{body, meta};
use crate::theme::{ink, muted_ink, token};
use crate::ui::tooltip::tooltip_bubble;

/// The width of a Roboto Mono character at 12 px.
const MONO_W: f32 = 7.2;
/// The space between columns (`gap-x-3`).
const COLUMN_GAP: f32 = 12.0;

/// The state word's color: thinking is the data color, answering green, waiting muted.
fn phase_color(phase: Phase) -> fn() -> AdaptiveColor {
    match phase {
        Phase::Thinking => || token("periwinkle-ink")(),
        Phase::Answering => || token("positive-ink")(),
        Phase::Tool | Phase::Code => ink,
        Phase::Waiting => muted_ink,
    }
}

fn phase_word(phase: Phase) -> &'static str {
    match phase {
        Phase::Thinking => "thinking",
        Phase::Answering => "answering",
        Phase::Waiting => "waiting",
        Phase::Tool => "tool",
        Phase::Code => "code",
    }
}

/// The tool line under a row: "calling lookup, search… · round 2" while its tools run, "round 2"
/// for a later request; nothing for a sample's first request.
pub fn tool_line(s: &LiveSequence) -> Option<String> {
    let round = (s.round > 1).then(|| format!("round {}", int(s.round)));
    if s.phase == Phase::Code {
        let live = s.code.clone().unwrap_or_default();
        let head = match live.running.len() {
            0 => "running code…".to_owned(),
            n if n <= 2 => format!("running code · {}…", live.running.join(", ")),
            n => format!("running code · {} +{}…", live.running[..2].join(", "), int(n as i64 - 2)),
        };
        let calls = (live.calls > 0).then(|| count(live.calls, "call", "calls"));
        return Some([Some(head), calls, round].into_iter().flatten().collect::<Vec<_>>().join(" · "));
    }
    if s.phase == Phase::Tool {
        let calling = if s.tools.is_empty() { "calling tools…".to_owned() } else { format!("calling {}…", s.tools.join(", ")) };
        return Some(match round {
            Some(r) => format!("{calling} · {r}"),
            None => calling,
        });
    }
    round
}

fn mono_text(words: impl Into<SharedString>) -> Text {
    text(words.into()).font_family(ROBOTO_MONO.family).text_size(px(12.0)).line_height(px(16.0)).letter_spacing(px(0.0)).number_of_lines(1)
}

/// The column widths of `sequences`: the label, the clock and the state word.
fn columns(sequences: &[LiveSequence], samples_per_prompt: i64) -> (f32, f32, f32) {
    let widest = |f: &dyn Fn(&LiveSequence) -> usize| sequences.iter().map(f).max().unwrap_or(0) as f32 * MONO_W;
    (
        widest(&|s| sample_label(s.prompt_idx, s.sample_idx, samples_per_prompt).chars().count()).ceil(),
        widest(&|s| clock(s.elapsed_ms).chars().count()).ceil(),
        widest(&|s| phase_word(s.phase).len()).ceil(),
    )
}

/// The log of `sequences`; `in_flight` counts all of them (the server sends the oldest 64). `wide`:
/// the panel is 384 px or wider.
pub fn in_flight_list(sequences: Vec<LiveSequence>, in_flight: i64, samples_per_prompt: i64, wide: bool) -> AnyElement {
    if sequences.is_empty() {
        return body(text("Nothing in flight.")).text_color(muted_ink).py(px(8.0)).into_element();
    }
    let (label_w, clock_w, phase_w) = columns(&sequences, samples_per_prompt);
    let shown = sequences.len() as i64;
    let rows: Vec<AnyElement> = sequences
        .into_iter()
        .map(|s| {
            let tokens = count(s.tokens, "token", "tokens");
            let line = view()
                .flex_row()
                .items_baseline()
                .gap(px(COLUMN_GAP))
                .child(mono_text(sample_label(s.prompt_idx, s.sample_idx, samples_per_prompt)).text_color(muted_ink).w(px(label_w)).flex_none())
                .child(
                    view()
                        .w(px(clock_w))
                        .flex_none()
                        .tooltip(tooltip_bubble(duration(Some(s.elapsed_ms as f64))))
                        .child(mono_text(clock(s.elapsed_ms)).text_color(muted_ink).text_right()),
                )
                .child(mono_text(phase_word(s.phase)).text_color(phase_color(s.phase)).w(px(phase_w)).flex_none())
                .child(body(text(s.prompt_preview.clone())).number_of_lines(1).flex_1().min_w(px(0.0)))
                .child(wide.then(|| mono_text(tokens.clone()).text_color(muted_ink).text_right().flex_none()));
            // Under the row: in a narrow panel the token count, a retry's attempt, and the newest
            // two lines streamed (older text leaves through the top edge).
            let tool = tool_line(&s);
            let facts = (!wide || s.attempt > 1 || tool.is_some()).then(|| {
                let in_tool = matches!(s.phase, Phase::Tool | Phase::Code);
                view()
                    .flex_row()
                    .gap(px(COLUMN_GAP))
                    .min_w(px(0.0))
                    .child(tool.map(|t| mono_text(t).text_color(move || if in_tool { ink() } else { muted_ink() }).flex_shrink(1.0).min_w(px(0.0))))
                    .child((!wide).then(|| mono_text(tokens.clone()).text_color(muted_ink)))
                    .child((s.attempt > 1).then(|| mono_text(format!("attempt {}", int(s.attempt))).text_color(token("coral-ink"))))
            });
            let tail = s.tail.trim_start().to_owned();
            let answering = s.phase == Phase::Answering;
            let streamed = (!tail.is_empty()).then(|| {
                view().max_h(px(41.6)).overflow_hidden().justify_end().child(
                    text(tail)
                        .text_size(px(13.0))
                        .line_height(px(20.8))
                        .text_color(move || if answering { ink() } else { muted_ink() })
                        .flex_none(),
                )
            });
            let under = (facts.is_some() || streamed.is_some()).then(|| {
                view()
                    .mt(px(2.0))
                    .ml(px(if wide { label_w + clock_w + phase_w + 3.0 * COLUMN_GAP } else { 0.0 }))
                    .child(facts)
                    .child(streamed)
            });
            view().py(px(6.0)).child(line).child(under).into_element()
        })
        .collect();
    view()
        .child(view().children(rows))
        .child((in_flight > shown).then(|| {
            meta(text(format!("Showing the {} oldest of {} in flight.", int(shown), int(in_flight)))).tabular_nums().mt(px(8.0))
        }))
        .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(phase: Phase, round: i64, tools: &[&str]) -> LiveSequence {
        serde_json::from_value(serde_json::json!({
            "sample_id": 1, "prompt_idx": 0, "sample_idx": 0, "phase": phase, "prompt_preview": "p", "tokens": 0,
            "tail": "", "elapsed_ms": 0, "attempt": 1, "round": round, "tools": tools
        }))
        .unwrap()
    }

    fn code_seq(round: i64, calls: i64, running: &[&str]) -> LiveSequence {
        let mut s = seq(Phase::Code, round, &["codemode"]);
        s.code = Some(crate::api::types::LiveCode { calls, running: running.iter().map(|r| (*r).to_owned()).collect() });
        s
    }

    #[test]
    fn a_code_phase_names_the_calls_in_progress_and_counts_them() {
        assert_eq!(tool_line(&code_seq(1, 3, &["lookup.search"])).as_deref(), Some("running code · lookup.search… · 3 calls"));
        assert_eq!(tool_line(&code_seq(2, 1, &["lookup.search", "lookup.fetch"])).as_deref(), Some("running code · lookup.search, lookup.fetch… · 1 call · round 2"));
        assert_eq!(tool_line(&code_seq(1, 7, &["a.x", "a.y", "b.z", "b.w"])).as_deref(), Some("running code · a.x, a.y +2… · 7 calls"));
        // Between two calls, or before the first: nothing in progress.
        assert_eq!(tool_line(&code_seq(1, 2, &[])).as_deref(), Some("running code… · 2 calls"));
        assert_eq!(tool_line(&code_seq(1, 0, &[])).as_deref(), Some("running code…"));
        let mut bare = seq(Phase::Code, 1, &[]);
        bare.code = None;
        assert_eq!(tool_line(&bare).as_deref(), Some("running code…"));
        assert_eq!(phase_word(Phase::Code), "code");
    }

    #[test]
    fn a_tool_phase_names_its_tools_and_round() {
        assert_eq!(tool_line(&seq(Phase::Tool, 2, &["lookup"])).as_deref(), Some("calling lookup… · round 2"));
        assert_eq!(tool_line(&seq(Phase::Tool, 1, &["lookup", "fetch"])).as_deref(), Some("calling lookup, fetch…"));
        assert_eq!(tool_line(&seq(Phase::Thinking, 3, &[])).as_deref(), Some("round 3"));
        assert_eq!(tool_line(&seq(Phase::Answering, 1, &[])), None);
        assert_eq!(phase_word(Phase::Tool), "tool");
    }
}
