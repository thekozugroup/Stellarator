//! Column kinds, model-output detection and automatic field choice.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;
use stellarator_core::schemas::{ColumnInfo, ColumnKind, Turns};

use crate::messages::{is_messages_value, normalize_messages, py_repr};
use crate::table::Row;

/// Names that hold the prompt, in order of preference.
pub const PROMPT_NAMES: &[&str] =
    &["prompt", "problem", "question", "instruction", "input", "query", "user", "text", "content"];
/// Names of chat-message columns, in order of preference.
pub const MESSAGES_NAMES: &[&str] = &["messages", "conversations", "conversation", "chat"];
pub const SYSTEM_NAMES: &[&str] = &["system", "system_prompt"];

/// Words that mark a column as model output (matched against the words of a column name, so
/// `deepseek_response`, `r1_generation` and `chosenResponse` count).
const OUTPUT_WORDS: &[&str] = &[
    "response", "responses", "completion", "completions", "output", "outputs", "answer", "answers",
    "solution", "solutions", "generation", "generations", "chosen", "rejected", "reasoning", "thinking",
    "think", "cot", "rationale", "rationales", "assistant", "target", "targets", "label", "labels",
];
/// Words of model generations: left out of meta. Reference answers (answer, solution, label,
/// target, ground_truth, gold) are output-looking (never the prompt) but stay in meta, where they
/// are useful for verification and RL.
const GENERATION_WORDS: &[&str] = &[
    "response", "responses", "completion", "completions", "generation", "generations", "generated", "output",
    "outputs", "chosen", "rejected", "reasoning", "thinking", "think", "cot", "rationale", "rationales", "assistant",
];
/// Words that mark a column as data about an output rather than the output itself
/// (`is_reasoning_complete`, `score_chosen`, `num_reasoning_tokens`, `response_id`).
const ABOUT_OUTPUT_WORDS: &[&str] = &[
    "is", "has", "num", "n", "count", "counts", "score", "scores", "len", "length", "lengths", "token", "tokens",
    "complete", "completed", "correct", "correctness", "verified", "verify", "id", "ids", "idx", "type", "types",
    "model", "models", "source", "lang", "language", "finish", "rank", "rating", "ratings", "logprob", "logprobs",
    "quality", "status", "valid",
];
/// Words that mark a prompt-like column name (`user_prompt`, `question_text`).
const PROMPT_WORDS: &[&str] =
    &["prompt", "prompts", "problem", "problems", "question", "questions", "instruction", "instructions", "query", "queries"];
/// Words that mark an identifier or category rather than prompt text (`prompt_id`, `question_type`).
const NOT_PROSE_WORDS: &[&str] = &[
    "id", "ids", "idx", "index", "uuid", "hash", "key", "type", "category", "categories", "source", "lang",
    "language", "len", "length", "count", "num", "score", "level", "difficulty", "url", "token", "tokens",
];

static NUMBER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$").expect("regex"));

/// Extra knowledge about a column from a typed schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    /// Dates and times: kept in meta (as ISO text) but never a prompt.
    Temporal,
}

/// The lowercase words of a column name: split on punctuation, camelCase and letter/digit changes.
pub fn name_words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut prev: Option<char> = None;
    for c in name.chars() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            prev = None;
            continue;
        }
        if let Some(p) = prev {
            let boundary = (p.is_lowercase() && c.is_uppercase())
                || (p.is_alphabetic() && c.is_numeric())
                || (p.is_numeric() && c.is_alphabetic());
            if boundary && !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
        }
        cur.extend(c.to_lowercase());
        prev = Some(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// True for names like response, completions, deepseek_response, r1_generation, chosen, reasoning;
/// false for data about an output such as score_chosen or is_reasoning_complete.
pub fn looks_like_output(name: &str) -> bool {
    if name.eq_ignore_ascii_case("model_output") {
        return true;
    }
    let words = name_words(name);
    words.iter().any(|w| OUTPUT_WORDS.contains(&w.as_str())) && !words.iter().any(|w| ABOUT_OUTPUT_WORDS.contains(&w.as_str()))
}

/// True for columns that hold model generations (response, completion, reasoning, chosen, ...):
/// they are left out of meta.
pub fn is_generation(name: &str) -> bool {
    if name.eq_ignore_ascii_case("model_output") {
        return true;
    }
    let words = name_words(name);
    words.iter().any(|w| GENERATION_WORDS.contains(&w.as_str())) && !words.iter().any(|w| ABOUT_OUTPUT_WORDS.contains(&w.as_str()))
}

pub fn shorten(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= n {
        s
    } else {
        let mut out: String = s.chars().take(n - 1).collect();
        out.push('…');
        out
    }
}

fn example(v: &Value) -> String {
    if let Value::String(s) = v {
        return shorten(s, 160);
    }
    if is_messages_value(v) {
        if let Some(msgs) = normalize_messages(v, Turns::UpToLastUser) {
            if let Some(last) = msgs.last() {
                return shorten(&last.content, 160);
            }
        }
    }
    shorten(&py_repr(v), 160)
}

/// Columns as the preview shows them, plus what the importer needs to know about each.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    pub columns: Vec<ColumnInfo>,
    /// Values can go into meta (text, numbers, booleans, dates).
    pub scalar: Vec<bool>,
    /// Most sampled values contain whitespace (sentences rather than ids or categories).
    pub prose: Vec<bool>,
}

