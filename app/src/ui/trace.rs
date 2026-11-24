//! Tool traces (DESIGN.md, "Tool traces"): a sample of a run with tools is a list of requests
//! (rounds). Each round is the model's turn (its thinking, its text, its tool calls) and the
//! results the tools sent back; the last round without calls holds the answer.
//!
//! - [`trace_rounds`] groups a trace into rounds, each call with its result; [`split_final`] takes
//!   the answer's round off the end. Pure, with tests.
//! - [`tool_call_card`]: one call in the sample viewer: a card (16 px corners) whose header is the
//!   tool's name in mono at 500, the server muted, the call's latency and a copy button; open, the
//!   arguments as pretty-printed mono JSON in a ground well (as the model wrote them when they are
//!   not JSON), then the result: its content in mono in a well, the TRUNCATED tag when it was cut,
//!   the negative tint and ink when it is an error. A result over 20 lines (or 2,000 characters)
//!   shows its start and "Show all".
//! - [`compact_calls`]: the same trace in a preference card: folded to "3 tool calls · lookup,
//!   search", open a line per call (name, server, arguments) with the result's first lines.
//! - [`code_spans`]: text with `backticked` parts in mono.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::button::{Size, Variant, button};
use super::icon::{self, icon};
use super::material::{dialog_muted, dialog_well};
use super::model_text::{TextMode, text_mode};
use super::tooltip::copy_button;
use crate::api::types::{ToolCall, TraceMessage, TraceTool};
use crate::format::{count, duration, int};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::{body, meta, mono};
use crate::theme::{muted_ink, radius, token};

/// A result longer than this many lines shows its start until opened.
pub const RESULT_LINES: usize = 20;
/// ...or longer than this many characters (one long line of JSON).
pub const RESULT_CHARS: usize = 2000;

// ------------------------------------------------------------------ rounds

/// A tool call and the result the tool sent back (none when the sample stopped before running it).
#[derive(Debug, Clone, PartialEq)]
pub struct CallBlock {
    pub call: ToolCall,
    pub result: Option<TraceTool>,
}

/// One request of a sample: the model's turn and the results of its calls.
#[derive(Debug, Clone, PartialEq)]
pub struct Round {
    /// 1-based.
    pub n: usize,
    pub reasoning: Option<String>,
    pub reasoning_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    /// The text the model wrote in this turn.
    pub text: String,
    pub calls: Vec<CallBlock>,
    pub finish_reason: Option<String>,
    pub latency_ms: i64,
}

/// The trace in rounds, in order: each assistant turn with its calls, each call with the tool
/// message that answers it (by id; a result no call asked for joins the round as a call of its
/// own, with no arguments).
pub fn trace_rounds(trace: &[TraceMessage]) -> Vec<Round> {
    let mut rounds: Vec<Round> = Vec::new();
    for message in trace {
        match message {
            TraceMessage::Assistant(a) => rounds.push(Round {
                n: rounds.len() + 1,
                reasoning: a.reasoning.clone().filter(|r| !r.trim().is_empty()),
                reasoning_tokens: a.reasoning_tokens,
                completion_tokens: a.completion_tokens,
                text: a.content.clone(),
                calls: a.tool_calls.iter().map(|c| CallBlock { call: c.clone(), result: None }).collect(),
                finish_reason: a.finish_reason.clone(),
                latency_ms: a.latency_ms,
            }),
            TraceMessage::Tool(t) => {
                if rounds.is_empty() {
                    rounds.push(Round {
                        n: 1,
                        reasoning: None,
                        reasoning_tokens: None,
                        completion_tokens: None,
                        text: String::new(),
                        calls: Vec::new(),
                        finish_reason: None,
                        latency_ms: 0,
                    });
                }
                let round = rounds.last_mut().expect("a round");
                match round.calls.iter_mut().find(|c| c.call.id == t.tool_call_id && c.result.is_none()) {
                    Some(block) => block.result = Some(t.clone()),
                    None => round.calls.push(CallBlock {
                        call: ToolCall { id: t.tool_call_id.clone(), name: t.name.clone(), arguments: String::new() },
                        result: Some(t.clone()),
                    }),
                }
            }
        }
    }
    rounds
}

