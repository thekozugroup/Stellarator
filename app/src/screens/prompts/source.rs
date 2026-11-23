//! Where a prompt set came from (frontend/src/components/prompts/source.ts): the words for a list
//! row or a chip, from the set's `source_kind` and `source`.

use serde_json::Value;

/// A string field of the source, trimmed; none when absent or blank.
fn field(source: &Value, name: &str) -> Option<String> {
    source.get(name).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// "config/split", as far as the source names them.
fn where_of(source: &Value) -> String {
    [field(source, "config"), field(source, "split")].into_iter().flatten().collect::<Vec<_>>().join("/")
}

/// "Hugging Face · openai/gsm8k · main/train", "File · prompts.md", "Pasted".
#[cfg_attr(not(test), allow(dead_code, reason = "the web UI sets it as a tooltip; the app has no tooltip on a row"))]
pub fn describe_source(kind: &str, source: &Value) -> String {
    let parts: Vec<String> = match kind {
        "huggingface" => vec!["Hugging Face".to_owned(), field(source, "dataset").unwrap_or_default(), where_of(source)],
        "file" => vec!["File".to_owned(), field(source, "filename").unwrap_or_default()],
        _ => vec!["Pasted".to_owned()],
    };
    parts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
}

/// [`describe_source`] split into the words and the data, so a screen can set the data in mono.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceParts {
    /// "Hugging Face", "File" or "Pasted".
    pub kind: &'static str,
    /// The dataset ID or file name: data, set in mono. None for pasted text.
    pub id: Option<String>,
    /// "main/train" for a Hugging Face set.
    pub whr: Option<String>,
}

pub fn source_parts(kind: &str, source: &Value) -> SourceParts {
    match kind {
        "huggingface" => SourceParts {
            kind: "Hugging Face",
            id: field(source, "dataset"),
            whr: Some(where_of(source)).filter(|w| !w.is_empty()),
        },
        "file" => SourceParts { kind: "File", id: field(source, "filename"), whr: None },
        _ => SourceParts { kind: "Pasted", id: None, whr: None },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_hugging_face_set_names_its_dataset_config_and_split() {
        let s = json!({"dataset": "openai/gsm8k", "config": "main", "split": "train"});
        assert_eq!(describe_source("huggingface", &s), "Hugging Face · openai/gsm8k · main/train");
        let parts = source_parts("huggingface", &s);
        assert_eq!((parts.kind, parts.id.as_deref(), parts.whr.as_deref()), ("Hugging Face", Some("openai/gsm8k"), Some("main/train")));
    }

    #[test]
    fn missing_or_blank_parts_are_left_out() {
        let s = json!({"dataset": " a/b ", "config": null, "split": "test"});
        assert_eq!(describe_source("huggingface", &s), "Hugging Face · a/b · test");
        assert_eq!(describe_source("huggingface", &json!({})), "Hugging Face");
        assert_eq!(source_parts("huggingface", &json!({"dataset": "a/b"})).whr, None);
    }

    #[test]
    fn files_and_pasted_text() {
        assert_eq!(describe_source("file", &json!({"filename": "prompts.md"})), "File · prompts.md");
        assert_eq!(describe_source("file", &Value::Null), "File");
        assert_eq!(describe_source("paste", &Value::Null), "Pasted");
        assert_eq!(source_parts("paste", &Value::Null).id, None);
        assert_eq!(source_parts("file", &json!({"filename": "x.csv"})).id.as_deref(), Some("x.csv"));
    }
}
