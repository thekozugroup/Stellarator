//! Code mode traces (DESIGN.md, "Tool traces"): the model's call to the code tool is a card of its
//! own, because what it did is the code and the tool calls the code made, not the arguments.
//!
//! - Pure: [`status_tag`] and [`status_word`] (how an execution ended), [`group_inner_calls`] (the
//!   calls an execution made, a method once with its count), [`code_runs`] (the executions of a
//!   trace), [`counts`] and [`fold_summary`] ("2 code runs · 5 tool calls"), [`fence_for`].
//! - [`code_card`]: one execution in the sample viewer. Header: a chevron, the tool's name in mono,
//!   "3 tool calls" muted, the status tag (ERROR, SYNTAX ERROR, TIMEOUT, MEMORY LIMIT), the latency
//!   and a copy button (the code). Open: the code, highlighted as JavaScript, in a ground well; the
//!   tool calls as compact rows that open to their arguments and result; the console output
//!   (muted mono, folded when long); and the result the model saw, which is exactly what the tool
//!   message held.
//! - [`compact_code_runs`]: the same in a preference card, folded to "2 code runs · 5 tool calls".

use std::rc::Rc;

use nsh_base::{TextViewClasses, text_view, theme};
use nsh_core::prelude::*;

use super::button::{Size, Variant, button};
use super::icon::{self, icon};
use super::material::{dialog_muted, dialog_well};
use super::tooltip::copy_button;
use super::trace::{CallBlock, RESULT_CHARS, RESULT_LINES, arguments_well, clamp_lines, data_tag, result_block, result_block_with_title, result_text, show_all_label};
use crate::api::types::{CodeExecution, CodeStatus, InnerToolCall, TraceTool};
use crate::format::{count, duration, int};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::{body, meta, mono};
use crate::theme::{muted_ink, radius, token};

/// Console output of more lines or characters than this opens folded.
const LOG_LINES: usize = 6;
const LOG_CHARS: usize = 800;

// ------------------------------------------------------------------ pure

/// The status tag's words for an execution that did not return, `None` for one that did.
pub fn status_tag(status: CodeStatus) -> Option<&'static str> {
    match status {
        CodeStatus::Ok => None,
        CodeStatus::Error => Some("ERROR"),
        CodeStatus::SyntaxError => Some("SYNTAX ERROR"),
        CodeStatus::Timeout => Some("TIMEOUT"),
        CodeStatus::MemoryLimit => Some("MEMORY LIMIT"),
    }
}

/// How an execution ended, in a sentence's words (for a label).
pub fn status_word(status: CodeStatus) -> &'static str {
    match status {
        CodeStatus::Ok => "returned",
        CodeStatus::Error => "threw an error",
        CodeStatus::SyntaxError => "has a syntax error",
        CodeStatus::Timeout => "timed out",
        CodeStatus::MemoryLimit => "ran out of memory",
    }
}

/// A method the code called, how many times, and how many of those calls threw.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodCalls {
    pub method: String,
    pub calls: usize,
    pub errors: usize,
}

/// The calls of `calls` grouped by method, in the order each method was first called.
pub fn group_inner_calls(calls: &[InnerToolCall]) -> Vec<MethodCalls> {
    let mut groups: Vec<MethodCalls> = Vec::new();
    for c in calls {
        let error = usize::from(c.is_error);
        match groups.iter_mut().find(|g| g.method == c.method) {
            Some(g) => {
                g.calls += 1;
                g.errors += error;
            }
            None => groups.push(MethodCalls { method: c.method.clone(), calls: 1, errors: error }),
        }
    }
    groups
}

/// "lookup.search ×3, lookup.fetch": each method once with its count (when more than one), at most
/// `max` of them, then "+N".
pub fn method_names(calls: &[InnerToolCall], max: usize) -> String {
    let groups = group_inner_calls(calls);
    let more = groups.len().saturating_sub(max);
    let mut shown = groups
        .iter()
        .take(max)
        .map(|g| if g.calls > 1 { format!("{} ×{}", g.method, int(g.calls as i64)) } else { g.method.clone() })
        .collect::<Vec<_>>()
        .join(", ");
    if more > 0 {
        shown.push_str(&format!(" +{}", int(more as i64)));
    }
    shown
}

