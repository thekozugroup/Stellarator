//! One chat-completion request: build it, send it (streaming or not), retry it, and split the
//! thinking trace from the answer.
//!
//! Public pieces:
//! - [`prepare_messages`]: the prompt as sent (system prompt and user template applied)
//! - [`build_body`]: the JSON request body ([`build_request`] with tools)
//! - [`split_inline`]: thinking/answer split for inline tags and gpt-oss harmony text
//! - [`generate`]: one sample, with retries ([`generate_round`]: one request of a tool loop)
//! - [`Progress`], [`ServerCaps`], [`GenerationResult`], [`GenerateError`]
//!
//! Tool calls are assembled from `delta.tool_calls` by index (id, name and argument pieces), from a
//! whole call in one chunk, from `message.tool_calls` in a non-streaming answer, and from the
//! legacy `function_call` field.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::StreamExt;
use reqwest::header::HeaderMap;
use serde_json::{Map, Value};
use stellarator_core::openai;
use stellarator_core::schemas::{LiveCode, Phase, RunParams, ToolCall};

pub const RETRY_STATUSES: [u16; 8] = [408, 409, 425, 429, 500, 502, 503, 504];
/// Characters of streamed text kept for the live view.
pub const TAIL_CHARS: usize = 240;
const MAX_BACKOFF_S: f64 = 30.0;
const MAX_RETRY_AFTER_S: f64 = 300.0;
const REASONING_FIELDS: [&str; 3] = ["reasoning_content", "reasoning", "thinking"];
const HARMONY_CHANNEL: &str = "<|channel|>";
const HARMONY_MESSAGE: &str = "<|message|>";
const HARMONY_CONSTRAIN: &str = "<|constrain|>";
const HARMONY_FINAL_MARK: &str = "<|channel|>final";
const HARMONY_ENDS: [&str; 5] = ["<|end|>", "<|return|>", "<|call|>", "<|start|>", "<|channel|>"];
const DROPPED: &str = "The connection to the server dropped before the answer finished.";
/// Where docs/tools.md explains which endpoints support tool calling.
pub const TOOLS_DOC: &str = "https://github.com/thekozugroup/Stellarator/blob/main/docs/tools.md";

// ------------------------------------------------------------------------------------ data

/// What one request is doing right now, for the live view.
#[derive(Clone, Debug)]
pub struct Progress {
    pub phase: Phase,
    /// Streamed chunks that carried text in this attempt.
    pub tokens: i64,
    /// Last ~240 characters of the current phase.
    pub tail: String,
    pub attempt: i64,
    /// The request of the sample: 1, then one more after each round of tool calls.
    pub round: i64,
    /// The tools running, while `phase` is `Tool` (`Code`: the code tool).
    pub tools: Vec<String>,
    /// Code mode, while `phase` is `Code`: the executions' tool calls.
    pub code: Option<LiveCode>,
}

impl Default for Progress {
    fn default() -> Self {
        Self { phase: Phase::Waiting, tokens: 0, tail: String::new(), attempt: 1, round: 1, tools: Vec::new(), code: None }
    }
}

/// What we learned about a server during a run (shared by every request of the run).
#[derive(Debug)]
pub struct ServerCaps {
    stream_options: AtomicBool,
}

impl Default for ServerCaps {
    fn default() -> Self {
        Self { stream_options: AtomicBool::new(true) }
    }
}

impl ServerCaps {
    /// False once the server rejected `stream_options`; later requests go without it.
    pub fn stream_options(&self) -> bool {
        self.stream_options.load(Ordering::Relaxed)
    }
    fn reject_stream_options(&self) {
        self.stream_options.store(false, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenerationResult {
    pub reasoning: Option<String>,
    pub response: String,
    pub finish_reason: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub latency_ms: i64,
    pub ttft_ms: Option<i64>,
    pub attempts: i64,
    /// Text-carrying chunks streamed in the successful attempt.
    pub chunks: i64,
    /// The tools the model called in this answer (empty for a plain answer).
    pub tool_calls: Vec<ToolCall>,
}

/// One failed attempt.
#[derive(Clone, Debug)]
pub struct AttemptError {
    pub message: String,
    pub retryable: bool,
    pub status: Option<u16>,
    pub retry_after: Option<f64>,
    pub body: String,
}

impl AttemptError {
    fn new(message: impl Into<String>, retryable: bool) -> Self {
        Self { message: message.into(), retryable, status: None, retry_after: None, body: String::new() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GenerateError {
    /// The sample failed for good (after retries, or at once for a non-retryable error).
    Failed { message: String, attempts: i64 },
    /// A wait before a retry was cut short (the run is pausing); the sample goes back to pending.
    Interrupted,
    /// The endpoint refused the request's `tools` or `tool_choice`: every sample would fail, so
    /// the run pauses with this message and the sample goes back to pending.
    ToolsRejected { message: String },
}

/// The tools part of a request: the definitions and `tool_choice`.
#[derive(Clone, Copy, Debug)]
pub struct RequestTools<'a> {
    pub definitions: &'a [Value],
    /// "auto" or "required".
    pub choice: &'a str,
}

/// Receives the live state of one request.
pub trait ProgressSink: Send + Sync {
    /// A chunk with text arrived (one more token for the rate).
    fn on_chunk(&self, _progress: &Progress) {}
    /// The state changed without a token: a new attempt started, or the request waits to retry.
    fn on_reset(&self, _progress: &Progress) {}
}

/// No live view.
pub struct NoProgress;
impl ProgressSink for NoProgress {}

/// Waits before a retry. Resolves to false when the wait was interrupted.
pub trait Backoff: Send + Sync {
    fn sleep(&self, seconds: f64) -> BoxFuture<'_, bool>;
}

/// A plain timer.
pub struct RealSleep;
impl Backoff for RealSleep {
    fn sleep(&self, seconds: f64) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_secs_f64(seconds.max(0.0))).await;
            true
        })
    }
}

// ------------------------------------------------------------------------------------ text helpers

/// The text of a message's content: a string, or the text parts of a list.
pub fn content_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => {
            let mut out = String::new();
            for part in parts {
                match part {
                    Value::String(s) => out.push_str(s),
                    Value::Object(o) => {
                        if let Some(Value::String(t)) = o.get("text") {
                            out.push_str(t);
                        }
                    }
                    _ => {}
                }
            }
            out
        }
        None | Some(Value::Null) => String::new(),
        Some(other) => py_str(other),
    }
}