impl Schema {
    pub fn index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }
    pub fn kind(&self, name: &str) -> Option<&ColumnKind> {
        self.index(name).map(|i| &self.columns[i].kind)
    }
    pub fn names(&self) -> Vec<String> {
        self.columns.iter().map(|c| c.name.clone()).collect()
    }
}

fn is_present(v: &Value) -> bool {
    !matches!(v, Value::Null) && !matches!(v, Value::String(s) if s.is_empty())
}

pub fn infer_schema(names: &[String], sample: &[Row], numeric_strings: bool, hints: &[Option<Hint>]) -> Schema {
    let mut schema = Schema::default();
    for (i, name) in names.iter().enumerate() {
        let vals: Vec<&Value> = sample.iter().filter_map(|r| r.get(i)).filter(|v| is_present(v)).collect();
        let all = |f: fn(&Value) -> bool| vals.iter().all(|v| f(v));
        let mut kind = if vals.is_empty() {
            ColumnKind::Other
        } else if all(|v| v.is_string()) {
            let numeric = numeric_strings
                && vals.iter().all(|v| v.as_str().is_some_and(|s| NUMBER_RE.is_match(s.trim())));
            if numeric {
                ColumnKind::Number
            } else {
                ColumnKind::Text
            }
        } else if all(|v| v.is_number()) {
            ColumnKind::Number
        } else if all(|v| v.is_string() || v.is_number()) {
            ColumnKind::Text
        } else if all(is_messages_value) {
            ColumnKind::Messages
        } else {
            ColumnKind::Other
        };
        let mut scalar =
            matches!(kind, ColumnKind::Text | ColumnKind::Number) || (!vals.is_empty() && all(|v| v.is_boolean()));
        if hints.get(i).copied().flatten() == Some(Hint::Temporal) {
            kind = ColumnKind::Other;
            scalar = true;
        }
        let strings: Vec<&str> = vals.iter().filter_map(|v| v.as_str()).collect();
        let spaced = strings.iter().filter(|s| s.trim().contains(char::is_whitespace)).count();
        let prose = !strings.is_empty() && spaced * 2 >= strings.len();
        schema.columns.push(ColumnInfo {
            name: name.clone(),
            kind,
            example: vals.first().map(|v| example(v)),
            looks_like_output: looks_like_output(name),
        });
        schema.scalar.push(scalar);
        schema.prose.push(prose);
    }
    schema
}

/// The prompt column to use when none was chosen. Never a model-output column.
pub fn auto_prompt_field(schema: &Schema) -> Option<String> {
    let cols = &schema.columns;
    let usable = |i: usize| {
        !cols[i].looks_like_output && matches!(cols[i].kind, ColumnKind::Text | ColumnKind::Messages)
    };
    let by_name = |n: &str| cols.iter().position(|c| c.name.to_lowercase() == n);
    // 1. A prompt-like name: prompt, problem, question, ...
    for n in PROMPT_NAMES {
        if let Some(i) = by_name(n).filter(|&i| usable(i)) {
            return Some(cols[i].name.clone());
        }
    }
    // 2. A message column by name.
    for n in MESSAGES_NAMES {
        if let Some(i) = by_name(n).filter(|&i| usable(i) && cols[i].kind == ColumnKind::Messages) {
            return Some(cols[i].name.clone());
        }
    }
    // 3. A name built from a prompt word (user_prompt, question_text), not an id or category.
    for (i, c) in cols.iter().enumerate() {
        let words = name_words(&c.name);
        let prompty = words.iter().any(|w| PROMPT_WORDS.contains(&w.as_str()));
        let id_like = words.iter().any(|w| NOT_PROSE_WORDS.contains(&w.as_str()));
        if prompty && !id_like && usable(i) && (c.kind == ColumnKind::Messages || schema.prose[i]) {
            return Some(c.name.clone());
        }
    }
    // 4. Any message column.
    if let Some(c) = cols.iter().find(|c| c.kind == ColumnKind::Messages && !c.looks_like_output) {
        return Some(c.name.clone());
    }
    // 5. The first text column of sentences, else the first text column that isn't named like an
    //    id or category. Not the longest: in a distilled dataset the longest column is the response.
    let texts: Vec<usize> = (0..cols.len()).filter(|&i| usable(i) && cols[i].kind == ColumnKind::Text).collect();
    let id_like = |i: usize| name_words(&cols[i].name).iter().any(|w| NOT_PROSE_WORDS.contains(&w.as_str()));
    texts
        .iter()
        .find(|&&i| schema.prose[i])
        .or_else(|| texts.iter().find(|&&i| !id_like(i)))
        .map(|&i| cols[i].name.clone())
}