/// A call of the code tool that ran: the model's call, the tool message and what happened inside.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeRun {
    pub block: CallBlock,
    pub execution: CodeExecution,
}

/// The code of `block`, when it is a call of the code tool that ran code.
pub fn execution_of(block: &CallBlock) -> Option<&CodeExecution> {
    block.result.as_ref().and_then(|r| r.code.as_ref())
}

/// The calls of `blocks` that ran code, in order.
pub fn code_runs(blocks: &[CallBlock]) -> Vec<CodeRun> {
    blocks.iter().filter_map(|b| execution_of(b).map(|e| CodeRun { block: b.clone(), execution: e.clone() })).collect()
}

/// What a sample's code runs add up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Executions of the code tool.
    pub runs: usize,
    /// Tool calls the code made.
    pub calls: usize,
    /// Executions that did not return.
    pub failed_runs: usize,
    /// Tool calls that threw.
    pub failed_calls: usize,
    /// Calls of other tools (the model named one that is not the code tool).
    pub other: usize,
}

/// The counts of `blocks`.
pub fn counts(blocks: &[CallBlock]) -> Counts {
    let mut c = Counts::default();
    for b in blocks {
        match execution_of(b) {
            Some(e) => {
                c.runs += 1;
                c.calls += e.calls.len();
                c.failed_runs += usize::from(e.status != CodeStatus::Ok);
                c.failed_calls += e.calls.iter().filter(|x| x.is_error).count();
            }
            None => c.other += 1,
        }
    }
    c
}

/// "2 code runs · 5 tool calls" (a call of another tool adds "· 1 other call").
pub fn fold_summary(c: &Counts) -> String {
    let mut s = format!("{} · {}", count(c.runs as i64, "code run", "code runs"), count(c.calls as i64, "tool call", "tool calls"));
    if c.other > 0 {
        s.push_str(&format!(" · {}", count(c.other as i64, "other call", "other calls")));
    }
    s
}

/// "Code 1 of 2" style words are not used; the header's count: "3 tool calls", "no tool calls".
pub fn calls_words(e: &CodeExecution) -> String {
    match e.calls.len() {
        0 => "no tool calls".to_owned(),
        n => count(n as i64, "tool call", "tool calls"),
    }
}

/// A fence for `code` in Markdown: three backticks, or one more than the longest run in the code.
pub fn fence_for(code: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for ch in code.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

/// `code` as a fenced JavaScript block.
pub fn js_markdown(code: &str) -> String {
    let fence = fence_for(code);
    format!("{fence}js\n{}\n{fence}", code.trim_end_matches('\n'))
}

/// The inner call as the tool result it is shown as (the result well and its tags are the same).
pub fn inner_as_result(c: &InnerToolCall) -> TraceTool {
    TraceTool {
        tool_call_id: String::new(),
        name: c.method.clone(),
        server: c.server.clone(),
        content: c.result.clone(),
        is_error: c.is_error,
        truncated: c.truncated,
        latency_ms: c.latency_ms,
        code: None,
    }
}

/// An inner call's arguments as JSON text (compact; the well pretty-prints it); empty when none.
fn arguments_json(c: &InnerToolCall) -> String {
    if c.arguments.is_null() { String::new() } else { serde_json::to_string(&c.arguments).unwrap_or_default() }
}

/// `method(key: value, …)` for a preference card's row.
pub fn inner_call_line(c: &InnerToolCall) -> String {
    let args = match &c.arguments {
        serde_json::Value::Object(o) => o.iter().map(|(k, v)| format!("{k}: {}", short(v))).collect::<Vec<_>>().join(", "),
        serde_json::Value::Null => String::new(),
        other => short(other),
    };
    format!("{}({args})", c.method)
}

fn short(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) if s.chars().count() > 40 => format!("\"{}…\"", s.chars().take(40).collect::<String>()),
        serde_json::Value::Array(a) => format!("[{}]", count(a.len() as i64, "item", "items")),
        serde_json::Value::Object(_) => "{…}".to_owned(),
        other => other.to_string(),
    }
}

// ------------------------------------------------------------------ pieces