/// The rounds that called tools, and the last one when it called none (the answer's).
pub fn split_final(mut rounds: Vec<Round>) -> (Vec<Round>, Option<Round>) {
    match rounds.last() {
        Some(last) if last.calls.is_empty() => {
            let last = rounds.pop();
            (rounds, last)
        }
        _ => (rounds, None),
    }
}

/// Every call of the trace, in order.
pub fn all_calls(rounds: &[Round]) -> Vec<CallBlock> {
    rounds.iter().flat_map(|r| r.calls.iter().cloned()).collect()
}

/// "lookup, search" (each name once, in order), at most `max` names and "+N".
pub fn call_names(calls: &[CallBlock], max: usize) -> String {
    let mut names: Vec<&str> = Vec::new();
    for c in calls {
        if !names.contains(&c.call.name.as_str()) {
            names.push(&c.call.name);
        }
    }
    let more = names.len().saturating_sub(max);
    let mut shown = names.into_iter().take(max).collect::<Vec<_>>().join(", ");
    if more > 0 {
        shown.push_str(&format!(" +{}", int(more as i64)));
    }
    shown
}

/// `raw` pretty-printed when it is JSON (keys in their order), else `None`.
pub fn pretty_json(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

/// What a result shows: in Rendered, JSON pretty-printed; else the text as the tool sent it.
pub fn result_text(content: &str, mode: TextMode) -> String {
    let t = content.trim_start();
    if mode == TextMode::Rendered && (t.starts_with('{') || t.starts_with('[')) {
        return pretty_json(content).unwrap_or_else(|| content.to_owned());
    }
    content.to_owned()
}

/// The arguments as shown: in Rendered, pretty JSON (`false` when they are not JSON, shown as the
/// model wrote them); in Raw, exactly as written.
pub fn arguments_text(raw: &str, mode: TextMode) -> (String, bool) {
    if raw.trim().is_empty() {
        return ("{}".to_owned(), true);
    }
    match pretty_json(raw) {
        Some(p) if mode == TextMode::Rendered => (p, true),
        Some(_) => (raw.to_owned(), true),
        None => (raw.to_owned(), false),
    }
}

/// The start of `text` when it is longer than `lines` lines or `chars` characters: the first lines
/// that fit (a line cut at `chars` ends with "…"). `None` when it fits.
pub fn clamp_lines(text: &str, lines: usize, chars: usize) -> Option<String> {
    let total = text.lines().count();
    if total <= lines && text.chars().count() <= chars {
        return None;
    }
    let mut out = String::new();
    for line in text.lines().take(lines) {
        let room = chars.saturating_sub(out.chars().count());
        if room == 0 {
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        if line.chars().count() > room {
            out.extend(line.chars().take(room));
            out.push('…');
            break;
        }
        out.push_str(line);
    }
    Some(out)
}

/// "Show all 143 lines", or for a few long lines "Show all 20,000 characters".
pub fn show_all_label(text: &str) -> String {
    let lines = text.lines().count();
    if lines > RESULT_LINES {
        format!("Show all {} lines", int(lines as i64))
    } else {
        format!("Show all {} characters", int(text.chars().count() as i64))
    }
}

/// "lookup(query, limit)": a call's name and its argument names, for a line.
pub fn call_line(call: &ToolCall) -> String {
    let keys = serde_json::from_str::<serde_json::Value>(&call.arguments)
        .ok()
        .and_then(|v| v.as_object().map(|o| o.iter().map(|(k, v)| format!("{k}: {}", short_value(v))).collect::<Vec<_>>()));
    match keys {
        Some(k) => format!("{}({})", call.name, k.join(", ")),
        None if call.arguments.trim().is_empty() => format!("{}()", call.name),
        None => format!("{}({})", call.name, call.arguments.trim()),
    }
}

/// A JSON value in a few characters: strings quoted and cut at 40.
fn short_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) if s.chars().count() > 40 => format!("\"{}…\"", s.chars().take(40).collect::<String>()),
        serde_json::Value::Array(a) => format!("[{}]", count(a.len() as i64, "item", "items")),
        serde_json::Value::Object(_) => "{…}".to_owned(),
        other => other.to_string(),
    }
}