pub fn auto_system_field(schema: &Schema, prompt_field: Option<&str>) -> Option<String> {
    schema
        .columns
        .iter()
        .find(|c| {
            SYSTEM_NAMES.contains(&c.name.to_lowercase().as_str())
                && c.kind == ColumnKind::Text
                && Some(c.name.as_str()) != prompt_field
        })
        .map(|c| c.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema_of(names: &[&str], rows: Vec<Vec<Value>>) -> Schema {
        let names: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        infer_schema(&names, &rows, false, &[])
    }

    #[test]
    fn output_names() {
        for n in [
            "response", "Responses", "completion", "outputs", "answer", "solution", "generations", "chosen",
            "rejected", "reasoning", "thinking", "think", "cot", "rationale", "model_output", "assistant",
            "target", "labels", "deepseek_response", "r1_generation", "chosenResponse", "final_answer",
            "reasoning_content", "gpt4_completion", "Generation1",
        ] {
            assert!(looks_like_output(n), "{n}");
        }
        for n in [
            "prompt", "problem", "question", "instruction", "input", "messages", "text", "id", "source", "thought_id_x",
            "is_reasoning_complete", "score_chosen", "num_reasoning_tokens", "response_id", "chosen_model",
        ] {
            assert!(!looks_like_output(n), "{n}");
        }
        assert_eq!(name_words("deepseekR1Response"), ["deepseek", "r", "1", "response"]);
        for n in ["response", "deepseek_response", "r1_generation", "chosen", "reasoning_content", "model_output", "outputs", "think"] {
            assert!(is_generation(n), "{n}");
        }
        for n in ["answer", "solution", "labels", "target", "ground_truth", "gold", "reference", "score_chosen", "prompt"] {
            assert!(!is_generation(n), "{n}");
        }
    }

    #[test]
    fn prefers_prompt_names_over_messages_and_never_outputs() {
        let msgs = json!([{"role": "user", "content": "p"}, {"role": "assistant", "content": "long answer here"}]);
        let s = schema_of(
            &["messages", "problem", "solution"],
            vec![vec![msgs, json!("Find x."), json!("x = 2, because of a long derivation")]],
        );
        assert_eq!(auto_prompt_field(&s).as_deref(), Some("problem"));
        // Only outputs and ids: nothing is picked.
        let s = schema_of(&["uuid", "response"], vec![vec![json!("ab12"), json!("a long response text")]]);
        assert_eq!(auto_prompt_field(&s), None);
        let s = schema_of(&["word", "response"], vec![vec![json!("serendipity"), json!("a long response text")]]);
        assert_eq!(auto_prompt_field(&s).as_deref(), Some("word"));
        let s = schema_of(&["response", "reasoning"], vec![vec![json!("a b"), json!("c d")]]);
        assert_eq!(auto_prompt_field(&s), None);
    }

    #[test]
    fn first_prose_column_not_longest() {
        let s = schema_of(
            &["id", "title", "body", "r1_generation"],
            vec![vec![json!(1), json!("short"), json!("a body of text"), json!("x ".repeat(500))]],
        );
        assert_eq!(auto_prompt_field(&s).as_deref(), Some("body"));
        let s = schema_of(
            &["prompt_id", "user_prompt"],
            vec![vec![json!("9f86d081884c7d65"), json!("Tell me a story")]],
        );
        assert_eq!(auto_prompt_field(&s).as_deref(), Some("user_prompt"));
    }

    #[test]
    fn kinds() {
        let names: Vec<String> = ["id", "prompt", "flag", "nested"].iter().map(|s| s.to_string()).collect();
        let rows = vec![
            vec![json!("1"), json!("hi"), json!(true), json!({"a": 1})],
            vec![json!("2"), json!(""), json!(false), json!([1])],
        ];
        let s = infer_schema(&names, &rows, true, &[]);
        let kinds: Vec<_> = s.columns.iter().map(|c| c.kind.clone()).collect();
        assert_eq!(kinds, [ColumnKind::Number, ColumnKind::Text, ColumnKind::Other, ColumnKind::Other]);
        assert_eq!(s.scalar, [true, true, true, false]);
        assert_eq!(s.columns[3].example.as_deref(), Some(r#"{"a": 1}"#));
    }
}