/// JavaScript, highlighted by the text view (its own palette) in a ground well. The block's own
/// fill, border and padding are taken off by classes (the well around it has them): classes reach
/// a text view wherever it is made, parts given as a context do not reach one in a sheet. Long
/// lines scroll sideways, as the text view's code blocks do.
fn code_well(code: String) -> impl IntoElement {
    let markdown = js_markdown(&code);
    let view_el = dynamic(move || {
        let classes = TextViewClasses {
            root: Some("text-[12px] leading-[19.5px] text-foreground".into()),
            code_block: Some("border-0 bg-transparent rounded-none".into()),
            code_block_code: Some("p-0 font-mono text-[12px] leading-[19.5px]".into()),
            ..TextViewClasses::default()
        };
        let markdown = markdown.clone();
        text_view().classes(classes).markdown(move || Some(SharedString::from(markdown.clone()))).selectable(true)
    });
    view().rounded(px(radius::SM)).bg(dialog_well).px(px(12.0)).py(px(10.0)).child(view_el)
}

/// The code, its start with "Show all N lines" when it is long.
fn code_block(code: String) -> impl IntoElement {
    let full = signal(false);
    dynamic(move || match clamp_lines(&code, RESULT_LINES, RESULT_CHARS).filter(|_| !full.get()) {
        None => code_well(code.clone()).into_element(),
        Some(start) => view()
            .gap(px(4.0))
            .child(code_well(start))
            .child(
                view().flex_row().child(
                    button(show_all_label(&code)).variant(Variant::Ghost).size(Size::Sm).on_press(move || full.set(true)),
                ),
            )
            .into_element(),
    })
}

/// A row that folds: a chevron and `title`, `open` to show `content`.
fn fold_row(open: Signal<bool>, label: String, title: impl IntoElement) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .min_h(px(28.0))
        .mx(px(-4.0))
        .px(px(4.0))
        .rounded(px(radius::XS))
        .role(AccessibilityRole::Button)
        .label(label)
        .accessibility_state(move || AccessibilityState { expanded: Some(open.get()), ..AccessibilityState::default() })
        .focusable(true)
        .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .on_press(move || open.update(|o| *o = !*o))
        .child(icon(icon::CHEVRON_RIGHT, 16.0, move || theme().muted_foreground).rotate(move || if open.get() { 90.0_f32.to_radians() } else { 0.0 }))
        .child(title)
}

/// One tool call the code made: a compact row (method, server, tags, latency) that opens to its
/// arguments and its result.
fn inner_call_row(c: InnerToolCall) -> impl IntoElement {
    let open = signal(false);
    let known = !c.server.is_empty();
    let title = view()
        .flex_1()
        .min_w(px(0.0))
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .child(mono(text(c.method.clone())).font_weight(FontWeight::MEDIUM).text_size(px(12.0)).number_of_lines(1).flex_shrink(1.0))
        .child(if known {
            body(text(c.server.clone())).text_color(muted_ink).number_of_lines(1).flex_shrink(1.0).into_element()
        } else {
            meta(text("no such method")).text_color(token("negative-ink")).number_of_lines(1).into_element()
        })
        .child(c.is_error.then(|| data_tag("ERROR", "negative-tint", "negative-ink")))
        .child(c.truncated.then(|| data_tag("TRUNCATED", "coral-tint", "coral-ink")))
        .child(view().flex_1())
        .child(mono(text(duration(Some(c.latency_ms as f64)))).text_color(muted_ink).flex_none());
    let args = arguments_json(&c);
    let result = inner_as_result(&c);
    view()
        .role(AccessibilityRole::Group)
        .label(format!("Tool call {}", c.method))
        .child(fold_row(open, format!("Tool call {}", c.method), title))
        .child(when(move || open.get(), move || {
            view().pl(px(22.0)).pt(px(2.0)).pb(px(6.0)).gap(px(6.0)).child(arguments_well(args.clone())).child(result_block(result.clone()))
        }))
}