// ------------------------------------------------------------------ pieces

/// Text with `backticked` parts in Roboto Mono (at 92% of `size`).
pub fn code_spans(words: &str, size: f32) -> Text {
    let mut t = text("");
    for (i, part) in words.split('`').enumerate().filter(|(_, p)| !p.is_empty()) {
        let span = text(part.to_owned());
        t = t.child(if i % 2 == 1 { span.font_family(ROBOTO_MONO.family).text_size(px(size * 0.92)).letter_spacing(px(0.0)) } else { span });
    }
    t
}

/// A 24 px data tag (`label-mono`) in a state's tint: TRUNCATED, ERROR.
pub fn data_tag(words: &'static str, tint: &'static str, ink: &'static str) -> impl IntoElement {
    view()
        .h(px(20.0))
        .flex_none()
        .flex_row()
        .items_center()
        .px(px(8.0))
        .rounded(px(10.0))
        .bg(token(tint))
        .child(text(words).font_family(ROBOTO_MONO.family).text_size(px(11.0)).line_height(px(16.0)).letter_spacing(px(0.44)).text_color(token(ink)))
}

/// A ground well (8 px corners, inside a 16 px card 8 px in) of mono text; `error` tints it.
fn mono_well(content: String, error: bool) -> View {
    view()
        .rounded(px(radius::SM))
        .bg(move || if error { token("negative-tint")() } else { dialog_well() })
        .px(px(12.0))
        .py(px(10.0))
        .child(
            mono(text(content))
                .line_height(px(19.5))
                .selectable(true)
                .text_color(move || if error { token("negative-ink")() } else { theme().foreground }),
        )
}

/// A result's content in its well: the start of a long one with "Show all N lines".
fn result_well(result: TraceTool) -> impl IntoElement {
    let full = signal(false);
    let error = result.is_error;
    dynamic(move || {
        let shown = result_text(&result.content, text_mode());
        if shown.trim().is_empty() {
            return mono_well("(empty)".to_owned(), error).into_element();
        }
        match clamp_lines(&shown, RESULT_LINES, RESULT_CHARS).filter(|_| !full.get()) {
            None => mono_well(shown, error).into_element(),
            Some(start) => view()
                .gap(px(4.0))
                .child(mono_well(start, error))
                .child(
                    view().flex_row().child(
                        button(show_all_label(&shown))
                            .variant(Variant::Ghost)
                            .size(Size::Sm)
                            .on_press(move || full.set(true)),
                    ),
                )
                .into_element(),
        }
    })
}

/// A call's arguments in their well, and a coral note when they are not JSON.
fn arguments_well(raw: String) -> impl IntoElement {
    dynamic(move || {
        let (shown, valid) = arguments_text(&raw, text_mode());
        view()
            .gap(px(4.0))
            .child(mono_well(shown, false))
            .child((!valid).then(|| meta(text("Not valid JSON: shown as the model wrote it.")).text_color(token("coral-ink")).px(px(4.0))))
    })
}

