//! The tool-calling mock models.
//!
//! | Model | Behavior |
//! | --- | --- |
//! | `mock-tools` | Calls a tool from the request's `tools`, then answers using the results |
//! | `mock-tools-loop` | Calls a tool on every request (never answers), for `max_rounds` tests |
//! | `mock-no-tools` | Answers 400 to any request with `tools`, as vLLM without `--enable-auto-tool-choice` |
//!
//! `mock-tools`, on a request with tools and no tool results after the last user message, calls:
//! - every tool named in the prompt as `tool:<name>` (a sent name equal to `<name>` or ending in
//!   `__<name>`), all in one answer;
//! - otherwise the first tool, and also the second (two calls in parallel) when the prompt contains
//!   "both" or when a hash of the prompt is divisible by 3.
//!
//! Arguments are made from each tool's schema (its required properties: strings from the prompt's
//! first words, numbers from the prompt's numbers, `seconds` 0.2) and streamed in pieces of 5 characters. `badargs` in the
//! prompt makes the first call's arguments invalid JSON. Once the results are in, it answers with
//! a sentence quoting them. Each answer has a thinking trace in `delta.reasoning`.
//!
//! Requests are checked as a strict server would: every `tool` message must answer a call of the
//! assistant message before it, every call must be answered before the next turn, and assistant
//! messages must not carry earlier reasoning (`reasoning`, `reasoning_content`, `thinking`).
//! Breaking a rule answers 400.

use serde_json::{json, Value};

pub const TOOL_MODELS: [&str; 3] = ["mock-tools", "mock-tools-loop", "mock-no-tools"];

/// The vLLM error for `tools` without a tool parser.
pub const NO_TOOLS_ERROR: &str =
    "\"auto\" tool choice requires --enable-auto-tool-choice and --tool-call-parser to be set";

/// A call the model makes.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// What the model does with a request.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    Calls { reasoning: String, calls: Vec<Call> },
    Answer { reasoning: String, content: String },
    Reject(String),
}

fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect(),
        _ => String::new(),
    }
}

/// FNV-1a, so the choice of parallel calls is the same on every run.
fn hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// The rules of a strict server, or the reason the conversation breaks them.
pub fn check_messages(messages: &[Value]) -> Result<(), String> {
    let mut open: Vec<String> = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        let role = m.get("role").and_then(Value::as_str).unwrap_or("");
        if role == "tool" {
            let id = m.get("tool_call_id").and_then(Value::as_str).unwrap_or("");
            match open.iter().position(|o| o == id) {
                Some(p) => {
                    open.remove(p);
                }
                None => return Err(format!("messages[{i}]: the tool message's tool_call_id \"{id}\" answers no open tool call.")),
            }
            continue;
        }
        if let Some(id) = open.first() {
            return Err(format!("messages[{i}]: the tool call \"{id}\" has no tool message before this {role} message."));
        }
        if role == "assistant" {
            for key in ["reasoning", "reasoning_content", "thinking"] {
                if m.get(key).is_some_and(|v| !v.is_null()) {
                    return Err(format!("messages[{i}]: the assistant message carries earlier reasoning ({key})."));
                }
            }
            if let Some(Value::Array(calls)) = m.get("tool_calls") {
                for c in calls {
                    let id = c.get("id").and_then(Value::as_str).unwrap_or("");
                    if id.is_empty() || c.pointer("/function/name").and_then(Value::as_str).is_none() {
                        return Err(format!("messages[{i}]: a tool call has no id or no function name."));
                    }
                    open.push(id.to_string());
                }
            }
        }
    }
    match open.first() {
        Some(id) => Err(format!("The tool call \"{id}\" has no tool message.")),
        None => Ok(()),
    }
}

/// Numbers in the text, in order.
fn numbers(text: &str) -> Vec<f64> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter_map(|w| w.trim_matches('.').parse::<f64>().ok())
        .collect()
}

/// Arguments for a tool, from its parameters schema and the prompt.
fn arguments(parameters: &Value, prompt: &str) -> Value {
    let props = parameters.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
    let required: Vec<String> = parameters
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let words: String = prompt
        .split_whitespace()
        .filter(|w| !w.starts_with("tool:") && *w != "badargs" && *w != "both")
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");
    let mut nums = numbers(prompt).into_iter().chain([2.0, 3.0]);
    let mut out = serde_json::Map::new();
    for name in required {
        let kind = props.get(&name).and_then(|p| p.get("type")).and_then(Value::as_str).unwrap_or("string");
        let value = match (name.as_str(), kind) {
            ("times", _) => json!(nums.next().unwrap_or(3.0) as i64),
            ("seconds", _) => json!(0.2),
            (_, "number") => json!(nums.next().unwrap_or(2.0)),
            (_, "integer") => json!(nums.next().unwrap_or(2.0) as i64),
            (_, "boolean") => json!(true),
            (_, "array") => json!([]),
            (_, "object") => json!({}),
            _ => match props.get(&name).and_then(|p| p.get("enum")).and_then(Value::as_array).and_then(|e| e.first()) {
                Some(first) => first.clone(),
                None => json!(words),
            },
        };
        out.insert(name, value);
    }
    Value::Object(out)
}

fn call_id(rng: &mut fastrand::Rng) -> String {
    format!("call_{:08x}", rng.u32(..))
}