/// The console output: "Console · 3 lines" folded or open, in muted mono.
fn console(logs: Vec<String>) -> impl IntoElement {
    let joined = logs.join("\n");
    let long = logs.len() > LOG_LINES || joined.chars().count() > LOG_CHARS;
    let open = signal(!long);
    let title = view()
        .flex_row()
        .items_baseline()
        .gap(px(8.0))
        .child(body(text("Console")).font_weight(FontWeight::SEMIBOLD))
        .child(meta(text(count(logs.len() as i64, "line", "lines"))).tabular_nums());
    let copy = joined.clone();
    view()
        .gap(px(2.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .pr(px(2.0))
                .child(view().flex_1().min_w(px(0.0)).child(fold_row(open, "Console output".to_owned(), title)))
                .child(copy_button(copy, "Copy console output")),
        )
        .child(when(move || open.get(), move || {
            let shown = clamp_lines(&joined, RESULT_LINES, RESULT_CHARS).unwrap_or_else(|| joined.clone());
            view().gap(px(4.0)).child(
                view().rounded(px(radius::SM)).bg(dialog_well).px(px(12.0)).py(px(10.0)).child(
                    mono(text(shown)).line_height(px(19.5)).selectable(true).text_color(muted_ink),
                ),
            )
        }))
}

/// The tool calls of one execution, under a "Tool calls 3" line.
fn inner_calls(calls: Vec<InnerToolCall>) -> impl IntoElement {
    let n = calls.len();
    let failed = calls.iter().filter(|c| c.is_error).count();
    let rows: Vec<AnyElement> = calls.into_iter().map(|c| inner_call_row(c).into_element()).collect();
    view()
        .gap(px(2.0))
        .child(
            view()
                .flex_row()
                .items_baseline()
                .gap(px(8.0))
                .min_h(px(28.0))
                .pl(px(8.0))
                .child(body(text("Tool calls")).font_weight(FontWeight::SEMIBOLD))
                .child(meta(text(if failed > 0 { format!("{} · {} failed", int(n as i64), int(failed as i64)) } else { int(n as i64) })).tabular_nums()),
        )
        .children(rows)
}

/// One execution in the sample viewer: a card, open while `open` holds.
pub fn code_card(block: CallBlock, open_at_first: bool) -> impl IntoElement {
    let open = signal(open_at_first);
    let CallBlock { call, result } = block;
    let result = result.expect("a code run has its result");
    let execution = result.code.clone().expect("a code run has its execution");
    let tag = status_tag(execution.status);
    let words = calls_words(&execution);
    let label = format!("Code run {}: {}", call.name, status_word(execution.status));
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
        .label(label.clone())
        .accessibility_state(move || AccessibilityState { expanded: Some(open.get()), ..AccessibilityState::default() })
        .focusable(true)
        .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .on_press(move || open.update(|o| *o = !*o))
        .child(icon(icon::CHEVRON_RIGHT, 16.0, move || theme().muted_foreground).rotate(move || if open.get() { 90.0_f32.to_radians() } else { 0.0 }))
        .child(mono(text(call.name.clone())).font_weight(FontWeight::MEDIUM).text_size(px(13.0)).number_of_lines(1).flex_shrink(1.0))
        .child(body(text(words)).text_color(muted_ink).number_of_lines(1).flex_shrink(1.0))
        .child(tag.map(|t| data_tag(t, "negative-tint", "negative-ink")))
        .child(view().flex_1())
        .child(mono(text(duration(Some(execution.latency_ms as f64)))).text_color(muted_ink).flex_none());
    let header = view()
        .min_h(px(32.0))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .pl(px(8.0))
        .pr(px(2.0))
        .child(trigger)
        .child(copy_button(execution.code.clone(), "Copy code"));
    let body_el = when(move || open.get(), move || {
        let e = execution.clone();
        let r = result.clone();
        view()
            .gap(px(8.0))
            .pt(px(4.0))
            .child(code_block(e.code.clone()))
            .child((!e.calls.is_empty()).then(|| inner_calls(e.calls.clone())))
            .child((!e.logs.is_empty()).then(|| console(e.logs.clone())))
            .child(model_result(r))
    });
    view()
        .rounded(px(radius::XL))
        .bg(dialog_muted)
        .p(px(8.0))
        .role(AccessibilityRole::Group)
        .label(label)
        .child(header)
        .child(body_el)
}

/// What the model saw: the tool message's content, as the other tool results show theirs.
fn model_result(r: TraceTool) -> impl IntoElement {
    let words = if r.is_error { "Error the model saw" } else { "Result the model saw" };
    result_block_with_title(r, words)
}

// ------------------------------------------------------------------ preference card

/// A preference card's code runs, folded to one line in a ground well ("2 code runs · 5 tool
/// calls"); open, each run's code and its tool calls with the result's first lines.
pub fn compact_code_runs(blocks: Vec<CallBlock>, muted: Rc<dyn Fn() -> bool>) -> impl IntoElement {
    let open = signal(false);
    let c = counts(&blocks);
    let summary = fold_summary(&c);
    let failed = c.failed_runs + c.failed_calls;
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
        .child((failed > 0).then(|| data_tag(if failed == 1 { "1 ERROR" } else { "ERRORS" }, "negative-tint", "negative-ink")));
    view().rounded(px(radius::SM)).bg(move || theme().background).child(trigger).child(when(move || open.get(), move || {
        let rows: Vec<AnyElement> =
            blocks.iter().cloned().enumerate().map(|(i, b)| compact_run(b, i, muted.clone()).into_element()).collect();
        scroll().max_h(px(512.0)).border_t_1().border_color(move || theme().border).child(view().px(px(12.0)).py(px(6.0)).children(rows))
    }))
}

/// One run in a preference card: its status and latency, the code's start, a line per tool call, and
/// the start of what the model saw.
fn compact_run(b: CallBlock, index: usize, muted: Rc<dyn Fn() -> bool>) -> impl IntoElement {
    let m1 = muted.clone();
    let Some(result) = b.result.clone() else {
        return view().py(px(8.0)).child(meta(text("Not run.")).text_color(token("coral-ink"))).into_element();
    };
    let ink = move || if m1() { muted_ink() } else { theme().foreground };
    let Some(e) = result.code.clone() else {
        // A call of another tool: the name and the start of its result.
        let shown = result_text(&result.content, super::model_text::TextMode::Raw);
        let start = clamp_lines(&shown, 4, 400).unwrap_or(shown);
        return view().py(px(8.0)).gap(px(4.0)).child(mono(text(b.call.name.clone())).text_color(ink)).child(meta(text(start)).font_family(ROBOTO_MONO.family)).into_element();
    };
    let tag = status_tag(e.status);
    let code_start = clamp_lines(&e.code, 6, 500).unwrap_or_else(|| e.code.clone());
    let lines: Vec<AnyElement> = e
        .calls
        .iter()
        .map(|c| {
            let line = inner_call_line(c);
            let error = c.is_error;
            let m = muted.clone();
            view()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(
                    mono(text(line))
                        .number_of_lines(2)
                        .flex_1()
                        .min_w(px(0.0))
                        .text_color(move || if error { token("negative-ink")() } else if m() { muted_ink() } else { theme().foreground }),
                )
                .child((!c.server.is_empty()).then(|| meta(text(c.server.clone())).number_of_lines(1).flex_none()))
                .child(meta(text(duration(Some(c.latency_ms as f64)))).font_family(ROBOTO_MONO.family).flex_none())
                .into_element()
        })
        .collect();
    let seen = result_text(&result.content, super::model_text::TextMode::Raw);
    let seen_start = clamp_lines(&seen, 4, 400).unwrap_or(seen);
    let error = result.is_error;
    view()
        .py(px(8.0))
        .gap(px(6.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(mono(text(code_start)).text_size(px(12.0)).line_height(px(18.0)).flex_1().min_w(px(0.0)).text_color(ink))
                .child(tag.map(|t| data_tag(t, "negative-tint", "negative-ink")))
                .child(meta(text(duration(Some(e.latency_ms as f64)))).font_family(ROBOTO_MONO.family).flex_none()),
        )
        .child((!lines.is_empty()).then(|| view().gap(px(2.0)).pl(px(12.0)).children(lines)))
        .child(
            meta(text(seen_start))
                .font_family(ROBOTO_MONO.family)
                .number_of_lines(4)
                .text_color(move || if error { token("negative-ink")() } else { muted_ink() }),
        )
        .child((index > 0).then(|| view().absolute().top(px(0.0)).left(px(0.0)).right(px(0.0)).h(px(1.0)).bg(move || theme().border)))
        .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ToolCall, TraceTool};
    use serde_json::json;

    fn inner(method: &str, error: bool) -> InnerToolCall {
        InnerToolCall {
            method: method.into(),
            server: "lookup".into(),
            tool: method.split('.').next_back().unwrap_or("").into(),
            arguments: json!({"q": "x", "limit": 3}),
            result: "ok".into(),
            is_error: error,
            truncated: false,
            latency_ms: 40,
        }
    }

    fn execution(status: CodeStatus, calls: Vec<InnerToolCall>) -> CodeExecution {
        CodeExecution { code: "async () => 1".into(), status, calls, logs: vec![], result: Some("1".into()), error: None, latency_ms: 120 }
    }

    fn block(id: &str, exec: Option<CodeExecution>) -> CallBlock {
        CallBlock {
            call: ToolCall { id: id.into(), name: "codemode".into(), arguments: "{\"code\":\"async () => 1\"}".into() },
            result: Some(TraceTool {
                tool_call_id: id.into(),
                name: "codemode".into(),
                server: String::new(),
                content: "{\"result\":1,\"logs\":[]}".into(),
                is_error: false,
                truncated: false,
                latency_ms: 120,
                code: exec,
            }),
        }
    }

    #[test]
    fn statuses_have_their_tags_and_words() {
        assert_eq!(status_tag(CodeStatus::Ok), None);
        assert_eq!(status_tag(CodeStatus::Error), Some("ERROR"));
        assert_eq!(status_tag(CodeStatus::SyntaxError), Some("SYNTAX ERROR"));
        assert_eq!(status_tag(CodeStatus::Timeout), Some("TIMEOUT"));
        assert_eq!(status_tag(CodeStatus::MemoryLimit), Some("MEMORY LIMIT"));
        assert_eq!(status_word(CodeStatus::Ok), "returned");
        assert_eq!(status_word(CodeStatus::SyntaxError), "has a syntax error");
    }

    #[test]
    fn inner_calls_group_by_method_in_first_call_order() {
        let calls = [inner("lookup.search", false), inner("lookup.fetch", true), inner("lookup.search", true), inner("lookup.search", false)];
        assert_eq!(
            group_inner_calls(&calls),
            vec![
                MethodCalls { method: "lookup.search".into(), calls: 3, errors: 1 },
                MethodCalls { method: "lookup.fetch".into(), calls: 1, errors: 1 },
            ]
        );
        assert_eq!(method_names(&calls, 3), "lookup.search ×3, lookup.fetch");
        assert_eq!(method_names(&calls, 1), "lookup.search ×3 +1");
        assert_eq!(method_names(&[], 3), "");
    }

    #[test]
    fn a_traces_code_runs_add_up() {
        let blocks = vec![
            block("a", Some(execution(CodeStatus::Ok, vec![inner("lookup.search", false), inner("lookup.fetch", false)]))),
            block("b", Some(execution(CodeStatus::Error, vec![inner("lookup.search", true), inner("lookup.search", false), inner("lookup.fetch", false)]))),
            block("c", None),
        ];
        assert_eq!(code_runs(&blocks).len(), 2);
        let c = counts(&blocks);
        assert_eq!(c, Counts { runs: 2, calls: 5, failed_runs: 1, failed_calls: 1, other: 1 });
        assert_eq!(fold_summary(&c), "2 code runs · 5 tool calls · 1 other call");
        let one = counts(&blocks[..1]);
        assert_eq!(fold_summary(&one), "1 code run · 2 tool calls");
        assert_eq!(fold_summary(&counts(&[block("d", Some(execution(CodeStatus::SyntaxError, vec![])))])), "1 code run · 0 tool calls");
        assert_eq!(calls_words(&execution(CodeStatus::Ok, vec![])), "no tool calls");
        assert_eq!(calls_words(&execution(CodeStatus::Ok, vec![inner("a.b", false)])), "1 tool call");
    }

    #[test]
    fn code_is_fenced_so_its_own_backticks_cannot_end_the_block() {
        assert_eq!(fence_for("async () => 1"), "```");
        assert_eq!(fence_for("`a` + ```b```"), "````");
        let md = js_markdown("const s = `x`;\n");
        assert_eq!(md, "```js\nconst s = `x`;\n```");
    }

    #[test]
    fn an_inner_call_reads_as_a_result_and_a_line() {
        let c = inner("lookup.search", true);
        let r = inner_as_result(&c);
        assert_eq!((r.name.as_str(), r.server.as_str(), r.content.as_str(), r.is_error, r.latency_ms), ("lookup.search", "lookup", "ok", true, 40));
        assert_eq!(inner_call_line(&c), "lookup.search(q: \"x\", limit: 3)");
        let mut none = c.clone();
        none.arguments = serde_json::Value::Null;
        assert_eq!(inner_call_line(&none), "lookup.search()");
        assert_eq!(arguments_json(&none), "");
    }
}