/// One tool call in the sample viewer: a card, open while `open` holds (Rendered and Raw apply to
/// its JSON). `card_bg` is the card's fill (`dialog_muted` in the sheet).
pub fn tool_call_card(block: CallBlock, open_at_first: bool) -> impl IntoElement {
    let open = signal(open_at_first);
    let CallBlock { call, result } = block;
    let server = result.as_ref().map(|r| r.server.clone()).filter(|s| !s.is_empty());
    let latency = result.as_ref().map(|r| duration(Some(r.latency_ms as f64)));
    let is_error = result.as_ref().is_some_and(|r| r.is_error);
    let trigger = view()
        .flex_1()
        .min_w(px(0.0))
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .mx(px(-4.0))
        .px(px(4.0))
        .min_h(px(28.0))
        .rounded(px(radius::XS))
        .role(AccessibilityRole::Button)
        .label(format!("Tool call {}", call.name))
        .accessibility_state(move || AccessibilityState { expanded: Some(open.get()), ..AccessibilityState::default() })
        .focusable(true)
        .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .on_press(move || open.update(|o| *o = !*o))
        .child(icon(icon::CHEVRON_RIGHT, 16.0, move || theme().muted_foreground).rotate(move || if open.get() { 90.0_f32.to_radians() } else { 0.0 }))
        .child(mono(text(call.name.clone())).font_weight(FontWeight::MEDIUM).text_size(px(13.0)).number_of_lines(1).flex_shrink(1.0))
        .child(server.map(|s| body(text(s)).text_color(muted_ink).number_of_lines(1).flex_shrink(1.0)))
        .child(is_error.then(|| data_tag("ERROR", "negative-tint", "negative-ink")))
        .child(view().flex_1())
        .child(latency.map(|l| mono(text(l)).text_color(muted_ink).flex_none()));
    let args = call.arguments.clone();
    let header = view()
        .min_h(px(32.0))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .pl(px(8.0))
        .pr(px(2.0))
        .child(trigger)
        .child(copy_button(args.clone(), "Copy arguments"));
    let body_el = when(move || open.get(), move || {
        let result = result.clone();
        view()
            .gap(px(6.0))
            .pt(px(4.0))
            .child(arguments_well(args.clone()))
            .child(match result {
                Some(r) => result_block(r).into_element(),
                None => meta(text("Not run: the sample used its last round of tool calls.")).text_color(token("coral-ink")).px(px(8.0)).py(px(4.0)).into_element(),
            })
    });
    view()
        .rounded(px(radius::XL))
        .bg(dialog_muted)
        .p(px(8.0))
        .role(AccessibilityRole::Group)
        .label(format!("Tool call {}", call.name))
        .child(header)
        .child(body_el)
}

/// The result under a call: "Result" with its tags and copy button, then its well.
fn result_block(r: TraceTool) -> impl IntoElement {
    let content = r.content.clone();
    view()
        .gap(px(4.0))
        .child(
            view()
                .min_h(px(28.0))
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .pl(px(8.0))
                .pr(px(2.0))
                .child(body(text(if r.is_error { "Error result" } else { "Result" })).font_weight(FontWeight::SEMIBOLD).text_color(move || {
                    if r.is_error { token("negative-ink")() } else { theme().foreground }
                }))
                .child(r.truncated.then(|| data_tag("TRUNCATED", "coral-tint", "coral-ink")))
                .child(view().flex_1())
                .child(copy_button(content, "Copy result")),
        )
        .child(result_well(r))
}