/// Python's `str()` of a JSON value, for the rare non-string content.
fn py_str(v: &Value) -> String {
    match v {
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The text of the last user message, or "".
pub fn last_user_text(messages: &[Value]) -> String {
    for m in messages.iter().rev() {
        if m.get("role").and_then(Value::as_str) == Some("user") {
            return content_text(m.get("content"));
        }
    }
    String::new()
}

/// The last `n` characters of `s`.
pub fn last_chars(s: &str, n: usize) -> &str {
    if n == 0 {
        return "";
    }
    match s.char_indices().rev().nth(n - 1) {
        Some((i, _)) => &s[i..],
        None => s,
    }
}

/// Drop characters from the front of `s` until at most `n` remain.
fn keep_last_chars(s: &mut String, n: usize) {
    let start = s.len() - last_chars(s, n).len();
    if start > 0 {
        s.drain(..start);
    }
}

fn first_chars(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Collapse runs of whitespace into single spaces and trim (Python's `" ".join(s.split())`).
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One line, at most `limit` characters, ending in punctuation.
pub fn sentence(text: &str, limit: usize) -> String {
    let mut text = one_line(text);
    if text.chars().count() > limit {
        text = format!("{}…", first_chars(&text, limit.saturating_sub(1)).trim_end());
    }
    if let Some(last) = text.chars().last() {
        if !".!?…".contains(last) {
            text.push('.');
        }
    }
    text
}

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Python truthiness of a JSON value.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// `{:g}` for the timeout in messages: 1800 -> "1800", 0.05 -> "0.05".
fn fmt_g(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

// ------------------------------------------------------------------------------------ request building

/// The prompt as sent: `system_prompt` replaces any system message, `user_template` wraps the last
/// user message. Plain string replacement, so braces in prompts are safe.
pub fn prepare_messages(messages: &[Value], params: &RunParams) -> Vec<Value> {
    let mut msgs: Vec<Value> = messages.iter().filter(|m| m.is_object()).cloned().collect();
    if let Some(sp) = params.system_prompt.as_deref().filter(|s| !s.trim().is_empty()) {
        msgs.retain(|m| m.get("role").and_then(Value::as_str) != Some("system"));
        let mut sys = Map::new();
        sys.insert("role".into(), Value::String("system".into()));
        sys.insert("content".into(), Value::String(sp.to_string()));
        msgs.insert(0, Value::Object(sys));
    }
    if let Some(template) = params.user_template.as_deref().filter(|t| t.contains("{prompt}")) {
        if let Some(m) = msgs.iter_mut().rev().find(|m| m.get("role").and_then(Value::as_str) == Some("user")) {
            match m.get_mut("content") {
                Some(Value::String(s)) => *s = template.replace("{prompt}", s),
                Some(Value::Array(parts)) => {
                    for p in parts.iter_mut().rev() {
                        if let Some(Value::String(t)) = p.get_mut("text") {
                            *t = template.replace("{prompt}", t);
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    msgs
}

/// `extra` merged into `base`: objects merge key by key, anything else replaces.
pub fn deep_merge(base: &mut Map<String, Value>, extra: &Map<String, Value>) {
    for (key, value) in extra {
        match (base.get_mut(key), value) {
            (Some(Value::Object(b)), Value::Object(e)) => deep_merge(b, e),
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

/// The JSON body for POST {api_root}/chat/completions. `messages` are already prepared.
pub fn build_body(model: &str, messages: &[Value], params: &RunParams, sample_idx: i64, include_usage: bool) -> Value {
    build_request(model, messages, params, sample_idx, include_usage, None)
}

/// [`build_body`] with `tools` and `tool_choice` when `tools` is given (before `extra_body`, which
/// can still override them, e.g. with `parallel_tool_calls`).
pub fn build_request(
    model: &str,
    messages: &[Value],
    params: &RunParams,
    sample_idx: i64,
    include_usage: bool,
    tools: Option<RequestTools<'_>>,
) -> Value {
    let mut body = Map::new();
    body.insert("model".into(), Value::String(model.to_string()));
    body.insert("messages".into(), Value::Array(messages.to_vec()));
    if let Some(t) = params.temperature {
        body.insert("temperature".into(), t.into());
    }
    if let Some(p) = params.top_p {
        body.insert("top_p".into(), p.into());
    }
    if let Some(seed) = params.seed {
        body.insert("seed".into(), (seed + sample_idx).into());
    }
    if let Some(m) = params.max_tokens {
        body.insert(params.max_tokens_field.as_str().into(), m.into());
    }
    if params.stream {
        body.insert("stream".into(), Value::Bool(true));
        if include_usage {
            let mut so = Map::new();
            so.insert("include_usage".into(), Value::Bool(true));
            body.insert("stream_options".into(), Value::Object(so));
        }
    } else {
        body.insert("stream".into(), Value::Bool(false));
    }
    if let Some(t) = tools {
        body.insert("tools".into(), Value::Array(t.definitions.to_vec()));
        body.insert("tool_choice".into(), Value::String(t.choice.to_string()));
    }
    deep_merge(&mut body, &params.extra_body);
    Value::Object(body)
}

// ------------------------------------------------------------------------------------ reasoning split

/// (channel, body) segments of gpt-oss harmony text, like Python's `re.findall` with
/// `<|channel|>([^<]*?)(?:<|constrain|>[^<]*)?<|message|>(.*?)(?=<|end|>|<|return|>|<|call|>|<|start|>|<|channel|>|\Z)`.
fn harmony_segments(text: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(rel) = text[pos..].find(HARMONY_CHANNEL) {
        let at = pos + rel;
        let start = at + HARMONY_CHANNEL.len();
        // The channel name runs to the next '<' (it cannot contain one).
        let Some(lt) = text[start..].find('<').map(|i| start + i) else { break };
        let channel = &text[start..lt];
        let body_start = if text[lt..].starts_with(HARMONY_MESSAGE) {
            Some(lt + HARMONY_MESSAGE.len())
        } else if text[lt..].starts_with(HARMONY_CONSTRAIN) {
            let after = lt + HARMONY_CONSTRAIN.len();
            match text[after..].find('<').map(|i| after + i) {
                Some(l2) if text[l2..].starts_with(HARMONY_MESSAGE) => Some(l2 + HARMONY_MESSAGE.len()),
                _ => None,
            }
        } else {
            None
        };
        match body_start {
            None => pos = at + 1,
            Some(b) => {
                let end = HARMONY_ENDS.iter().filter_map(|t| text[b..].find(t)).min().map_or(text.len(), |i| b + i);
                out.push((channel, &text[b..end]));
                pos = end;
            }
        }
    }
    out
}

fn split_harmony(text: &str) -> Option<(Option<String>, String)> {
    let segments = harmony_segments(text);
    if segments.is_empty() {
        return None;
    }
    let mut thinking: Vec<&str> = Vec::new();
    let mut fin: Vec<&str> = Vec::new();
    for (channel, body) in segments {
        let name = channel.trim().split(' ').next().unwrap_or("");
        if name == "final" {
            fin.push(body.trim());
        } else {
            thinking.push(body.trim());
        }
    }
    let reasoning = thinking.into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    Some((non_empty(&reasoning), fin.join("\n\n")))
}

/// Split thinking from the answer when the server returned it inline in `content`.
///
/// Handles `<think>X</think>Y`, `X</think>Y` (the chat template opened the tag), `<think>X` with
/// no close (truncated: all thinking, empty answer) and gpt-oss harmony text that leaked through.
/// Only whitespace next to a split is stripped; everything else is kept raw.
pub fn split_inline(text: &str, think_open: &str, think_close: &str) -> (Option<String>, String) {
    if text.is_empty() {
        return (None, String::new());
    }
    if text.contains(HARMONY_CHANNEL) && text.contains(HARMONY_MESSAGE) {
        if let Some(h) = split_harmony(text) {
            return h;
        }
    }
    if !think_open.is_empty() {
        if let Some(rest) = text.trim_start().strip_prefix(think_open) {
            let idx = if think_close.is_empty() { None } else { rest.find(think_close) };
            if let Some(i) = idx {
                return (non_empty(rest[..i].trim()), rest[i + think_close.len()..].trim_start().to_string());
            }
            return (non_empty(rest.trim_start()), String::new());
        }
    }
    if !think_close.is_empty() {
        if let Some(idx) = text.find(think_close) {
            let before = &text[..idx];
            if !think_open.is_empty() && before.contains(think_open) {
                // Text, then a think block in the middle: not a thinking template. Keep it raw.
                return (None, text.to_string());
            }
            return (non_empty(before.trim()), text[idx + think_close.len()..].trim_start().to_string());
        }
    }
    (None, text.to_string())
}

// ------------------------------------------------------------------------------------ errors

/// The error message inside an error response body, if it has one (at most 300 characters).
pub fn server_message(body: &str) -> Option<String> {
    let body = body.trim();
    if body.is_empty() {
        return None;
    }
    let data: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => {
            if body.starts_with('<') {
                return None; // an HTML error page from a proxy
            }
            return non_empty(first_chars(&one_line(body), 300));
        }
    };
    let pick = |o: &Map<String, Value>, keys: &[&str]| -> Option<Value> {
        keys.iter().filter_map(|k| o.get(*k)).find(|v| truthy(Some(v))).cloned()
    };
    let mut msg: Option<Value> = None;
    match &data {
        Value::Object(o) => {
            match o.get("error") {
                Some(Value::Object(err)) => msg = pick(err, &["message", "msg", "detail", "type"]),
                Some(Value::String(s)) if !s.is_empty() => msg = Some(Value::String(s.clone())),
                _ => {}
            }
            if !truthy(msg.as_ref()) {
                match o.get("detail") {
                    Some(Value::String(s)) => msg = Some(Value::String(s.clone())),
                    Some(Value::Array(list)) => {
                        if let Some(Value::Object(first)) = list.first() {
                            msg = first.get("msg").cloned();
                        }
                    }
                    _ => {}
                }
            }
            if !truthy(msg.as_ref()) {
                msg = pick(o, &["message", "msg"]);
            }
        }
        Value::String(s) => msg = Some(Value::String(s.clone())),
        _ => {}
    }
    let text = match msg? {
        Value::Null => return None,
        Value::String(s) => s,
        other => other.to_string(),
    };
    non_empty(first_chars(&one_line(&text), 300))
}

/// One readable sentence for an HTTP error status, with the server's message when it has one.
pub fn describe_status(status: u16, body: &str, reason_phrase: &str) -> String {
    if let Some(msg) = server_message(body) {
        return match status {
            401 => sentence(&format!("The server rejected the API key (401): {msg}"), 400),
            403 => sentence(&format!("The API key is not allowed to do this (403): {msg}"), 400),
            _ => sentence(&format!("The server answered {status}: {msg}"), 400),
        };
    }
    if matches!(status, 401 | 403 | 404) {
        return openai::describe_status(status, "");
    }
    sentence(format!("The server answered {status} {reason_phrase}").trim(), 400)
}

pub fn is_retryable_status(status: u16) -> bool {
    RETRY_STATUSES.contains(&status)
}

/// Connect, read and protocol errors and timeouts are worth another try; a malformed request is not.
pub fn is_retryable_error(e: &reqwest::Error) -> bool {
    e.is_timeout() || e.is_connect() || e.is_request() || e.is_body() || e.is_decode()
}

/// One sentence for a failed request.
pub fn describe_error(e: &reqwest::Error) -> String {
    if e.is_timeout() || e.is_connect() {
        return sentence(&openai::describe_reqwest_error(e), 400);
    }
    if e.is_body() || e.is_decode() || e.is_request() {
        return DROPPED.into();
    }
    sentence(&openai::describe_reqwest_error(e), 400)
}

fn transport_error(e: &reqwest::Error) -> AttemptError {
    AttemptError::new(describe_error(e), is_retryable_error(e))
}

/// Seconds to wait from `retry-after-ms`, or `Retry-After` (seconds or an HTTP date).
pub fn parse_retry_after(headers: &HeaderMap) -> Option<f64> {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty());
    if let Some(ms) = get("retry-after-ms") {
        if let Ok(v) = ms.parse::<f64>() {
            return Some((v / 1000.0).max(0.0));
        }
    }
    let value = get("retry-after")?;
    if let Ok(v) = value.parse::<f64>() {
        return Some(v.max(0.0));
    }
    let when = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let secs = (when.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_milliseconds() as f64 / 1000.0;
    Some(secs.max(0.0))
}

/// Seconds to wait before attempt `attempt + 1`: Retry-After when given (at most 300 s), else
/// 1, 2, 4… s capped at 30 s, with ±25% jitter.
pub fn backoff_delay(attempt: i64, retry_after: Option<f64>) -> f64 {
    if let Some(r) = retry_after {
        return r.min(MAX_RETRY_AFTER_S);
    }
    let exp = (attempt - 1).clamp(0, 30) as i32;
    let base = MAX_BACKOFF_S.min(2f64.powi(exp));
    MAX_BACKOFF_S.min(base * (0.75 + fastrand::f64() * 0.5))
}

// ------------------------------------------------------------------------------------ response parsing

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ContentMode {
    Think,
    Harmony,
    Answer,
}

/// Collects one attempt's output from stream chunks or a full message.
struct Accumulator<'a> {
    think_open: &'a str,
    think_close: &'a str,
    progress: &'a mut Progress,
    sink: &'a dyn ProgressSink,
    t0: Instant,
    reasoning: String,
    details: String,
    content: String,
    saw_reasoning: bool,
    saw_details: bool,
    finish_reason: Option<String>,
    usage: Option<Map<String, Value>>,
    chunks: i64,
    ttft_ms: Option<i64>,
    done: bool,
    /// How streamed content is shown live; None until decided.
    content_mode: Option<ContentMode>,
    content_head: String,
    /// Tool calls being assembled, in the order they started.
    calls: Vec<PartialCall>,
    /// The server's `index` of a call -> its place in `calls`.
    call_index: HashMap<u64, usize>,
}

/// A tool call while its pieces arrive.
#[derive(Default, Debug)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
}

impl<'a> Accumulator<'a> {
    fn new(params: &'a RunParams, progress: &'a mut Progress, sink: &'a dyn ProgressSink, t0: Instant) -> Self {
        Self {
            think_open: &params.think_open,
            think_close: &params.think_close,
            progress,
            sink,
            t0,
            reasoning: String::new(),
            details: String::new(),
            content: String::new(),
            saw_reasoning: false,
            saw_details: false,
            finish_reason: None,
            usage: None,
            chunks: 0,
            ttft_ms: None,
            done: false,
            content_mode: None,
            content_head: String::new(),
            calls: Vec::new(),
            call_index: HashMap::new(),
        }
    }

    // ---- live view

    fn emit(&mut self, phase: Phase, text: &str) {
        if self.ttft_ms.is_none() {
            self.ttft_ms = Some(self.t0.elapsed().as_millis() as i64);
        }
        self.chunks += 1;
        let p = &mut *self.progress;
        p.tokens += 1;
        if p.phase != phase {
            p.phase = phase;
            p.tail = last_chars(text.trim_start(), TAIL_CHARS).to_string();
        } else if !p.tail.is_empty() {
            p.tail.push_str(text);
            keep_last_chars(&mut p.tail, TAIL_CHARS);
        } else {
            // The tail is for display: no leading blank lines.
            p.tail = last_chars(text.trim_start(), TAIL_CHARS).to_string();
        }
        self.sink.on_chunk(p);
    }

    /// If `marker` completes in this chunk, the text after it.
    fn switch_after(&self, marker: &str, text: &str) -> Option<String> {
        let mut window = last_chars(&self.progress.tail, marker.chars().count()).to_string();
        window.push_str(text);
        let idx = window.find(marker)?;
        Some(window[idx + marker.len()..].to_string())
    }

    fn content_chunk(&mut self, text: &str) {
        if self.saw_reasoning {
            self.emit(Phase::Answering, text);
            return;
        }
        let Some(mode) = self.content_mode else {
            self.content_head.push_str(text);
            let head = self.content_head.trim_start();
            let open = self.think_open;
            let mode = if head.is_empty() {
                None
            } else if !open.is_empty() && head.starts_with(open) {
                Some(ContentMode::Think)
            } else if head.starts_with("<|") {
                Some(ContentMode::Harmony)
            } else if (!open.is_empty() && open.starts_with(head)) || HARMONY_CHANNEL.starts_with(head) {
                None // could still become a tag
            } else {
                Some(ContentMode::Answer)
            };
            self.content_mode = mode;
            match mode {
                None => {
                    // Count the token but keep the phase until we know what this is.
                    let phase =
                        if self.progress.phase != Phase::Waiting { self.progress.phase } else { Phase::Answering };
                    self.emit(phase, text);
                }
                Some(ContentMode::Think) => {
                    let after = head[open.len()..].to_string();
                    self.content_head = String::new();
                    let idx = if self.think_close.is_empty() { None } else { after.find(self.think_close) };
                    if let Some(i) = idx {
                        // Open and close in one go.
                        self.progress.tail.clear();
                        let rest = after[i + self.think_close.len()..].trim_start().to_string();
                        self.emit(Phase::Answering, &rest);
                        return;
                    }
                    self.progress.phase = Phase::Thinking;
                    self.progress.tail.clear();
                    self.emit(Phase::Thinking, &after);
                }
                Some(ContentMode::Harmony) => {
                    self.content_head = String::new();
                    self.emit(Phase::Thinking, text);
                }
                Some(ContentMode::Answer) => {
                    self.content_head = String::new();
                    self.emit(Phase::Answering, text);
                }
            }
            return;
        };
        if mode == ContentMode::Think && self.progress.phase == Phase::Thinking {
            let after = if self.think_close.is_empty() { None } else { self.switch_after(self.think_close, text) };
            match after {
                Some(after) => {
                    self.progress.phase = Phase::Answering;
                    self.progress.tail.clear();
                    self.emit(Phase::Answering, after.trim_start());
                }
                None => self.emit(Phase::Thinking, text),
            }
            return;
        }
        if mode == ContentMode::Harmony && self.progress.phase == Phase::Thinking {
            match self.switch_after(HARMONY_FINAL_MARK, text) {
                Some(after) => {
                    let after = match after.split_once(HARMONY_MESSAGE) {
                        Some((_, rest)) => rest.to_string(),
                        None => after,
                    };
                    self.progress.phase = Phase::Answering;
                    self.progress.tail.clear();
                    self.emit(Phase::Answering, &after);
                }
                None => self.emit(Phase::Thinking, text),
            }
            return;
        }
        if mode == ContentMode::Answer && !self.think_close.is_empty() {
            // The chat template may have opened the think tag: everything so far was thinking.
            if let Some(after) = self.switch_after(self.think_close, text) {
                self.progress.tail.clear();
                self.emit(Phase::Answering, after.trim_start());
                return;
            }
        }
        self.emit(Phase::Answering, text);
    }

    // ---- parsing

    fn feed_delta(&mut self, delta: &Map<String, Value>) {
        let mut reasoning: Option<&str> = None;
        for key in REASONING_FIELDS {
            if let Some(Value::String(s)) = delta.get(key) {
                if !s.is_empty() {
                    reasoning = Some(s);
                    break;
                }
            }
        }
        if let Some(r) = reasoning {
            self.saw_reasoning = true;
            self.reasoning.push_str(r);
            self.emit(Phase::Thinking, r);
        }
        if let Some(Value::Array(details)) = delta.get("reasoning_details") {
            let mut joined = String::new();
            for item in details {
                if let Value::Object(o) = item {
                    let t = match o.get("text") {
                        Some(Value::String(s)) => Some(s),
                        _ => match o.get("summary") {
                            Some(Value::String(s)) => Some(s),
                            _ => None,
                        },
                    };
                    if let Some(t) = t.filter(|t| !t.is_empty()) {
                        joined.push_str(t);
                    }
                }
            }
            if !joined.is_empty() {
                self.saw_details = true;
                self.details.push_str(&joined);
                if reasoning.is_none() && !self.saw_reasoning {
                    self.emit(Phase::Thinking, &joined);
                }
            }
        }
        let content = content_text(delta.get("content"));
        if !content.is_empty() {
            self.content.push_str(&content);
            self.content_chunk(&content);
        }
        if let Some(Value::Array(calls)) = delta.get("tool_calls") {
            for item in calls {
                self.tool_call_piece(item, item.get("function"));
            }
        }
        if let Some(fc @ Value::Object(_)) = delta.get("function_call") {
            // The legacy single function call: one call at index 0.
            self.tool_call_piece(&serde_json::json!({"index": 0}), Some(fc));
        }
    }

    /// One piece of a tool call: `item` holds `index` and `id`, `function` the name and arguments.
    fn tool_call_piece(&mut self, item: &Value, function: Option<&Value>) {
        let id = item.get("id").and_then(Value::as_str).filter(|s| !s.is_empty());
        let index = item.get("index").and_then(Value::as_u64);
        let slot = match (index, id) {
            (Some(i), _) => match self.call_index.get(&i).copied() {
                // A new id at a used index starts another call (servers that send whole calls,
                // all at index 0).
                Some(s) if id.is_none_or(|id| self.calls[s].id.is_empty() || self.calls[s].id == id) => s,
                _ => {
                    self.calls.push(PartialCall::default());
                    self.call_index.insert(i, self.calls.len() - 1);
                    self.calls.len() - 1
                }
            },
            (None, Some(id)) => match self.calls.iter().position(|c| c.id == id) {
                Some(s) => s,
                None => {
                    self.calls.push(PartialCall::default());
                    self.calls.len() - 1
                }
            },
            (None, None) => {
                if self.calls.is_empty() {
                    self.calls.push(PartialCall::default());
                }
                self.calls.len() - 1
            }
        };
        let started = id.is_some();
        let call = &mut self.calls[slot];
        if let Some(id) = id {
            call.id = id.to_string();
        }
        let mut shown = String::new();
        if let Some(f) = function {
            if let Some(name) = f.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()) {
                if call.name.is_empty() || started {
                    call.name = name.to_string();
                } else if call.name != name {
                    call.name.push_str(name);
                }
                shown = format!("{name}(");
            }
            match f.get("arguments") {
                Some(Value::String(a)) => {
                    call.arguments.push_str(a);
                    shown.push_str(a);
                }
                Some(v @ (Value::Object(_) | Value::Array(_))) => {
                    // A server that sends the arguments as JSON instead of text.
                    call.arguments = v.to_string();
                    shown.push_str(&call.arguments);
                }
                _ => {}
            }
        }
        if !shown.is_empty() {
            self.emit(Phase::Answering, &shown);
        }
    }

    fn feed_event(&mut self, obj: &Value) -> Result<(), AttemptError> {
        let Value::Object(obj) = obj else { return Ok(()) };
        if let Some(err) = obj.get("error").filter(|e| truthy(Some(e))) {
            let wrapped = serde_json::json!({ "error": err }).to_string();
            let msg = server_message(&wrapped).unwrap_or_else(|| "unknown error".into());
            let status = err.get("code").and_then(Value::as_i64).and_then(|c| u16::try_from(c).ok());
            let retryable = status.map_or(true, is_retryable_status);
            let mut e = AttemptError::new(sentence(&format!("The server reported an error mid-answer: {msg}"), 400), retryable);
            e.status = status;
            return Err(e);
        }
        if let Some(Value::Object(u)) = obj.get("usage") {
            if !u.is_empty() {
                self.usage = Some(u.clone());
            }
        }
        let Some(Value::Array(choices)) = obj.get("choices") else { return Ok(()) };
        for choice in choices {
            let Value::Object(choice) = choice else { continue };
            match choice.get("index") {
                None | Some(Value::Null) => {}
                Some(v) if v.as_f64() == Some(0.0) => {}
                Some(_) => continue,
            }
            let empty = Map::new();
            let delta = match choice.get("delta") {
                Some(Value::Object(d)) => d,
                _ => match choice.get("message") {
                    Some(Value::Object(m)) => m,
                    _ => &empty,
                },
            };
            self.feed_delta(delta);
            if let Some(f) = choice.get("finish_reason").filter(|f| truthy(Some(f))) {
                self.finish_reason = Some(match f {
                    Value::String(s) => s.clone(),
                    other => py_str(other),
                });
            }
        }
        Ok(())
    }

    fn data(&mut self, data: &str) -> Result<(), AttemptError> {
        if data.trim() == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        match serde_json::from_str::<Value>(data) {
            Ok(obj) => self.feed_event(&obj),
            Err(_) => Ok(()),
        }
    }

    /// One line of an SSE stream. Returns true when the stream is finished.
    fn line(&mut self, line: &str, pending: &mut Vec<String>) -> Result<bool, AttemptError> {
        if line.trim().is_empty() {
            if !pending.is_empty() {
                let data = pending.join("\n");
                pending.clear();
                self.data(&data)?;
                if self.done {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        if line.starts_with(':') {
            return Ok(false);
        }
        let data = if let Some(d) = line.strip_prefix("data:") {
            d.strip_prefix(' ').unwrap_or(d)
        } else if line.trim_start().starts_with('{') {
            line // newline-delimited JSON without the SSE prefix
        } else {
            return Ok(false); // event:, id:, retry:
        };
        if !pending.is_empty() {
            pending.push(data.to_string());
            return Ok(false);
        }
        if data.trim() == "[DONE]" {
            self.done = true;
            return Ok(true);
        }
        match serde_json::from_str::<Value>(data) {
            Ok(obj) => self.feed_event(&obj)?,
            Err(_) => pending.push(data.to_string()),
        }
        Ok(false)
    }

    async fn read_sse(&mut self, resp: reqwest::Response) -> Result<(), AttemptError> {
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut pending: Vec<String> = Vec::new();
        'read: while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| transport_error(&e))?;
            buf.extend_from_slice(&chunk);
            let mut start = 0;
            while let Some(pos) = buf[start..].iter().position(|b| *b == b'\n') {
                let end = start + pos;
                let mut line = &buf[start..end];
                if line.last() == Some(&b'\r') {
                    line = &line[..line.len() - 1];
                }
                let text = String::from_utf8_lossy(line).into_owned();
                start = end + 1;
                if self.line(&text, &mut pending)? {
                    break 'read;
                }
            }
            buf.drain(..start);
        }
        if !self.done && !buf.is_empty() {
            let mut line = &buf[..];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            let text = String::from_utf8_lossy(line).into_owned();
            self.line(&text, &mut pending)?;
        }
        if !pending.is_empty() && !self.done {
            let data = pending.join("\n");
            self.data(&data)?;
        }
        Ok(())
    }

    fn read_message(&mut self, data: &Value) -> Result<(), AttemptError> {
        if !data.is_object() {
            return Err(AttemptError::new("The server answered with something that is not a chat completion.", false));
        }
        if truthy(data.get("error")) {
            let msg = server_message(&data.to_string()).unwrap_or_else(|| "unknown error".into());
            return Err(AttemptError::new(sentence(&format!("The server reported an error: {msg}"), 400), true));
        }
        self.feed_event(data)?;
        self.done = true;
        Ok(())
    }

    fn result(self, attempts: i64, latency_ms: i64, streamed: bool) -> GenerationResult {
        let mut reasoning = if self.saw_reasoning {
            Some(self.reasoning)
        } else if self.saw_details {
            Some(self.details)
        } else {
            None
        };
        let mut response = self.content;
        if reasoning.as_deref().map_or(true, str::is_empty) {
            let (r, resp) = split_inline(&response, self.think_open, self.think_close);
            reasoning = r;
            response = resp;
        }
        let usage = self.usage.unwrap_or_default();
        let mut reasoning_tokens = match usage.get("completion_tokens_details") {
            Some(Value::Object(d)) => d.get("reasoning_tokens").cloned(),
            _ => None,
        };
        if reasoning_tokens.as_ref().map_or(true, Value::is_null) {
            reasoning_tokens = usage.get("reasoning_tokens").cloned();
        }
        let tool_calls = self
            .calls
            .into_iter()
            .enumerate()
            .filter(|(_, c)| !c.name.is_empty() || !c.arguments.is_empty())
            .map(|(i, c)| ToolCall {
                id: if c.id.is_empty() { format!("call_{}", i + 1) } else { c.id },
                name: c.name,
                arguments: c.arguments,
            })
            .collect();
        GenerationResult {
            tool_calls,
            reasoning: reasoning.filter(|r| !r.is_empty()),
            response,
            finish_reason: self.finish_reason,
            prompt_tokens: int_or_none(usage.get("prompt_tokens")),
            completion_tokens: int_or_none(usage.get("completion_tokens")),
            reasoning_tokens: int_or_none(reasoning_tokens.as_ref()),
            latency_ms,
            // Unknown when the answer came in one piece.
            ttft_ms: if streamed { self.ttft_ms } else { None },
            attempts,
            chunks: self.chunks,
        }
    }
}

fn int_or_none(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().filter(|f| f.is_finite()).map(|f| f.trunc() as i64)),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

// ------------------------------------------------------------------------------------ sending

async fn attempt_once(
    client: &reqwest::Client,
    url: &str,
    body: &Value,
    params: &RunParams,
    progress: &mut Progress,
    sink: &dyn ProgressSink,
    attempt: i64,
) -> Result<GenerationResult, AttemptError> {
    let t0 = Instant::now();
    let mut acc = Accumulator::new(params, progress, sink, t0);
    let resp = client.post(url).json(body).send().await.map_err(|e| transport_error(&e))?;
    let status = resp.status();
    if status.as_u16() >= 400 {
        let retry_after = parse_retry_after(resp.headers());
        let raw = resp.bytes().await.map_err(|e| transport_error(&e))?;
        let text = String::from_utf8_lossy(&raw);
        return Err(AttemptError {
            message: describe_status(status.as_u16(), &text, status.canonical_reason().unwrap_or("")),
            retryable: is_retryable_status(status.as_u16()),
            status: Some(status.as_u16()),
            retry_after,
            body: first_chars(&text, 2000).to_string(),
        });
    }
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let wants_stream = body.get("stream").is_some_and(|v| truthy(Some(v)));
    let streamed = ctype.contains("text/event-stream") || (wants_stream && !ctype.contains("json"));
    if streamed {
        acc.read_sse(resp).await?;
        if !acc.done && acc.finish_reason.is_none() {
            return Err(AttemptError::new("The stream ended before the server finished the answer.", true));
        }
    } else {
        let raw = resp.bytes().await.map_err(|e| transport_error(&e))?;
        let data: Value = serde_json::from_slice(&raw)
            .map_err(|_| AttemptError::new("The server answered with something that is not JSON.", true))?;
        acc.read_message(&data)?;
    }
    let latency_ms = t0.elapsed().as_millis() as i64;
    Ok(acc.result(attempt, latency_ms, streamed))
}

fn mentions_stream_options(err: &AttemptError) -> bool {
    let text = if err.body.is_empty() { &err.message } else { &err.body }.to_lowercase();
    text.contains("stream_options") || text.contains("include_usage")
}

/// Run one sample to completion. `messages` are the prompt as sent (see [`prepare_messages`]).
///
/// Retries connect/read errors, timeouts and 408/409/425/429/5xx with backoff, up to
/// `params.max_attempts` attempts, each limited to `params.request_timeout_s`. A 400 or 422 to a
/// body with `stream_options` is retried once without it (not counted as an attempt); when that
/// works, or the error names the field, the run stops sending it (`caps`).
#[allow(clippy::too_many_arguments)]
pub async fn generate(
    client: &reqwest::Client,
    url: &str,
    model: &str,
    messages: &[Value],
    params: &RunParams,
    sample_idx: i64,
    caps: &ServerCaps,
    sink: &dyn ProgressSink,
    backoff: &dyn Backoff,
) -> Result<GenerationResult, GenerateError> {
    generate_round(client, url, model, messages, params, sample_idx, caps, sink, backoff, None, 1).await
}

/// True when a 400 or 422 answer to a request with tools says the server does not take them: the
/// error names tools, tool_choice or tool calling.
pub fn rejects_tools(err: &AttemptError) -> bool {
    if !matches!(err.status, Some(400) | Some(422)) {
        return false;
    }
    let text = format!("{} {}", err.message, err.body).to_lowercase();
    text.contains("tool") || text.contains("function call")
}

/// The run-pausing message for an endpoint that refused the tools.
pub fn tools_rejected_message(server_message: &str) -> String {
    let detail = server_message.trim().trim_end_matches('.');
    format!(
        "The endpoint refused the tool definitions ({detail}). Runs with tools need an endpoint with tool calling \
         turned on; for vLLM, start it with --enable-auto-tool-choice and --tool-call-parser set to the model's parser. See {TOOLS_DOC}"
    )
}

/// One request of a sample, with retries, as [`generate`], plus `tools` and the request's number
/// in the sample (`round`, for the live view).
#[allow(clippy::too_many_arguments)]
pub async fn generate_round(
    client: &reqwest::Client,
    url: &str,
    model: &str,
    messages: &[Value],
    params: &RunParams,
    sample_idx: i64,
    caps: &ServerCaps,
    sink: &dyn ProgressSink,
    backoff: &dyn Backoff,
    tools: Option<RequestTools<'_>>,
    round: i64,
) -> Result<GenerationResult, GenerateError> {
    let mut progress = Progress { round, ..Progress::default() };
    let mut attempt: i64 = 0;
    let mut send_stream_options = true;
    let mut probe_attempt: i64 = 0; // the attempt sent without stream_options after a 400, if any
    let timeout = Duration::from_secs_f64(params.request_timeout_s.max(0.001));
    loop {
        attempt += 1;
        let include_usage = send_stream_options && caps.stream_options();
        let body = build_request(model, messages, params, sample_idx, include_usage, tools);
        progress.phase = Phase::Waiting;
        progress.tokens = 0;
        progress.tail.clear();
        progress.attempt = attempt;
        sink.on_reset(&progress);
        let outcome =
            tokio::time::timeout(timeout, attempt_once(client, url, &body, params, &mut progress, sink, attempt)).await;
        let error = match outcome {
            Ok(Ok(result)) => {
                if probe_attempt != 0 && probe_attempt == attempt {
                    caps.reject_stream_options(); // it worked without: remember for the run
                }
                return Ok(result);
            }
            Ok(Err(e)) => e,
            Err(_) => AttemptError::new(
                format!(
                    "The request did not finish within the run's request timeout of {} s.",
                    fmt_g(params.request_timeout_s)
                ),
                true,
            ),
        };
        if tools.is_some() && rejects_tools(&error) && !mentions_stream_options(&error) {
            return Err(GenerateError::ToolsRejected { message: tools_rejected_message(&error.message) });
        }
        if matches!(error.status, Some(400) | Some(422)) && body.get("stream_options").is_some() && probe_attempt == 0 {
            // Some servers reject stream_options. Try once without it (not counted as an attempt).
            send_stream_options = false;
            if mentions_stream_options(&error) {
                caps.reject_stream_options();
            }
            attempt -= 1;
            probe_attempt = attempt + 1;
            continue;
        }
        if tools.is_some() && rejects_tools(&error) {
            return Err(GenerateError::ToolsRejected { message: tools_rejected_message(&error.message) });
        }
        if !error.retryable || attempt >= params.max_attempts {
            return Err(GenerateError::Failed { message: error.message, attempts: attempt });
        }
        progress.phase = Phase::Waiting;
        progress.tail.clear();
        sink.on_reset(&progress);
        if !backoff.sleep(backoff_delay(attempt, error.retry_after)).await {
            return Err(GenerateError::Interrupted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn split(text: &str) -> (Option<String>, String) {
        split_inline(text, "<think>", "</think>")
    }

    fn pair(r: Option<&str>, a: &str) -> (Option<String>, String) {
        (r.map(str::to_string), a.to_string())
    }

    #[test]
    fn split_inline_cases() {
        let cases: &[(&str, Option<&str>, &str)] = &[
            ("<think>X</think>Y", Some("X"), "Y"),
            ("<think>\nLet me think.\n</think>\n\nThe answer is 4.", Some("Let me think."), "The answer is 4."),
            ("  <think>X</think>Y", Some("X"), "Y"),
            ("Let me think.\n</think>\n\nThe answer is 4.", Some("Let me think."), "The answer is 4."),
            ("<think>\nStill thinking when the tokens ran out", Some("Still thinking when the tokens ran out"), ""),
            ("<think>\n\n</think>\n\nNo thinking.", None, "No thinking."),
            ("Just an answer.", None, "Just an answer."),
            ("", None, ""),
            // Raw text is kept: only whitespace next to the split goes.
            ("<think>a  b</think>\n\nY  \n\n  Z  ", Some("a  b"), "Y  \n\n  Z  "),
            ("Text first <think>x</think> y", None, "Text first <think>x</think> y"),
        ];
        for (text, r, a) in cases {
            assert_eq!(split(text), pair(*r, a), "{text:?}");
        }
    }

    #[test]
    fn split_inline_custom_tags() {
        assert_eq!(split_inline("<reasoning>X</reasoning> Y", "<reasoning>", "</reasoning>"), pair(Some("X"), "Y"));
        // The default tags are left alone when other tags are configured.
        assert_eq!(
            split_inline("<think>X</think>Y", "<reasoning>", "</reasoning>"),
            pair(None, "<think>X</think>Y")
        );
    }

    #[test]
    fn split_harmony_cases() {
        let text = "<|channel|>analysis<|message|>We need to add 2 and 2.<|end|>\
                    <|start|>assistant<|channel|>final<|message|>4<|return|>";
        assert_eq!(split(text), pair(Some("We need to add 2 and 2."), "4"));
        // Truncated in the analysis channel: all thinking.
        assert_eq!(split("<|channel|>analysis<|message|>We need to"), pair(Some("We need to"), ""));
        // Only a final channel.
        assert_eq!(split("<|start|>assistant<|channel|>final<|message|>Hi<|end|>"), pair(None, "Hi"));
        // A constrained tool call counts as thinking.
        let t = "<|channel|>commentary to=functions.x <|constrain|>json<|message|>{\"a\":1}<|call|>\
                 <|channel|>final<|message|>Done";
        assert_eq!(split(t), pair(Some("{\"a\":1}"), "Done"));
    }

    #[test]
    fn prepare_messages_system_prompt_and_template() {
        let msgs = vec![
            json!({"role": "system", "content": "old system"}),
            json!({"role": "user", "content": "first"}),
            json!({"role": "assistant", "content": "reply"}),
            json!({"role": "user", "content": "f(x) = {x} and {y}"}),
        ];
        let params = RunParams {
            system_prompt: Some("Be brief.".into()),
            user_template: Some("Solve: {prompt}\nShow {work}.".into()),
            ..RunParams::default()
        };
        let out = prepare_messages(&msgs, &params);
        assert_eq!(out[0], json!({"role": "system", "content": "Be brief."}));
        let roles: Vec<&str> = out.iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["system", "user", "assistant", "user"]);
        assert_eq!(out[1]["content"], "first"); // only the last user message is wrapped
        assert_eq!(out[3]["content"], "Solve: f(x) = {x} and {y}\nShow {work}.");
        assert_eq!(msgs[3]["content"], "f(x) = {x} and {y}"); // input untouched
    }

    #[test]
    fn prepare_messages_defaults_keep_prompt() {
        let msgs = vec![json!({"role": "system", "content": "s"}), json!({"role": "user", "content": "u"})];
        assert_eq!(prepare_messages(&msgs, &RunParams::default()), msgs);
        let blank = RunParams { system_prompt: Some("  ".into()), ..RunParams::default() };
        assert_eq!(prepare_messages(&msgs, &blank), msgs); // blank means unset
        // List content: the last text part is wrapped.
        let parts = vec![json!({"role": "user", "content": [{"type": "text", "text": "a"}, {"type": "image_url"}]})];
        let t = RunParams { user_template: Some("Q: {prompt}".into()), ..RunParams::default() };
        assert_eq!(prepare_messages(&parts, &t)[0]["content"][0]["text"], "Q: a");
    }

    #[test]
    fn build_body_fields_and_merge() {
        let extra = json!({"chat_template_kwargs": {"enable_thinking": true}, "temperature": 1.0,
                           "stream_options": {"continuous_usage_stats": true}});
        let params = RunParams {
            temperature: Some(0.6),
            top_p: Some(0.95),
            max_tokens: Some(1000),
            max_tokens_field: stellarator_core::schemas::MaxTokensField::MaxCompletionTokens,
            seed: Some(42),
            extra_body: extra.as_object().unwrap().clone(),
            ..RunParams::default()
        };
        let msgs = vec![json!({"role": "user", "content": "What is 2 + 2?"})];
        let body = build_body("m", &msgs, &params, 3, true);
        assert_eq!(body["model"], "m");
        assert_eq!(body["messages"], json!(msgs));
        assert_eq!(body["top_p"], 0.95);
        assert_eq!(body["seed"], 45);
        assert_eq!(body["max_completion_tokens"], 1000);
        assert!(body.get("max_tokens").is_none());
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"], json!({"include_usage": true, "continuous_usage_stats": true}));
        assert_eq!(body["chat_template_kwargs"], json!({"enable_thinking": true}));
        assert_eq!(body["temperature"], 1.0); // extra_body is merged last
    }

    #[test]
    fn build_body_omits_unset() {
        let params = RunParams { temperature: None, top_p: None, max_tokens: None, seed: None, stream: false, ..RunParams::default() };
        let msgs = vec![json!({"role": "user", "content": "x"})];
        assert_eq!(build_body("m", &msgs, &params, 0, true), json!({"model": "m", "messages": msgs, "stream": false}));
        assert!(build_body("m", &msgs, &RunParams::default(), 0, false).get("stream_options").is_none());
    }

    #[test]
    fn retry_classification() {
        for status in [408, 409, 425, 429, 500, 502, 503, 504] {
            assert!(is_retryable_status(status));
        }
        for status in [400, 401, 403, 404, 413, 422, 501] {
            assert!(!is_retryable_status(status));
        }
    }

    #[test]
    fn backoff_and_retry_after() {
        for (attempt, base) in [(1, 1.0), (2, 2.0), (3, 4.0), (4, 8.0)] {
            let d = backoff_delay(attempt, None);
            assert!(base * 0.75 <= d && d <= base * 1.25, "{attempt} {d}");
        }
        assert!(backoff_delay(20, None) <= 30.0);
        assert_eq!(backoff_delay(1, Some(7.0)), 7.0);
        assert_eq!(backoff_delay(1, Some(900.0)), 300.0);
        let mut h = HeaderMap::new();
        h.insert("retry-after", "3".parse().unwrap());
        assert_eq!(parse_retry_after(&h), Some(3.0));
        let mut h = HeaderMap::new();
        h.insert("retry-after-ms", "1500".parse().unwrap());
        assert_eq!(parse_retry_after(&h), Some(1.5));
        let when = chrono::Utc::now() + chrono::Duration::seconds(30);
        let mut h = HeaderMap::new();
        h.insert("retry-after", when.format("%a, %d %b %Y %H:%M:%S GMT").to_string().parse().unwrap());
        let s = parse_retry_after(&h).unwrap();
        assert!((25.0..=31.0).contains(&s), "{s}");
        assert_eq!(parse_retry_after(&HeaderMap::new()), None);
    }

    #[test]
    fn describe_status_sentences() {
        assert_eq!(
            describe_status(500, r#"{"error": {"message": "Internal error"}}"#, ""),
            "The server answered 500: Internal error."
        );
        assert_eq!(
            describe_status(400, r#"{"detail": "max_tokens is too large"}"#, ""),
            "The server answered 400: max_tokens is too large."
        );
        assert_eq!(describe_status(401, "", ""), "The server rejected the API key (401).");
        assert_eq!(describe_status(401, r#"{"error": "bad key"}"#, ""), "The server rejected the API key (401): bad key.");
        assert_eq!(
            describe_status(502, "<html><body>Bad gateway</body></html>", "Bad Gateway"),
            "The server answered 502 Bad Gateway."
        );
        assert!(!describe_status(500, "line one\nline two", "").contains('\n'));
        assert_eq!(describe_status(422, r#"{"detail": [{"msg": "field required"}]}"#, ""), "The server answered 422: field required.");
    }

    #[test]
    fn sentences_and_tails() {
        assert_eq!(sentence("  a \n b ", 400), "a b.");
        assert_eq!(sentence("ok!", 400), "ok!");
        let long = "x".repeat(500);
        let s = sentence(&long, 400);
        assert_eq!(s.chars().count(), 400);
        assert!(s.ends_with('…'));
        assert_eq!(last_chars("héllo", 3), "llo");
        assert_eq!(last_chars("hé", 5), "hé");
        let mut t = "ééééé".to_string();
        keep_last_chars(&mut t, 2);
        assert_eq!(t, "éé");
        assert_eq!(fmt_g(1800.0), "1800");
        assert_eq!(fmt_g(0.05), "0.05");
    }
}