/// What `model` does with this request body.
pub fn plan(model: &str, body: &Value, rng: &mut fastrand::Rng) -> Plan {
    let messages: Vec<Value> = body.get("messages").and_then(Value::as_array).cloned().unwrap_or_default();
    if let Err(e) = check_messages(&messages) {
        return Plan::Reject(e);
    }
    let tools: Vec<(String, Value)> = body
        .get("tools")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|t| {
                    let f = t.get("function")?;
                    Some((f.get("name")?.as_str()?.to_string(), f.get("parameters").cloned().unwrap_or(json!({}))))
                })
                .collect()
        })
        .unwrap_or_default();
    if model == "mock-no-tools" && !tools.is_empty() {
        return Plan::Reject(NO_TOOLS_ERROR.into());
    }
    let last_user = messages.iter().rposition(|m| m.get("role").and_then(Value::as_str) == Some("user"));
    let prompt = last_user.map(|i| text_of(messages[i].get("content"))).unwrap_or_default();
    let results: Vec<String> = match last_user {
        Some(i) => messages[i + 1..]
            .iter()
            .filter(|m| m.get("role").and_then(Value::as_str) == Some("tool"))
            .map(|m| text_of(m.get("content")))
            .collect(),
        None => Vec::new(),
    };
    let head: String = prompt.split_whitespace().take(12).collect::<Vec<_>>().join(" ");
    let wants_calls = !tools.is_empty() && (model == "mock-tools-loop" || results.is_empty());
    if !wants_calls {
        if results.is_empty() {
            return Plan::Answer {
                reasoning: format!("The question asks: {head}. No tools are needed for this."),
                content: format!("Here is the answer to \u{201c}{head}\u{201d}. Final answer: {}.", rng.i64(2..=999)),
            };
        }
        let quoted: Vec<String> = results
            .iter()
            .map(|r| {
                let r: String = r.split_whitespace().collect::<Vec<_>>().join(" ");
                let cut: String = r.chars().take(120).collect();
                format!("\u{201c}{cut}\u{201d}")
            })
            .collect();
        return Plan::Answer {
            reasoning: format!("The tools returned {} result(s). Let me combine them into the answer.", results.len()),
            content: format!("Using the tool results: {}. Final answer: {}.", quoted.join("; "), rng.i64(2..=999)),
        };
    }
    let named: Vec<&(String, Value)> = prompt
        .split_whitespace()
        .filter_map(|w| w.strip_prefix("tool:"))
        .map(|w| w.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')))
        .filter_map(|n| tools.iter().find(|(t, _)| t == n || t.ends_with(&format!("__{n}"))))
        .collect();
    let picked: Vec<&(String, Value)> = if !named.is_empty() {
        named
    } else if tools.len() >= 2 && (prompt.contains("both") || hash(&prompt) % 3 == 0) {
        vec![&tools[0], &tools[1]]
    } else {
        vec![&tools[0]]
    };
    let mut calls: Vec<Call> = picked
        .iter()
        .map(|(name, params)| Call { id: call_id(rng), name: name.clone(), arguments: arguments(params, &prompt).to_string() })
        .collect();
    if prompt.contains("badargs") {
        calls[0].arguments = "{\"query\": \"unterminated".into();
    }
    let names: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
    Plan::Calls { reasoning: format!("The question asks: {head}. I should call {}.", names.join(" and ")), calls }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Value {
        json!([
            {"type": "function", "function": {"name": "lookup", "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}}},
            {"type": "function", "function": {"name": "add", "parameters": {"type": "object", "properties": {"a": {"type": "number"}, "b": {"type": "number"}}, "required": ["a", "b"]}}},
            {"type": "function", "function": {"name": "srv__fail", "parameters": {"type": "object", "properties": {}}}}
        ])
    }

    #[test]
    fn calls_then_answers() {
        let mut rng = fastrand::Rng::with_seed(1);
        let body = json!({"tools": tools(), "messages": [{"role": "user", "content": "What is 4 + 5? both"}]});
        let Plan::Calls { calls, .. } = plan("mock-tools", &body, &mut rng) else { panic!() };
        assert_eq!(calls.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["lookup", "add"]);
        assert_eq!(calls[1].arguments, r#"{"a":4.0,"b":5.0}"#);
        let body = json!({"tools": tools(), "messages": [
            {"role": "user", "content": "x tool:fail"},
            {"role": "assistant", "content": "", "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "srv__fail", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "c1", "content": "The lookup failed."}
        ]});
        let Plan::Answer { content, .. } = plan("mock-tools", &body, &mut rng) else { panic!() };
        assert!(content.starts_with("Using the tool results: \u{201c}The lookup failed.\u{201d}"), "{content}");
        let Plan::Calls { .. } = plan("mock-tools-loop", &body, &mut rng) else { panic!() };
        assert_eq!(plan("mock-no-tools", &body, &mut rng), Plan::Reject(NO_TOOLS_ERROR.into()));
    }

    #[test]
    fn strict_conversation_rules() {
        let call = json!({"role": "assistant", "content": "", "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "a", "arguments": "{}"}}]});
        assert!(check_messages(&[json!({"role": "user", "content": "q"}), call.clone()]).unwrap_err().contains("no tool message"));
        assert!(check_messages(&[json!({"role": "tool", "tool_call_id": "x", "content": ""})]).unwrap_err().contains("answers no open"));
        let reasoning = json!({"role": "assistant", "content": "a", "reasoning_content": "r"});
        assert!(check_messages(&[reasoning]).unwrap_err().contains("earlier reasoning"));
        assert!(check_messages(&[call, json!({"role": "tool", "tool_call_id": "c1", "content": "ok"})]).is_ok());
    }
}