/// A preference card's tool calls, folded to one line in a ground well ("3 tool calls · lookup,
/// search"); open, a line per call with the result's first four lines under it.
pub fn compact_calls(calls: Vec<CallBlock>, muted: Rc<dyn Fn() -> bool>) -> impl IntoElement {
    let open = signal(false);
    let n = calls.len();
    let names = call_names(&calls, 3);
    let errors = calls.iter().filter(|c| c.result.as_ref().is_some_and(|r| r.is_error)).count();
    let summary = format!("{} · {names}", count(n as i64, "tool call", "tool calls"));
    let trigger = view()
        .min_h(px(36.0))
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .px(px(12.0))
        .rounded(px(radius::SM))
        .role(AccessibilityRole::Button)
        .label(summary.clone())
        .accessibility_state(move || AccessibilityState { expanded: Some(open.get()), ..AccessibilityState::default() })
        .focusable(true)
        .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .on_press(move || open.update(|o| *o = !*o))
        .child(icon(icon::CHEVRON_RIGHT, 16.0, muted_ink).rotate(move || if open.get() { 90.0_f32.to_radians() } else { 0.0 }))
        .child(body(text(summary)).text_color(muted_ink).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)))
        .child((errors > 0).then(|| data_tag(if errors == 1 { "1 ERROR" } else { "ERRORS" }, "negative-tint", "negative-ink")));
    view().rounded(px(radius::SM)).bg(move || theme().background).child(trigger).child(when(move || open.get(), move || {
        let muted = muted.clone();
        let rows: Vec<AnyElement> = calls.iter().cloned().enumerate().map(|(i, c)| compact_row(c, i, muted.clone()).into_element()).collect();
        scroll().max_h(px(512.0)).border_t_1().border_color(move || theme().border).child(view().px(px(12.0)).py(px(6.0)).children(rows))
    }))
}

