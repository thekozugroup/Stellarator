//! Record builders: one exported record per sample (SFT) or per preference pair (DPO/KTO).
//! A port of `backend/stellarator/services/exporters.py`; key order and text match it exactly.
//!
//! Runs with tools (a `RunInfo.tools` snapshot) export each sample's whole trace:
//! - messages: the assistant turns with OpenAI `tool_calls`, a `tool` message per result, the final
//!   answer, and a top-level `tools` list (what TRL's SFTTrainer and Hugging Face chat templates
//!   take). The reasoning placement applies to every assistant turn.
//! - ShareGPT: LLaMA-Factory's `function_call` and `observation` turns, `tools` as a JSON string.
//! - raw: the stored trace, the tool count and the definitions.
//! - DPO and KTO (conversational): chosen, rejected and completion are trajectories, plus `tools`.
//! - Alpaca, prompt-completion and text preference records cannot hold tool calls: the routes refuse
//!   them for runs with tools ([`tools_refusal`]).
//!
//! JSON and JSONL keep tool arguments and the `tools` list as JSON objects. Parquet columns need one
//! type per field, so there the arguments and `tools` are JSON text (see [`Shape`]).

use chrono::{NaiveDateTime, Timelike};
use serde_json::{json, Map, Value};
use stellarator_core::schemas::{
    ExportOptions, FileType, PrefExportOptions, PrefFormat, ReasoningPlacement, SftFormat, ToolCall, ToolSnapshot,
    TraceMessage,
};

/// The run fields a record needs.
#[derive(Debug, Clone)]
pub struct RunInfo {
    pub id: i64,
    pub name: String,
    pub endpoint_name: String,
    pub model: String,
    pub prompt_set_id: i64,
    pub params: Value,
    /// The tools the run sent (None: a run without tools).
    pub tools: Option<ToolSnapshot>,
}

/// The sample columns a record needs.
#[derive(Debug, Clone, Default)]
pub struct SampleRec {
    pub id: i64,
    pub run_id: i64,
    pub prompt_id: i64,
    pub prompt_idx: i64,
    pub sample_idx: i64,
    pub reasoning: Option<String>,
    pub response: Option<String>,
    pub finish_reason: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub latency_ms: Option<i64>,
    pub ttft_ms: Option<i64>,
    pub completed_at: Option<NaiveDateTime>,
    /// The messages after the prompt (runs with tools).
    pub trace: Option<Vec<TraceMessage>>,
    pub tool_calls: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Msg {
    pub role: String,
    pub content: String,
}

impl Msg {
    pub fn new(role: &str, content: &str) -> Self {
        Self { role: role.into(), content: content.into() }
    }
    fn value(&self) -> Value {
        json!({"role": self.role, "content": self.content})
    }
}

fn msgs_value(msgs: &[Msg]) -> Value {
    Value::Array(msgs.iter().map(Msg::value).collect())
}

pub fn sft_name(f: SftFormat) -> &'static str {
    match f {
        SftFormat::Messages => "messages",
        SftFormat::Sharegpt => "sharegpt",
        SftFormat::Alpaca => "alpaca",
        SftFormat::PromptCompletion => "prompt_completion",
        SftFormat::Raw => "raw",
    }
}

pub fn pref_name(f: PrefFormat) -> &'static str {
    match f {
        PrefFormat::Dpo => "dpo",
        PrefFormat::Kto => "kto",
    }
}

pub fn placement_name(p: ReasoningPlacement) -> &'static str {
    match p {
        ReasoningPlacement::ThinkTags => "think_tags",
        ReasoningPlacement::ReasoningContent => "reasoning_content",
        ReasoningPlacement::Thinking => "thinking",
        ReasoningPlacement::Omit => "omit",
    }
}

/// The field name for placements that keep the trace outside the content.
pub fn outside_key(p: ReasoningPlacement) -> Option<&'static str> {
    match p {
        ReasoningPlacement::ReasoningContent => Some("reasoning_content"),
        ReasoningPlacement::Thinking => Some("thinking"),
        _ => None,
    }
}

