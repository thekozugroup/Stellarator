//! Chat messages: OpenAI `{role, content}` and ShareGPT `{from, value}` lists, normalized to the
//! turns that make up the prompt.

use serde::Serialize;
use serde_json::Value;
use stellarator_core::schemas::Turns;

/// One stored message. Field order matches the Python backend's `{"role", "content"}`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    pub role: &'static str,
    pub content: String,
}

impl Msg {
    pub fn new(role: &'static str, content: impl Into<String>) -> Self {
        Self { role, content: content.into() }
    }
    pub fn to_value(&self) -> Value {
        serde_json::json!({"role": self.role, "content": self.content})
    }
}

pub fn role_of(raw: &str) -> Option<&'static str> {
    Some(match raw.trim().to_lowercase().as_str() {
        "system" | "developer" => "system",
        "user" | "human" => "user",
        "assistant" | "gpt" | "model" | "bot" | "chatgpt" => "assistant",
        _ => return None,
    })
}

/// Python's `str(value)` for scalars: True/False for booleans, JSON for the rest.
pub fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        Value::Number(n) => n.to_string(),
        other => py_repr(other),
    }
}

/// `json.dumps(value, ensure_ascii=False)`: JSON with ", " and ": " separators.
pub fn py_repr(v: &Value) -> String {
    let mut out = String::new();
    write_py(v, &mut out);
    out
}

fn write_py(v: &Value, out: &mut String) {
    match v {
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_py(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (k, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push_str(": ");
                write_py(item, out);
            }
            out.push('}');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap_or_default()),
    }
}

/// The text of a message's content: a string, a list of text parts, or `{"text": ...}`.
pub fn content_text(content: Option<&Value>) -> String {
    match content {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => {
            let mut texts: Vec<&str> = Vec::new();
            for p in parts {
                match p {
                    Value::String(s) => texts.push(s),
                    Value::Object(o) => {
                        let ty = o.get("type");
                        let text_type = match ty {
                            None | Some(Value::Null) => true,
                            Some(Value::String(t)) => matches!(t.as_str(), "text" | "input_text" | "output_text"),
                            _ => false,
                        };
                        if let (Some(Value::String(t)), true) = (o.get("text"), text_type) {
                            texts.push(t);
                        }
                    }
                    _ => {}
                }
            }
            texts.join("\n")
        }
        Some(Value::Object(o)) => match o.get("text") {
            Some(Value::String(t)) => t.clone(),
            _ => String::new(),
        },
        Some(other) => py_str(other),
    }
}

fn present(o: &serde_json::Map<String, Value>, key: &str) -> bool {
    !matches!(o.get(key), None | Some(Value::Null))
}

/// True for a non-empty list whose first three items are OpenAI or ShareGPT messages.
pub fn is_messages_value(v: &Value) -> bool {
    let Value::Array(items) = v else { return false };
    if items.is_empty() {
        return false;
    }
    items.iter().take(3).all(|m| match m {
        Value::Object(o) => {
            (present(o, "role") && o.contains_key("content")) || (present(o, "from") && o.contains_key("value"))
        }
        _ => false,
    })
}

/// Every turn with a known role, in order.
pub fn parse_turns(v: &Value) -> Option<Vec<Msg>> {
    let Value::Array(items) = v else { return None };
    let mut out = Vec::with_capacity(items.len());
    for m in items {
        let Value::Object(o) = m else { continue };
        let (role, content) = if present(o, "role") {
            (o.get("role"), o.get("content"))
        } else if present(o, "from") {
            (o.get("from"), o.get("value"))
        } else {
            continue;
        };
        let raw = match role {
            Some(Value::String(s)) => s.clone(),
            Some(other) => py_str(other),
            None => continue,
        };
        let Some(role) = role_of(&raw) else { continue };
        out.push(Msg::new(role, content_text(content).trim()));
    }
    Some(out)
}

/// The prompt part of a conversation. `UpToLastUser` keeps every turn up to and including the last
/// user turn; `FirstUser` keeps the system messages before the first user turn and that turn.
/// Assistant turns after the chosen user turn are always dropped. None when there is no user turn
/// with text.
pub fn select_turns(mut msgs: Vec<Msg>, turns: Turns) -> Option<Vec<Msg>> {
    match turns {
        Turns::UpToLastUser => {
            let last = msgs.iter().rposition(|m| m.role == "user")?;
            if msgs[last].content.is_empty() {
                return None;
            }
            msgs.truncate(last + 1);
            Some(msgs)
        }
        Turns::FirstUser => {
            let first = msgs.iter().position(|m| m.role == "user")?;
            if msgs[first].content.is_empty() {
                return None;
            }
            msgs.truncate(first + 1);
            msgs.retain(|m| m.role != "assistant");
            Some(msgs)
        }
    }
}

pub fn normalize_messages(v: &Value, turns: Turns) -> Option<Vec<Msg>> {
    select_turns(parse_turns(v)?, turns)
}

/// True when `turns` leaves out at least one assistant turn of this conversation.
pub fn drops_assistant_turns(v: &Value, turns: Turns) -> bool {
    let Some(all) = parse_turns(v) else { return false };
    let total = all.iter().filter(|m| m.role == "assistant").count();
    if total == 0 {
        return false;
    }
    match select_turns(all, turns) {
        Some(kept) => kept.iter().filter(|m| m.role == "assistant").count() < total,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn openai_messages_and_parts() {
        let v = json!([
            {"role": "system", "content": "S"},
            {"role": "user", "content": [{"type": "text", "text": "a"}, {"type": "image_url", "image_url": {}}, {"type": "text", "text": "b"}]},
            {"role": "assistant", "content": "x"}
        ]);
        assert_eq!(normalize_messages(&v, Turns::UpToLastUser).unwrap(), vec![Msg::new("system", "S"), Msg::new("user", "a\nb")]);
        assert!(normalize_messages(&json!([{"role": "assistant", "content": "x"}]), Turns::UpToLastUser).is_none());
        assert!(drops_assistant_turns(&v, Turns::UpToLastUser));
    }

    #[test]
    fn sharegpt_and_turn_modes() {
        let v = json!([
            {"from": "system", "value": "Be brief."},
            {"from": "human", "value": "Q1"},
            {"from": "gpt", "value": "A1"},
            {"from": "human", "value": "Q2"},
            {"from": "gpt", "value": "<think>long</think>A2"}
        ]);
        assert!(is_messages_value(&v));
        let up = normalize_messages(&v, Turns::UpToLastUser).unwrap();
        assert_eq!(up.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(), ["Be brief.", "Q1", "A1", "Q2"]);
        let first = normalize_messages(&v, Turns::FirstUser).unwrap();
        assert_eq!(first, vec![Msg::new("system", "Be brief."), Msg::new("user", "Q1")]);
        assert!(drops_assistant_turns(&v, Turns::FirstUser));
        // A conversation that already ends in a user turn loses nothing.
        let prompt_only = json!([{"role": "user", "content": "hi"}]);
        assert!(!drops_assistant_turns(&prompt_only, Turns::UpToLastUser));
    }

    #[test]
    fn messages_shape_detection() {
        assert!(!is_messages_value(&json!([])));
        assert!(!is_messages_value(&json!(["a"])));
        assert!(!is_messages_value(&json!([{"role": null, "content": "x"}])));
        assert!(is_messages_value(&json!([{"role": "user", "content": null}])));
        assert_eq!(py_repr(&json!([1, {"a": [true, null]}])), r#"[1, {"a": [true, null]}]"#);
    }
}