/// One call in a preference card: "lookup(query: …)", the server and latency, the result's start.
fn compact_row(c: CallBlock, index: usize, muted: Rc<dyn Fn() -> bool>) -> impl IntoElement {
    let line = call_line(&c.call);
    let (server, latency, result) = match &c.result {
        Some(r) => (r.server.clone(), duration(Some(r.latency_ms as f64)), Some(r.clone())),
        None => (String::new(), String::new(), None),
    };
    let m1 = muted.clone();
    let result_el = match result {
        Some(r) => {
            let shown = result_text(&r.content, TextMode::Raw);
            let start = clamp_lines(&shown, 4, 400).unwrap_or(shown);
            let error = r.is_error;
            view()
                .flex_row()
                .items_start()
                .gap(px(6.0))
                .child(
                    meta(text(start))
                        .font_family(ROBOTO_MONO.family)
                        .number_of_lines(4)
                        .flex_1()
                        .min_w(px(0.0))
                        .text_color(move || if error { token("negative-ink")() } else { muted_ink() }),
                )
                .child(r.truncated.then(|| data_tag("TRUNCATED", "coral-tint", "coral-ink")))
                .into_element()
        }
        None => meta(text("Not run.")).text_color(token("coral-ink")).into_element(),
    };
    view()
        .py(px(8.0))
        .gap(px(4.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(
                    mono(text(line))
                        .number_of_lines(2)
                        .flex_1()
                        .min_w(px(0.0))
                        .text_color(move || if m1() { muted_ink() } else { theme().foreground }),
                )
                .child((!server.is_empty()).then(|| meta(text(server)).number_of_lines(1).flex_none()))
                .child((!latency.is_empty()).then(|| meta(text(latency)).font_family(ROBOTO_MONO.family).flex_none())),
        )
        .child(result_el)
        .child((index > 0).then(|| view().absolute().top(px(0.0)).left(px(0.0)).right(px(0.0)).h(px(1.0)).bg(move || theme().border)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{TraceAssistant, TraceTool};

    fn assistant(content: &str, calls: &[(&str, &str, &str)], finish: &str) -> TraceMessage {
        TraceMessage::Assistant(TraceAssistant {
            reasoning: Some("thinking".into()),
            content: content.into(),
            tool_calls: calls.iter().map(|(id, name, args)| ToolCall { id: (*id).into(), name: (*name).into(), arguments: (*args).into() }).collect(),
            finish_reason: Some(finish.into()),
            prompt_tokens: Some(10),
            completion_tokens: Some(20),
            reasoning_tokens: None,
            latency_ms: 900,
        })
    }

    fn tool(id: &str, name: &str, content: &str, error: bool) -> TraceMessage {
        TraceMessage::Tool(TraceTool {
            tool_call_id: id.into(),
            name: name.into(),
            server: "docs".into(),
            content: content.into(),
            is_error: error,
            truncated: false,
            latency_ms: 120,
            code: None,
        })
    }

    #[test]
    fn a_two_round_trace_groups_calls_with_their_results() {
        let trace = vec![
            assistant("Let me look.", &[("c1", "lookup", "{\"q\": \"x\"}"), ("c2", "fetch", "{oops")], "tool_calls"),
            tool("c2", "fetch", "404 not found", true),
            tool("c1", "lookup", "{\"hits\": 3}", false),
            assistant("", &[("c3", "lookup", "{}")], "tool_calls"),
            tool("c3", "lookup", "ok", false),
            assistant("The answer is 42.", &[], "stop"),
        ];
        let rounds = trace_rounds(&trace);
        assert_eq!(rounds.len(), 3);
        assert_eq!(rounds[0].n, 1);
        assert_eq!(rounds[0].text, "Let me look.");
        // Results join their calls by id, whatever their order.
        assert_eq!(rounds[0].calls[0].result.as_ref().unwrap().content, "{\"hits\": 3}");
        assert!(rounds[0].calls[1].result.as_ref().unwrap().is_error);
        assert_eq!(all_calls(&rounds).len(), 3);
        assert_eq!(call_names(&all_calls(&rounds), 3), "lookup, fetch");
        let (steps, last) = split_final(rounds);
        assert_eq!(steps.len(), 2);
        assert_eq!(last.unwrap().text, "The answer is 42.");
    }

    #[test]
    fn a_trace_cut_by_the_round_limit_has_no_answer() {
        let trace = vec![assistant("", &[("c1", "lookup", "{}")], "tool_calls"), tool("c1", "lookup", "x", false), assistant("", &[("c2", "lookup", "{}")], "tool_rounds")];
        let (steps, last) = split_final(trace_rounds(&trace));
        assert_eq!(steps.len(), 2);
        assert!(last.is_none());
        assert!(steps[1].calls[0].result.is_none());
        // A result no call asked for still shows.
        let stray = trace_rounds(&[tool("z", "ghost", "?", false)]);
        assert_eq!(stray[0].calls[0].call.name, "ghost");
    }

    #[test]
    fn arguments_and_results_render_or_stay_raw() {
        assert_eq!(arguments_text("{\"b\":1,\"a\":2}", TextMode::Rendered), ("{\n  \"b\": 1,\n  \"a\": 2\n}".to_owned(), true));
        assert_eq!(arguments_text("{\"b\":1}", TextMode::Raw), ("{\"b\":1}".to_owned(), true));
        assert_eq!(arguments_text("{oops", TextMode::Rendered), ("{oops".to_owned(), false));
        assert_eq!(result_text("[1,2]", TextMode::Rendered), "[\n  1,\n  2\n]");
        assert_eq!(result_text("plain text", TextMode::Rendered), "plain text");
        assert_eq!(call_line(&ToolCall { id: "1".into(), name: "lookup".into(), arguments: "{\"q\":\"x\",\"n\":3}".into() }), "lookup(q: \"x\", n: 3)");
    }

    #[test]
    fn long_results_show_their_start() {
        let short = "a\nb";
        assert_eq!(clamp_lines(short, 20, 2000), None);
        let long = (1..=30).map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
        let start = clamp_lines(&long, 20, 2000).unwrap();
        assert_eq!(start.lines().count(), 20);
        assert_eq!(show_all_label(&long), "Show all 30 lines");
        let wide = "x".repeat(5000);
        let start = clamp_lines(&wide, 20, 2000).unwrap();
        assert_eq!(start.chars().count(), 2001);
        assert_eq!(show_all_label(&wide), "Show all 5,000 characters");
    }
}