// ------------------------------------------------------------------------------ prompt rendering

/// The prompt as the model saw it: run system prompt and user template applied.
pub fn render_prompt(messages: &Value, params: &Value, include_system: bool) -> Vec<Msg> {
    let mut msgs: Vec<Msg> = messages
        .as_array()
        .map(|a| {
            a.iter()
                .map(|m| Msg {
                    role: m.get("role").and_then(Value::as_str).unwrap_or("").to_string(),
                    content: match m.get("content") {
                        None | Some(Value::Null) => String::new(),
                        Some(Value::String(s)) => s.clone(),
                        Some(other) => other.to_string(),
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(sp) = params.get("system_prompt").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        msgs.retain(|m| m.role != "system");
        msgs.insert(0, Msg::new("system", sp));
    }
    if let Some(t) = params.get("user_template").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        if let Some(m) = msgs.iter_mut().rev().find(|m| m.role == "user") {
            m.content = t.replace("{prompt}", &m.content);
        }
    }
    if !include_system {
        msgs.retain(|m| m.role != "system");
    }
    msgs
}

pub fn think_wrap(reasoning: Option<&str>, response: &str) -> String {
    let reasoning = reasoning.unwrap_or("").trim_matches('\n');
    if reasoning.trim().is_empty() {
        return response.to_string();
    }
    format!("<think>\n{reasoning}\n</think>\n\n{response}")
}

fn nonblank(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|s| !s.trim().is_empty())
}

/// Assistant turn as a chat message object.
pub fn assistant_message(reasoning: &Option<String>, response: &Option<String>, placement: ReasoningPlacement) -> Value {
    let reasoning = nonblank(reasoning);
    let response = response.as_deref().unwrap_or("");
    if placement == ReasoningPlacement::ThinkTags {
        return json!({"role": "assistant", "content": think_wrap(reasoning, response)});
    }
    let mut m = Map::new();
    m.insert("role".into(), "assistant".into());
    m.insert("content".into(), response.into());
    if let Some(key) = outside_key(placement) {
        m.insert(key.into(), reasoning.map_or(Value::Null, Value::from));
    }
    Value::Object(m)
}

/// Assistant turn for formats without message objects: (text, reasoning for a side field).
pub fn assistant_text(
    reasoning: &Option<String>,
    response: &Option<String>,
    placement: ReasoningPlacement,
) -> (String, Option<String>) {
    let reasoning = nonblank(reasoning);
    let response = response.as_deref().unwrap_or("");
    match placement {
        ReasoningPlacement::ThinkTags => (think_wrap(reasoning, response), None),
        ReasoningPlacement::Omit => (response.to_string(), None),
        _ => (response.to_string(), reasoning.map(str::to_string)),
    }
}

/// Plain-text prompt: system + blank line + user, or role-labelled turns for multi-turn.
pub fn render_prompt_text(messages: &[Msg]) -> String {
    let system: Vec<&str> = messages.iter().filter(|m| m.role == "system").map(|m| m.content.as_str()).collect();
    let system = system.join("\n\n");
    let rest: Vec<&Msg> = messages.iter().filter(|m| m.role != "system").collect();
    if rest.len() == 1 && rest[0].role == "user" {
        return if system.is_empty() { rest[0].content.clone() } else { format!("{system}\n\n{}", rest[0].content) };
    }
    messages
        .iter()
        .map(|m| {
            let label = match m.role.as_str() {
                "system" => "System",
                "user" => "User",
                "assistant" => "Assistant",
                other => other,
            };
            format!("{label}: {}", m.content)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Python's `datetime.isoformat()` for a UTC time.
pub fn iso(t: Option<NaiveDateTime>) -> Value {
    match t {
        None => Value::Null,
        Some(t) => {
            let micros = t.nanosecond() / 1000;
            let base = t.format("%Y-%m-%dT%H:%M:%S").to_string();
            if micros == 0 {
                Value::from(format!("{base}+00:00"))
            } else {
                Value::from(format!("{base}.{micros:06}+00:00"))
            }
        }
    }
}

// ------------------------------------------------------------------------------ tools

/// How arbitrary JSON (tool arguments, tool definitions) is written: as JSON values (JSON, JSONL,
/// CSV, where cells are JSON text anyway) or as JSON text (Parquet, whose columns have one type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Native,
    Text,
}

impl Shape {
    pub fn of(file_type: FileType) -> Shape {
        if file_type == FileType::Parquet {
            Shape::Text
        } else {
            Shape::Native
        }
    }
}

/// Why a format cannot export a run with tools, or None when it can.
pub fn tools_refusal(format: SftFormat) -> Option<&'static str> {
    match format {
        SftFormat::Alpaca => Some(
            "Alpaca records have no place for tool calls and tool results. For a run with tools, export Chat \
             messages (TRL, Axolotl) or ShareGPT (LLaMA-Factory).",
        ),
        SftFormat::PromptCompletion => Some(
            "Prompt-completion records would put the tool results in the completion, where a trainer learns to \
             write them. For a run with tools, export Chat messages (TRL, Axolotl) or ShareGPT (LLaMA-Factory).",
        ),
        _ => None,
    }
}

/// Why text (non-conversational) preference records cannot hold a set with tool runs.
pub const PREF_TOOLS_REFUSAL: &str = "Text preference records cannot hold tool calls and tool results. This set has \
     runs with tools: export conversational records.";

/// The arguments as a JSON object when they parse as one (what chat templates take), else the
/// text the model wrote. Empty arguments are `{}`. In Parquet, that value as JSON text (always
/// valid JSON: its column is marked as JSON, see `schema::json_text`).
fn arguments_value(raw: &str, shape: Shape) -> Value {
    let native = if raw.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str::<Value>(raw) {
            Ok(v @ Value::Object(_)) => v,
            _ => Value::String(raw.to_string()),
        }
    };
    match shape {
        Shape::Native => native,
        Shape::Text => Value::String(crate::pyjson::to_string(&native)),
    }
}

fn tool_call_value(c: &ToolCall, shape: Shape) -> Value {
    json!({"id": c.id, "type": "function", "function": {"name": c.name, "arguments": arguments_value(&c.arguments, shape)}})
}

/// The messages after the prompt: each assistant turn (reasoning placed, `tool_calls` when it made
/// calls) and each tool result.
pub fn trace_messages(trace: &[TraceMessage], placement: ReasoningPlacement, shape: Shape) -> Vec<Value> {
    trace
        .iter()
        .map(|m| match m {
            TraceMessage::Assistant(a) => {
                let mut v = assistant_message(&a.reasoning, &Some(a.content.clone()), placement);
                if !a.tool_calls.is_empty() {
                    if let Value::Object(o) = &mut v {
                        let calls = a.tool_calls.iter().map(|c| tool_call_value(c, shape)).collect();
                        o.insert("tool_calls".into(), Value::Array(calls));
                    }
                }
                v
            }
            TraceMessage::Tool(t) => {
                json!({"role": "tool", "tool_call_id": t.tool_call_id, "name": t.name, "content": t.content})
            }
        })
        .collect()
}

/// A sample's completion as messages: its trace, or the one final answer.
pub fn completion_messages(sample: &SampleRec, placement: ReasoningPlacement, shape: Shape) -> Vec<Value> {
    match &sample.trace {
        Some(t) if !t.is_empty() => trace_messages(t, placement, shape),
        _ => vec![assistant_message(&sample.reasoning, &sample.response, placement)],
    }
}

/// The `tools` column: the definitions, as a list or as JSON text.
pub fn tools_value(definitions: &[Value], shape: Shape) -> Value {
    match shape {
        Shape::Native => Value::Array(definitions.to_vec()),
        Shape::Text => Value::String(crate::pyjson::to_string(&Value::Array(definitions.to_vec()))),
    }
}

/// Definitions of two runs together, each name once (the first run's wins).
pub fn merge_definitions(a: &[Value], b: &[Value]) -> Vec<Value> {
    let name = |d: &Value| d.pointer("/function/name").and_then(Value::as_str).unwrap_or("").to_string();
    let mut out: Vec<Value> = a.to_vec();
    for d in b {
        if !out.iter().any(|x| name(x) == name(d)) {
            out.push(d.clone());
        }
    }
    out
}

/// LLaMA-Factory's tool list: `[{"name", "description", "parameters"}]` as JSON text.
fn llama_factory_tools(definitions: &[Value]) -> String {
    let list: Vec<Value> = definitions.iter().map(|d| d.get("function").cloned().unwrap_or(Value::Null)).collect();
    crate::pyjson::to_string(&Value::Array(list))
}

/// ShareGPT turns for a trace, as LLaMA-Factory reads them: a `function_call` turn per assistant
/// turn with calls (its value the call as JSON, `{"name", "arguments"}`, or a list for parallel
/// calls, after the thinking in think tags), one `observation` turn with that turn's results
/// (several joined with `</tool_response><tool_response>`, as LLaMA-Factory's own converter does),
/// and `gpt` turns for answers. Text an assistant wrote next to its calls is left out: LLaMA-Factory
/// reads a `function_call` value as JSON.
fn sharegpt_trace(trace: &[TraceMessage], placement: ReasoningPlacement) -> Vec<Value> {
    let mut out = Vec::new();
    let mut results: Vec<String> = Vec::new();
    let flush = |results: &mut Vec<String>, out: &mut Vec<Value>| {
        if !results.is_empty() {
            out.push(json!({"from": "observation", "value": results.join("\n</tool_response>\n<tool_response>\n")}));
            results.clear();
        }
    };
    for m in trace {
        match m {
            TraceMessage::Tool(t) => results.push(t.content.clone()),
            TraceMessage::Assistant(a) => {
                flush(&mut results, &mut out);
                let thought = if placement == ReasoningPlacement::ThinkTags { think_wrap(nonblank(&a.reasoning), "") } else { String::new() };
                if a.tool_calls.is_empty() {
                    let (text, _) = assistant_text(&a.reasoning, &Some(a.content.clone()), placement);
                    out.push(json!({"from": "gpt", "value": text}));
                } else {
                    let calls: Vec<Value> = a
                        .tool_calls
                        .iter()
                        .map(|c| json!({"name": c.name, "arguments": arguments_value(&c.arguments, Shape::Native)}))
                        .collect();
                    let call = if calls.len() == 1 { calls[0].clone() } else { Value::Array(calls) };
                    out.push(json!({"from": "function_call", "value": format!("{thought}{}", crate::pyjson::to_string(&call))}));
                }
            }
        }
    }
    flush(&mut results, &mut out);
    out
}

/// The last assistant turn's reasoning of a sample (the side field of text formats).
fn final_reasoning(sample: &SampleRec) -> Option<String> {
    sample.reasoning.clone()
}

// ------------------------------------------------------------------------------ SFT

fn history_pairs(turns: &[&Msg]) -> Vec<Value> {
    let mut pairs = Vec::new();
    let mut pending: Option<&str> = None;
    for m in turns {
        if m.role == "user" {
            if let Some(p) = pending {
                pairs.push(json!([p, ""]));
            }
            pending = Some(&m.content);
        } else if m.role == "assistant" {
            pairs.push(json!([pending.unwrap_or(""), m.content]));
            pending = None;
        }
    }
    if let Some(p) = pending {
        pairs.push(json!([p, ""]));
    }
    pairs
}

pub fn sft_record(
    options: &ExportOptions,
    run: &RunInfo,
    sample: &SampleRec,
    prompt_messages: &Value,
    prompt_meta: Option<&Value>,
) -> Value {
    let prompt = render_prompt(prompt_messages, &run.params, options.include_system);
    let placement = options.reasoning;
    let shape = Shape::of(options.file_type);
    if let Some(tools) = &run.tools {
        match options.format {
            SftFormat::Messages => {
                let mut msgs = match msgs_value(&prompt) {
                    Value::Array(a) => a,
                    _ => Vec::new(),
                };
                msgs.extend(completion_messages(sample, placement, shape));
                return json!({"messages": msgs, "tools": tools_value(&tools.definitions, shape)});
            }
            SftFormat::Sharegpt => {
                let mut conv = sharegpt_prompt(&prompt);
                match &sample.trace {
                    Some(t) if !t.is_empty() => conv.extend(sharegpt_trace(t, placement)),
                    _ => conv.push(json!({"from": "gpt", "value": assistant_text(&sample.reasoning, &sample.response, placement).0})),
                }
                let mut rec = Map::new();
                rec.insert("conversations".into(), Value::Array(conv));
                rec.insert("tools".into(), Value::String(llama_factory_tools(&tools.definitions)));
                if outside_key(placement).is_some() {
                    rec.insert("reasoning".into(), final_reasoning(sample).filter(|r| !r.trim().is_empty()).map_or(Value::Null, Value::from));
                }
                return Value::Object(rec);
            }
            _ => {}
        }
    }

    match options.format {
        SftFormat::Raw => {
            let meta = match prompt_meta {
                Some(Value::Object(m)) if !m.is_empty() => Value::Object(m.clone()),
                _ => json!({}),
            };
            let params = match &run.params {
                Value::Object(m) if !m.is_empty() => run.params.clone(),
                _ => json!({}),
            };
            let mut rec = json!({
                "id": sample.id,
                "run_id": sample.run_id,
                "prompt_id": sample.prompt_id,
                "prompt_index": sample.prompt_idx,
                "sample_index": sample.sample_idx,
                "model": run.model,
                "endpoint": run.endpoint_name,
                "messages": msgs_value(&prompt),
                "reasoning": sample.reasoning.clone().unwrap_or_default(),
                "response": sample.response.clone().unwrap_or_default(),
                "finish_reason": sample.finish_reason,
                "usage": {
                    "prompt_tokens": sample.prompt_tokens,
                    "completion_tokens": sample.completion_tokens,
                    "reasoning_tokens": sample.reasoning_tokens,
                },
                "latency_ms": sample.latency_ms,
                "ttft_ms": sample.ttft_ms,
                "params": params,
                "meta": meta,
                "created_at": iso(sample.completed_at),
            });
            if let (Some(tools), Value::Object(o)) = (&run.tools, &mut rec) {
                o.insert("tool_calls".into(), sample.tool_calls.into());
                o.insert(
                    "trace".into(),
                    serde_json::to_value(sample.trace.as_deref().unwrap_or(&[])).unwrap_or(Value::Null),
                );
                o.insert("tools".into(), Value::Array(tools.definitions.clone()));
            }
            rec
        }
        SftFormat::Messages => {
            let mut msgs = msgs_value(&prompt);
            if let Value::Array(a) = &mut msgs {
                a.push(assistant_message(&sample.reasoning, &sample.response, placement));
            }
            json!({"messages": msgs})
        }
        SftFormat::PromptCompletion => {
            json!({
                "prompt": msgs_value(&prompt),
                "completion": [assistant_message(&sample.reasoning, &sample.response, placement)],
            })
        }
        SftFormat::Sharegpt => {
            let (text, side) = assistant_text(&sample.reasoning, &sample.response, placement);
            let mut conv = sharegpt_prompt(&prompt);
            conv.push(json!({"from": "gpt", "value": text}));
            let mut rec = Map::new();
            rec.insert("conversations".into(), Value::Array(conv));
            if outside_key(placement).is_some() {
                rec.insert("reasoning".into(), side.map_or(Value::Null, Value::from));
            }
            Value::Object(rec)
        }
        SftFormat::Alpaca => {
            let (text, side) = assistant_text(&sample.reasoning, &sample.response, placement);
            let system = prompt.iter().find(|m| m.role == "system").map(|m| m.content.clone());
            let turns: Vec<&Msg> = prompt.iter().filter(|m| m.role != "system").collect();
            let last_user = turns.last().is_some_and(|m| m.role == "user");
            let instruction = if last_user { turns[turns.len() - 1].content.clone() } else { String::new() };
            let earlier: &[&Msg] = if last_user { &turns[..turns.len() - 1] } else { &turns };
            let mut rec = Map::new();
            rec.insert("instruction".into(), instruction.into());
            rec.insert("input".into(), "".into());
            rec.insert("output".into(), text.into());
            if let Some(s) = system {
                rec.insert("system".into(), s.into());
            }
            let history = history_pairs(earlier);
            if !history.is_empty() {
                rec.insert("history".into(), Value::Array(history));
            }
            if outside_key(placement).is_some() {
                rec.insert("reasoning".into(), side.map_or(Value::Null, Value::from));
            }
            Value::Object(rec)
        }
    }
}

/// The prompt as ShareGPT turns.
fn sharegpt_prompt(prompt: &[Msg]) -> Vec<Value> {
    prompt
        .iter()
        .map(|m| {
            let from = match m.role.as_str() {
                "system" => "system",
                "user" => "human",
                "assistant" => "gpt",
                other => other,
            };
            json!({"from": from, "value": m.content})
        })
        .collect()
}

// ------------------------------------------------------------------------------ preferences

pub fn pref_records(
    options: &PrefExportOptions,
    chosen: &SampleRec,
    rejected: &SampleRec,
    prompt_messages: &Value,
    params: &Value,
) -> Vec<Value> {
    pref_records_with_tools(options, chosen, rejected, prompt_messages, params, None)
}

/// The tools of a preference pair: whether the set has runs with tools (then every record gets a
/// `tools` field), and the definitions of the chosen and the rejected sample's runs.
#[derive(Clone, Copy, Debug, Default)]
pub struct PairTools<'a> {
    pub chosen: &'a [Value],
    pub rejected: &'a [Value],
}

/// [`pref_records`] for a set with runs with tools: conversational records with trajectories and
/// `tools` (DPO: both runs' definitions, each name once, the chosen run's first; KTO: each
/// record's own run's).
pub fn pref_records_with_tools(
    options: &PrefExportOptions,
    chosen: &SampleRec,
    rejected: &SampleRec,
    prompt_messages: &Value,
    params: &Value,
    tools: Option<PairTools<'_>>,
) -> Vec<Value> {
    let prompt = render_prompt(prompt_messages, params, options.include_system);
    let placement = options.reasoning;
    let outside = outside_key(placement).is_some();
    let dpo = options.format == PrefFormat::Dpo;

    if let (Some(t), true) = (tools, options.conversational) {
        let shape = Shape::of(options.file_type);
        let c = Value::Array(completion_messages(chosen, placement, shape));
        let r = Value::Array(completion_messages(rejected, placement, shape));
        let p = msgs_value(&prompt);
        if dpo {
            let defs = merge_definitions(t.chosen, t.rejected);
            return vec![json!({"prompt": p, "chosen": c, "rejected": r, "tools": tools_value(&defs, shape)})];
        }
        return vec![
            json!({"prompt": p.clone(), "completion": c, "label": true, "tools": tools_value(t.chosen, shape)}),
            json!({"prompt": p, "completion": r, "label": false, "tools": tools_value(t.rejected, shape)}),
        ];
    }

    if options.conversational {
        let c = json!([assistant_message(&chosen.reasoning, &chosen.response, placement)]);
        let r = json!([assistant_message(&rejected.reasoning, &rejected.response, placement)]);
        let p = msgs_value(&prompt);
        if dpo {
            return vec![json!({"prompt": p, "chosen": c, "rejected": r})];
        }
        return vec![
            json!({"prompt": p.clone(), "completion": c, "label": true}),
            json!({"prompt": p, "completion": r, "label": false}),
        ];
    }

    let ptext = render_prompt_text(&prompt);
    let (ctext, cside) = assistant_text(&chosen.reasoning, &chosen.response, placement);
    let (rtext, rside) = assistant_text(&rejected.reasoning, &rejected.response, placement);
    let opt = |s: Option<String>| s.map_or(Value::Null, Value::from);
    if dpo {
        let mut rec = Map::new();
        rec.insert("prompt".into(), ptext.into());
        rec.insert("chosen".into(), ctext.into());
        rec.insert("rejected".into(), rtext.into());
        if outside {
            rec.insert("chosen_reasoning".into(), opt(cside));
            rec.insert("rejected_reasoning".into(), opt(rside));
        }
        return vec![Value::Object(rec)];
    }
    let mut good = Map::new();
    good.insert("prompt".into(), ptext.clone().into());
    good.insert("completion".into(), ctext.into());
    good.insert("label".into(), true.into());
    let mut bad = Map::new();
    bad.insert("prompt".into(), ptext.into());
    bad.insert("completion".into(), rtext.into());
    bad.insert("label".into(), false.into());
    if outside {
        good.insert("reasoning".into(), opt(cside));
        bad.insert("reasoning".into(), opt(rside));
    }
    vec![Value::Object(good), Value::Object(bad)]
}

/// Drop nulls recursively (parquet round trips add nulls for absent keys).
pub fn strip_none(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(m.iter().filter(|(_, x)| !x.is_null()).map(|(k, x)| (k.clone(), strip_none(x))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(strip_none).collect()),
        other => other.clone(),
    }
}

// ------------------------------------------------------------------------------ names

pub fn slugify(name: &str) -> String {
    // NFKD then drop non-ASCII: transliterate the common Latin accents by hand (no extra crate),
    // everything else non-ASCII is dropped like Python's `encode("ascii", "ignore")`.
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.chars().flat_map(fold_char) {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' {
            out.push(c);
            last_dash = false;
        } else if c.is_ascii() && !last_dash {
            out.push('-');
            last_dash = true;
        }
        // non-ASCII chars with no fold are dropped without breaking a run, like Python
    }
    let trimmed = out.trim_matches(|c| c == '-' || c == '.' || c == '_');
    let s: String = trimmed.chars().take(80).collect();
    if s.is_empty() {
        "export".into()
    } else {
        s
    }
}

