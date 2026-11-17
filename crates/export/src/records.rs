//! Record builders: one exported record per sample (SFT) or per preference pair (DPO/KTO).
//! A port of `backend/stellarator/services/exporters.py`; key order and text match it exactly.

use chrono::{NaiveDateTime, Timelike};
use serde_json::{json, Map, Value};
use stellarator_core::schemas::{ExportOptions, PrefExportOptions, PrefFormat, ReasoningPlacement, SftFormat};

/// The run fields a record needs.
#[derive(Debug, Clone)]
pub struct RunInfo {
    pub id: i64,
    pub name: String,
    pub endpoint_name: String,
    pub model: String,
    pub prompt_set_id: i64,
    pub params: Value,
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
            json!({
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
            })
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
            let mut conv: Vec<Value> = prompt
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
                .collect();
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

// ------------------------------------------------------------------------------ preferences

pub fn pref_records(
    options: &PrefExportOptions,
    chosen: &SampleRec,
    rejected: &SampleRec,
    prompt_messages: &Value,
    params: &Value,
) -> Vec<Value> {
    let prompt = render_prompt(prompt_messages, params, options.include_system);
    let placement = options.reasoning;
    let outside = outside_key(placement).is_some();
    let dpo = options.format == PrefFormat::Dpo;

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