/// ASCII base letters for common precomposed Latin characters (what NFKD + ASCII-ignore leaves).
fn fold_char(c: char) -> Vec<char> {
    if c.is_ascii() {
        return vec![c];
    }
    let base = match c {
        'À'..='Å' | 'à'..='å' | 'Ā'..='ą' => 'a',
        'Ç' | 'ç' | 'Ć'..='č' => 'c',
        'È'..='Ë' | 'è'..='ë' | 'Ē'..='ě' => 'e',
        'Ì'..='Ï' | 'ì'..='ï' | 'Ĩ'..='ı' => 'i',
        'Ñ' | 'ñ' | 'Ń'..='ň' => 'n',
        'Ò'..='Ö' | 'ò'..='ö' | 'Ō'..='ő' => 'o',
        'Ù'..='Ü' | 'ù'..='ü' | 'Ũ'..='ų' => 'u',
        'Ý' | 'ý' | 'ÿ' | 'Ÿ' => 'y',
        'Š' | 'š' | 'Ś'..='ş' => 's',
        'Ž' | 'ž' | 'Ź'..='ž' => 'z',
        _ => return vec![],
    };
    vec![base]
}

pub fn filename_for(name: &str, fmt: &str, file_type: &str) -> String {
    format!("{}-{}.{}", slugify(name), fmt, file_type)
}

#[cfg(test)]
mod tests;
