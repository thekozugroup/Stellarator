//! The import screen's pure helpers (frontend/src/components/import/helpers.ts, with its tests in
//! frontend/tests/import-helpers.test.ts): dataset names, the default split, the formats and split
//! modes with their words, counts typed by the user, Simple and Advanced, the explanations behind
//! the info buttons, and Simple mode's summary of what was detected.

use nsh_core::storage::Storage;

use crate::api::types::{HfSplit, ImportFormat, ImportPreviewOut, SplitMode, Turns};
use crate::format::int;

/// Accepts "owner/name", "datasets/owner/name" or a full huggingface.co URL; returns "owner/name"
/// (whatever follows the second path segment is dropped).
pub fn parse_dataset(input: &str) -> String {
    let mut s = input.trim().to_owned();
    if s.is_empty() {
        return String::new();
    }
    s = strip_prefix_ci(&s, "https://").or_else(|| strip_prefix_ci(&s, "http://")).unwrap_or(s);
    s = strip_prefix_ci(&s, "www.huggingface.co/").or_else(|| strip_prefix_ci(&s, "huggingface.co/")).unwrap_or(s);
    if let Some(i) = s.find(['?', '#']) {
        s.truncate(i);
    }
    s = s.strip_prefix("datasets/").map(str::to_owned).unwrap_or(s);
    s.split('/').filter(|p| !p.is_empty()).take(2).collect::<Vec<_>>().join("/")
}

/// `s` without the ASCII case-insensitive `prefix`, when it starts with it.
fn strip_prefix_ci(s: &str, prefix: &str) -> Option<String> {
    (s.len() >= prefix.len() && s.is_char_boundary(prefix.len()) && s[..prefix.len()].eq_ignore_ascii_case(prefix))
        .then(|| s[prefix.len()..].to_owned())
}

/// True for a complete "owner/name", the shape Simple mode loads without a click.
pub fn is_dataset_id(s: &str) -> bool {
    let part = |p: &str| {
        let mut chars = p.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphanumeric()) && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    };
    match s.split_once('/') {
        Some((owner, name)) => part(owner) && part(name),
        None => false,
    }
}

/// The split Stellarator reads by default: "train", else the first starting with "train", else the
/// first.
pub fn default_split(names: &[String]) -> Option<String> {
    names
        .iter()
        .find(|n| *n == "train")
        .or_else(|| names.iter().find(|n| n.starts_with("train")))
        .or_else(|| names.first())
        .cloned()
}

/// The split to show after the user picks `config`: `current` when the config has it, else the
/// config's default split.
pub fn split_for_config(splits: &[HfSplit], config: &str, current: Option<&str>) -> Option<String> {
    let names: Vec<String> = splits.iter().filter(|s| s.config == config).map(|s| s.split.clone()).collect();
    names.iter().find(|s| Some(s.as_str()) == current).cloned().or_else(|| default_split(&names))
}

/// The name a Hugging Face set gets unless the user types one: the dataset and its split
/// ("openai/gsm8k train"), or only the dataset when every split is imported or none is known yet.
pub fn hf_default_name(dataset: &str, split: Option<&str>, all_splits: bool) -> String {
    match split {
        _ if dataset.is_empty() => String::new(),
        Some(split) if !all_splits => format!("{dataset} {split}"),
        _ => dataset.to_owned(),
    }
}

/// What the file picker offers, by extension.
pub const EXTENSIONS: [&str; 9] = ["txt", "md", "markdown", "csv", "tsv", "json", "jsonl", "ndjson", "parquet"];

/// The formats of a file, with their words ("Auto" first).
pub const FORMAT_OPTIONS: [(ImportFormat, &str); 8] = [
    (ImportFormat::Auto, "Auto"),
    (ImportFormat::Text, "Text"),
    (ImportFormat::Markdown, "Markdown"),
    (ImportFormat::Csv, "CSV"),
    (ImportFormat::Tsv, "TSV"),
    (ImportFormat::Json, "JSON"),
    (ImportFormat::Jsonl, "JSONL"),
    (ImportFormat::Parquet, "Parquet"),
];

/// The words for a format the server detected ("csv", "huggingface", ...).
pub fn format_label(f: &str) -> String {
    if let Some((_, label)) = FORMAT_OPTIONS.iter().find(|(v, _)| format_name(*v) == f) {
        return (*label).to_owned();
    }
    if f == "huggingface" {
        return "Hugging Face".to_owned();
    }
    f.to_owned()
}

/// The lowercase name the API uses for a format.
pub fn format_name(f: ImportFormat) -> &'static str {
    match f {
        ImportFormat::Auto => "auto",
        ImportFormat::Text => "text",
        ImportFormat::Markdown => "markdown",
        ImportFormat::Csv => "csv",
        ImportFormat::Tsv => "tsv",
        ImportFormat::Json => "json",
        ImportFormat::Jsonl => "jsonl",
        ImportFormat::Parquet => "parquet",
    }
}

/// How text is split into prompts: the mode, its label in the select, and the sentence it gives
/// ("split on --- lines"; empty for Automatic).
pub const SPLIT_OPTIONS: [(SplitMode, &str, &str); 7] = [
    (SplitMode::Auto, "Automatic", ""),
    (SplitMode::Line, "One per line", "one prompt per non-empty line"),
    (SplitMode::BlankLine, "Blank line between prompts", "split on blank lines"),
    (SplitMode::Separator, "Separator lines (---)", "split on --- lines"),
    (SplitMode::Heading, "Markdown headings", "split on Markdown headings"),
    (SplitMode::ListItem, "List items", "one prompt per list item"),
    (SplitMode::Whole, "Whole text is one prompt", "the whole text is one prompt"),
];

/// The sentence for the split mode the server found.
pub fn split_hint(mode: Option<SplitMode>) -> Option<&'static str> {
    let mode = mode?;
    SPLIT_OPTIONS.iter().find(|(m, _, _)| *m == mode).map(|(_, _, hint)| *hint).filter(|h| !h.is_empty())
}

/// The API's name for a split mode, for a select's value.
pub fn split_name(m: SplitMode) -> &'static str {
    match m {
        SplitMode::Auto => "auto",
        SplitMode::Line => "line",
        SplitMode::BlankLine => "blank_line",
        SplitMode::Separator => "separator",
        SplitMode::Heading => "heading",
        SplitMode::ListItem => "list_item",
        SplitMode::Whole => "whole",
    }
}

/// A file name without its last extension (".hidden" becomes "", as the web UI's).
pub fn strip_extension(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i + 1 < name.len() && !name[i + 1..].contains('/') => name[..i].to_owned(),
        _ => name.to_owned(),
    }
}

/// A count the user typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Count {
    /// Nothing typed.
    Empty,
    Value(i64),
    /// Not a whole number.
    Invalid,
}

impl Count {
    /// The number, when one was typed.
    pub fn value(self) -> Option<i64> {
        match self {
            Count::Value(n) => Some(n),
            _ => None,
        }
    }
}

/// "" is empty; digits only (commas and spaces ignored) are a number; anything else is invalid.
pub fn parse_count(s: &str) -> Count {
    let t: String = s.chars().filter(|c| !c.is_whitespace() && *c != ',').collect();
    if t.is_empty() {
        return Count::Empty;
    }
    if !t.chars().all(|c| c.is_ascii_digit()) {
        return Count::Invalid;
    }
    t.parse::<i64>().map(Count::Value).unwrap_or(Count::Invalid)
}

// ------------------------------------------------------------------ Simple and Advanced

/// Simple shows the source and what was detected; Advanced shows every setting. Both edit the same
/// state, so switching never resets a choice. The choice is remembered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportMode {
    #[default]
    Simple,
    Advanced,
}

const MODE_KEY: &str = "stellarator:import-mode";

/// The remembered mode (Simple when nothing was stored).
pub fn read_import_mode() -> ImportMode {
    if Storage::get_item(MODE_KEY).as_deref() == Some("advanced") { ImportMode::Advanced } else { ImportMode::Simple }
}

/// Remembers the mode (it lasts while the app is open when storage is not available).
pub fn save_import_mode(mode: ImportMode) {
    let _ = Storage::set_item(MODE_KEY, if mode == ImportMode::Advanced { "advanced" } else { "simple" });
}

// ------------------------------------------------------------------ explanations

/// The one- or two-sentence explanations behind the info buttons on the import screen.
pub mod info {
    pub const MODE: &str = "Simple picks the columns and settings for you and lists what it picked. Advanced shows every setting; switching keeps your choices.";
    pub const DATASET: &str = "Paste a link to the dataset's page or type its name as owner/name. Gated and private datasets need a Hugging Face token in Settings.";
    pub const CONFIG: &str = "Some datasets come in several configs, such as versions or languages, each with its own splits. Stellarator picks the one named default, or else the first.";
    pub const SPLIT: &str = "The part of the dataset to import, such as train or test. Stellarator picks train, or else the first split whose name starts with train, such as train_sft.";
    pub const FORMAT: &str = "How the file is read. Auto goes by the file extension, or by the contents when there is none; pick a format if it guesses wrong.";
    pub const PASTE: &str = "Each prompt becomes one user message. How the text is divided is detected for you; Advanced lets you choose.";
    pub const PROMPT_COLUMN: &str = "The column sent to the model as the prompt. Columns that hold model output, such as response, completion or chosen, are never picked for you.";
    pub const SYSTEM_COLUMN: &str = "Its text becomes the system message before each prompt. A system prompt set on a run replaces it.";
    pub const TURNS: &str = "First user turn keeps the system message and the first user message, with no text from another model. Whole conversation keeps every turn up to the last user message, including earlier replies written by the dataset's model.";
    pub const SPLIT_TEXT: &str = "How the text is divided into prompts. Automatic looks for --- lines first, then headings, list items, blank lines and line breaks; otherwise the whole text is one prompt.";
    pub const OFFSET_ROWS: &str = "Skips this many rows from the start. Rows are counted as they are read, before duplicates are removed.";
    pub const OFFSET_PROMPTS: &str = "Skips this many prompts from the start, counted before duplicates are removed.";
    pub const LIMIT_ROWS: &str = "Reads at most this many rows. Duplicates are removed afterward, so the set can end up smaller.";
    pub const LIMIT_PROMPTS: &str = "Imports at most this many prompts. Duplicates are removed afterward, so the set can end up smaller.";
    pub const DEDUPE: &str = "Keeps the first of any prompts whose final user message is exactly the same text.";
    pub const NAME: &str = "Shown in the prompt set list and when you choose prompts for a run. You can rename the set later from its page.";
}

// ------------------------------------------------------------------ Simple mode summary

/// Part of a summary value: plain text, or a name from the data (column, split, config) set in mono.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryPart {
    Text(String),
    Code(String),
}

fn t(s: &str) -> SummaryPart {
    SummaryPart::Text(s.to_owned())
}

fn c(s: &str) -> SummaryPart {
    SummaryPart::Code(s.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryRow {
    pub term: String,
    pub parts: Vec<SummaryPart>,
    /// Something to fix before importing.
    pub warn: bool,
}

fn row(term: &str, parts: Vec<SummaryPart>) -> SummaryRow {
    SummaryRow { term: term.to_owned(), parts, warn: false }
}

/// Where the prompts come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceTab {
    #[default]
    HuggingFace,
    File,
    Paste,
}

/// What the summary needs of the screen's state.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryState {
    pub source: SourceTab,
    pub turns: Turns,
    pub all_splits: bool,
    pub format: ImportFormat,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    pub dedupe: bool,
    /// The name the user typed, or none while the set keeps its automatic name.
    pub name: Option<String>,
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|f| f.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// What Simple mode shows in place of the settings: where the prompts come from, the split, the
/// system prompt and what is left out. Settings changed in Advanced follow, so Simple never hides a
/// choice that differs from the recommended one.
pub fn import_summary(p: &ImportPreviewOut, st: &SummaryState) -> Vec<SummaryRow> {
    let mut rows: Vec<SummaryRow> = Vec::new();
    let tabular = !p.columns.is_empty();

    match (&p.prompt_field, tabular) {
        (_, false) => {
            if let Some(how) = split_hint(p.split_mode) {
                rows.push(row("Prompts", vec![t(&capitalize(how))]));
            }
        }
        (None, true) => rows.push(SummaryRow { term: "Prompts".into(), parts: vec![t("No prompt column found")], warn: true }),
        (Some(field), true) if p.has_conversations => rows.push(row(
            "Prompts",
            vec![c(field), t(if st.turns == Turns::FirstUser { ", first user turn" } else { ", whole conversation" })],
        )),
        (Some(field), true) => rows.push(row("Prompts", vec![c(field), t(" column")])),
    }

    if st.source == SourceTab::HuggingFace && !p.hf_splits.is_empty() {
        let mut configs: Vec<&str> = p.hf_splits.iter().map(|s| s.config.as_str()).collect();
        configs.sort_unstable();
        configs.dedup();
        if let (true, Some(config)) = (configs.len() > 1, &p.hf_config) {
            rows.push(row("Config", vec![c(config)]));
        }
        if st.all_splits {
            rows.push(row("Split", vec![t("All splits")]));
        } else if let Some(split) = &p.hf_split {
            rows.push(row("Split", vec![c(split)]));
        }
    }

    if tabular && p.prompt_field.is_some() {
        if let Some(system) = &p.system_field {
            rows.push(row("System prompt", vec![c(system), t(" column")]));
        } else if p.prompts.iter().any(|m| m.first().and_then(|m| m.get("role")).and_then(|r| r.as_str()) == Some("system")) {
            rows.push(row("System prompt", vec![t("From the conversation")]));
        } else {
            rows.push(row("System prompt", vec![t("None")]));
        }
    }

    if !p.dropped.is_empty() {
        let mut parts = Vec::new();
        for (i, d) in p.dropped.iter().enumerate() {
            if i > 0 {
                parts.push(t(", "));
            }
            match d.strip_prefix("assistant turns in ").filter(|rest| !rest.is_empty()) {
                Some(rest) => {
                    parts.push(t("assistant turns in "));
                    parts.push(c(rest));
                }
                None => parts.push(c(d)),
            }
        }
        rows.push(row("Left out", parts));
    }

    if st.source == SourceTab::File && st.format != ImportFormat::Auto {
        rows.push(row("Format", vec![t(&format_label(format_name(st.format)))]));
    }
    if let Some(offset) = st.offset.filter(|n| *n != 0) {
        rows.push(row(if tabular { "Skip rows" } else { "Skip prompts" }, vec![t(&int(offset))]));
    }
    if let Some(limit) = st.limit.filter(|n| *n != 0) {
        rows.push(row("Import at most", vec![t(&int(limit))]));
    }
    if !st.dedupe {
        rows.push(row("Duplicate prompts", vec![t("Kept")]));
    }
    if let Some(name) = &st.name {
        rows.push(row("Name", vec![t(name)]));
    }
    rows
}

// ------------------------------------------------------------------ what the form says

/// The message under "Skip rows", or none.
pub fn offset_error(c: Count) -> Option<&'static str> {
    (c == Count::Invalid).then_some("Use a whole number, or leave empty.")
}

/// The message under "Import at most", or none: a whole number of 1 or more, or empty for all.
pub fn limit_error(c: Count) -> Option<&'static str> {
    match c {
        Count::Invalid => Some("Use a whole number, or leave empty."),
        Count::Value(0) => Some("Use 1 or more, or leave empty to import all."),
        _ => None,
    }
}

/// What decides whether Import can be pressed.
#[derive(Debug, Clone, Copy, Default)]
pub struct Gate {
    /// The preview has loaded.
    pub has_preview: bool,
    /// The preview failed.
    pub preview_error: bool,
    /// Prompts the import will create, when known.
    pub estimated: Option<i64>,
    /// The source has columns but none is the prompt.
    pub no_prompt_column: bool,
    pub name_missing: bool,
    pub offset_error: bool,
    pub limit_error: bool,
    /// A file is still uploading.
    pub uploading: bool,
    pub simple: bool,
}

impl Gate {
    /// Import is disabled.
    pub fn blocked(&self) -> bool {
        !self.has_preview
            || self.preview_error
            || self.estimated == Some(0)
            || self.no_prompt_column
            || self.name_missing
            || self.offset_error
            || self.limit_error
            || self.uploading
    }

    /// The line under the button that says what to do first.
    pub fn reason(&self) -> Option<&'static str> {
        if !self.has_preview {
            Some("Choose a source to import.")
        } else if self.preview_error {
            Some("Fix the problem in the preview first.")
        } else if self.no_prompt_column {
            Some(if self.simple { "Choose a prompt column in Advanced." } else { "Choose a prompt column." })
        } else if self.estimated == Some(0) {
            Some("Nothing to import with these settings.")
        } else if self.name_missing {
            Some("Give the prompt set a name.")
        } else if self.simple && (self.offset_error || self.limit_error) {
            Some("Fix the row limits in Advanced.")
        } else {
            None
        }
    }
}

/// The button's words: "Starting…" while it runs, "Import 7,473 prompts" once the count is known.
pub fn import_label(importing: bool, estimated: Option<i64>) -> String {
    if importing {
        "Starting…".to_owned()
    } else if let Some(n) = estimated {
        format!("Import {}", crate::format::count(n, "prompt", "prompts"))
    } else {
        "Import prompts".to_owned()
    }
}

/// The preview card's description: "First 20 of ~7,473 prompts", "12 prompts", or "First 20 prompts".
pub fn preview_summary(shown: usize, estimated: Option<i64>) -> String {
    const PREVIEW_ROWS: usize = 20;
    match estimated {
        Some(est) if est > shown as i64 => format!("First {} of ~{} prompts", int(shown as i64), int(est)),
        Some(est) => format!("{} {}", int(est), if est == 1 { "prompt" } else { "prompts" }),
        None => format!("First {} prompts", int(shown.min(PREVIEW_ROWS) as i64)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::types::{ColumnInfo, ColumnKind, HfSplit};

    /// "Term: value", with names from the data in backticks.
    fn lines(rows: &[SummaryRow]) -> Vec<String> {
        rows.iter()
            .map(|r| {
                let value: String = r
                    .parts
                    .iter()
                    .map(|p| match p {
                        SummaryPart::Text(s) => s.clone(),
                        SummaryPart::Code(s) => format!("`{s}`"),
                    })
                    .collect();
                format!("{}: {value}{}", r.term, if r.warn { " (warn)" } else { "" })
            })
            .collect()
    }

    fn col(name: &str, kind: ColumnKind, looks_like_output: bool) -> ColumnInfo {
        ColumnInfo { name: name.into(), kind, example: None, looks_like_output }
    }

    fn text_col(name: &str) -> ColumnInfo {
        col(name, ColumnKind::Text, false)
    }

    fn hf(config: &str, split: &str, rows: i64) -> HfSplit {
        HfSplit { config: config.into(), split: split.into(), num_rows: Some(rows) }
    }

    fn base() -> ImportPreviewOut {
        ImportPreviewOut {
            detected_format: "csv".into(),
            columns: vec![],
            prompt_field: None,
            system_field: None,
            split_mode: None,
            total_rows: None,
            estimated_prompts: None,
            prompts: vec![],
            warnings: vec![],
            hf_splits: vec![],
            hf_config: None,
            hf_split: None,
            has_conversations: false,
            dropped: vec![],
        }
    }

    fn recommended() -> SummaryState {
        SummaryState {
            source: SourceTab::HuggingFace,
            turns: Turns::FirstUser,
            all_splits: false,
            format: ImportFormat::Auto,
            offset: None,
            limit: None,
            dedupe: true,
            name: None,
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn links_and_names_load_as_owner_name_and_partial_input_does_not() {
        for (input, id) in [
            ("openai/gsm8k", "openai/gsm8k"),
            ("https://huggingface.co/datasets/openai/gsm8k", "openai/gsm8k"),
            ("https://huggingface.co/datasets/openai/gsm8k/viewer/main/train", "openai/gsm8k"),
            ("huggingface.co/datasets/HuggingFaceH4/ultrachat_200k?row=3", "HuggingFaceH4/ultrachat_200k"),
            ("  HTTP://www.HuggingFace.co/datasets/a/b#x ", "a/b"),
        ] {
            assert_eq!(parse_dataset(input), id, "{input}");
            assert!(is_dataset_id(&parse_dataset(input)), "{input}");
        }
        for input in ["", "openai", "openai/", "https://huggingface.co/datasets/", "open ai/gsm8k", "squad"] {
            assert!(!is_dataset_id(&parse_dataset(input)), "{input}");
        }
    }

    #[test]
    fn picking_a_config_keeps_the_split_it_has_or_takes_its_default() {
        let split = |c: &str, s: &str| HfSplit { config: c.into(), split: s.into(), num_rows: None };
        let splits = vec![split("one", "train"), split("one", "test"), split("two", "validation"), split("two", "train_sft")];
        assert_eq!(split_for_config(&splits, "two", Some("test")).as_deref(), Some("train_sft"));
        assert_eq!(split_for_config(&splits, "two", Some("validation")).as_deref(), Some("validation"));
        assert_eq!(split_for_config(&splits, "one", Some("test")).as_deref(), Some("test"));
        assert_eq!(split_for_config(&splits, "one", None).as_deref(), Some("train"));
        assert_eq!(split_for_config(&splits, "three", Some("train")), None);
    }

    #[test]
    fn a_hugging_face_set_is_named_by_its_dataset_and_split() {
        assert_eq!(hf_default_name("", Some("train"), false), "");
        assert_eq!(hf_default_name("openai/gsm8k", None, false), "openai/gsm8k");
        assert_eq!(hf_default_name("openai/gsm8k", Some("train"), false), "openai/gsm8k train");
        assert_eq!(hf_default_name("openai/gsm8k", Some("train"), true), "openai/gsm8k");
    }

    #[test]
    fn the_default_split_is_train_then_a_train_like_name_then_the_first() {
        assert_eq!(default_split(&names(&["test", "train"])).as_deref(), Some("train"));
        assert_eq!(default_split(&names(&["test_sft", "train_sft", "test_gen", "train_gen"])).as_deref(), Some("train_sft"));
        assert_eq!(default_split(&names(&["train_sft", "train"])).as_deref(), Some("train"));
        assert_eq!(default_split(&names(&["validation", "test"])).as_deref(), Some("validation"));
        assert_eq!(default_split(&[]), None);
    }

    #[test]
    fn a_question_answer_dataset_shows_the_prompt_column_split_and_config() {
        let p = ImportPreviewOut {
            columns: vec![text_col("question"), col("answer", ColumnKind::Text, true)],
            prompt_field: Some("question".into()),
            hf_splits: vec![hf("main", "train", 7473), hf("main", "test", 1319), hf("socratic", "train", 7473)],
            hf_config: Some("main".into()),
            hf_split: Some("train".into()),
            prompts: vec![vec![json!({"role": "user", "content": "Natalia sold clips…"})]],
            ..base()
        };
        assert_eq!(
            lines(&import_summary(&p, &recommended())),
            ["Prompts: `question` column", "Config: `main`", "Split: `train`", "System prompt: None"]
        );
    }

    #[test]
    fn a_conversation_dataset_shows_the_turns_setting_and_what_is_left_out() {
        let p = ImportPreviewOut {
            columns: vec![text_col("prompt"), col("messages", ColumnKind::Messages, false), text_col("prompt_id")],
            prompt_field: Some("messages".into()),
            has_conversations: true,
            dropped: vec!["assistant turns in messages".into()],
            hf_splits: vec![hf("default", "train_sft", 207_865), hf("default", "test_sft", 23_110)],
            hf_config: Some("default".into()),
            hf_split: Some("train_sft".into()),
            prompts: vec![vec![json!({"role": "user", "content": "Hi"})]],
            ..base()
        };
        assert_eq!(
            lines(&import_summary(&p, &recommended())),
            [
                "Prompts: `messages`, first user turn",
                "Split: `train_sft`",
                "System prompt: None",
                "Left out: assistant turns in `messages`"
            ]
        );
        // Whole conversation, chosen in Advanced, shows in Simple too.
        let whole = SummaryState { turns: Turns::UpToLastUser, ..recommended() };
        assert_eq!(lines(&import_summary(&p, &whole))[0], "Prompts: `messages`, whole conversation");
    }

    #[test]
    fn system_prompts_come_from_a_column_the_conversation_or_are_none() {
        let p = ImportPreviewOut {
            columns: vec![
                text_col("system"),
                text_col("prompt"),
                col("chosen", ColumnKind::Messages, true),
                col("rejected", ColumnKind::Messages, true),
            ],
            prompt_field: Some("prompt".into()),
            system_field: Some("system".into()),
            dropped: vec!["chosen".into(), "rejected".into()],
            ..base()
        };
        let file = SummaryState { source: SourceTab::File, ..recommended() };
        assert_eq!(
            lines(&import_summary(&p, &file)),
            ["Prompts: `prompt` column", "System prompt: `system` column", "Left out: `chosen`, `rejected`"]
        );
        let conv = ImportPreviewOut {
            columns: vec![col("conversations", ColumnKind::Messages, false)],
            prompt_field: Some("conversations".into()),
            has_conversations: true,
            prompts: vec![vec![json!({"role": "system", "content": "Be brief."}), json!({"role": "user", "content": "Hi"})]],
            ..base()
        };
        assert!(lines(&import_summary(&conv, &file)).contains(&"System prompt: From the conversation".to_owned()));
    }

    #[test]
    fn no_prompt_column_is_a_warning() {
        let p = ImportPreviewOut { columns: vec![col("id", ColumnKind::Number, false), col("response", ColumnKind::Text, true)], ..base() };
        let file = SummaryState { source: SourceTab::File, ..recommended() };
        assert_eq!(lines(&import_summary(&p, &file)), ["Prompts: No prompt column found (warn)"]);
    }

    #[test]
    fn pasted_text_says_how_it_was_split() {
        let p = ImportPreviewOut {
            split_mode: Some(SplitMode::Separator),
            prompts: vec![vec![json!({"role": "user", "content": "a"})]],
            ..base()
        };
        let paste = SummaryState { source: SourceTab::Paste, ..recommended() };
        assert_eq!(lines(&import_summary(&p, &paste)), ["Prompts: Split on --- lines"]);
        let whole = ImportPreviewOut { split_mode: Some(SplitMode::Whole), ..p };
        assert_eq!(lines(&import_summary(&whole, &paste)), ["Prompts: The whole text is one prompt"]);
    }

    #[test]
    fn settings_changed_in_advanced_are_listed_never_hidden() {
        let p = ImportPreviewOut { split_mode: Some(SplitMode::Line), ..base() };
        let st = SummaryState {
            source: SourceTab::File,
            format: ImportFormat::Markdown,
            offset: Some(10),
            limit: Some(500),
            dedupe: false,
            name: Some("Haiku prompts".into()),
            ..recommended()
        };
        assert_eq!(
            lines(&import_summary(&p, &st)),
            [
                "Prompts: One prompt per non-empty line",
                "Format: Markdown",
                "Skip prompts: 10",
                "Import at most: 500",
                "Duplicate prompts: Kept",
                "Name: Haiku prompts"
            ]
        );
        let all = ImportPreviewOut {
            columns: vec![text_col("text")],
            prompt_field: Some("text".into()),
            hf_splits: vec![hf("default", "train", 1)],
            hf_config: Some("default".into()),
            hf_split: Some("train".into()),
            ..base()
        };
        let st = SummaryState { all_splits: true, offset: Some(5), ..recommended() };
        assert_eq!(lines(&import_summary(&all, &st)), ["Prompts: `text` column", "Split: All splits", "System prompt: None", "Skip rows: 5"]);
    }

    #[test]
    fn counts_are_whole_numbers_or_empty() {
        assert_eq!(parse_count(""), Count::Empty);
        assert_eq!(parse_count("  "), Count::Empty);
        assert_eq!(parse_count("1,500"), Count::Value(1500));
        assert_eq!(parse_count(" 12 "), Count::Value(12));
        assert_eq!(parse_count("0"), Count::Value(0));
        assert_eq!(parse_count("12.5"), Count::Invalid);
        assert_eq!(parse_count("-3"), Count::Invalid);
        assert_eq!(parse_count("ten"), Count::Invalid);
        assert_eq!(parse_count("99999999999999999999"), Count::Invalid);
        assert_eq!(Count::Value(4).value(), Some(4));
        assert_eq!(Count::Empty.value(), None);
    }

    #[test]
    fn row_limits_are_checked_where_they_are_typed() {
        assert_eq!(offset_error(Count::Empty), None);
        assert_eq!(offset_error(Count::Value(0)), None);
        assert_eq!(offset_error(Count::Invalid), Some("Use a whole number, or leave empty."));
        assert_eq!(limit_error(Count::Empty), None);
        assert_eq!(limit_error(Count::Value(10)), None);
        assert_eq!(limit_error(Count::Value(0)), Some("Use 1 or more, or leave empty to import all."));
        assert_eq!(limit_error(Count::Invalid), Some("Use a whole number, or leave empty."));
    }

    #[test]
    fn import_waits_for_a_preview_and_says_what_is_missing() {
        let ready = Gate { has_preview: true, estimated: Some(12), ..Gate::default() };
        assert!(!ready.blocked());
        assert_eq!(ready.reason(), None);
        assert_eq!(Gate::default().reason(), Some("Choose a source to import."));
        assert!(Gate::default().blocked());
        let failed = Gate { preview_error: true, ..ready };
        assert!(failed.blocked());
        assert_eq!(failed.reason(), Some("Fix the problem in the preview first."));
        let no_column = Gate { no_prompt_column: true, ..ready };
        assert_eq!(no_column.reason(), Some("Choose a prompt column."));
        assert_eq!(Gate { simple: true, ..no_column }.reason(), Some("Choose a prompt column in Advanced."));
        let empty = Gate { estimated: Some(0), ..ready };
        assert!(empty.blocked());
        assert_eq!(empty.reason(), Some("Nothing to import with these settings."));
        let unnamed = Gate { name_missing: true, ..ready };
        assert_eq!(unnamed.reason(), Some("Give the prompt set a name."));
        // The row-limit hint is for Simple mode, where the fields are hidden; Advanced shows them.
        let limit = Gate { limit_error: true, ..ready };
        assert!(limit.blocked());
        assert_eq!(limit.reason(), None);
        assert_eq!(Gate { simple: true, ..limit }.reason(), Some("Fix the row limits in Advanced."));
        // An upload in flight blocks without a reason.
        let uploading = Gate { uploading: true, ..ready };
        assert!(uploading.blocked());
        assert_eq!(uploading.reason(), None);
    }

    #[test]
    fn the_button_and_the_preview_count_prompts() {
        assert_eq!(import_label(true, Some(3)), "Starting…");
        assert_eq!(import_label(false, Some(1)), "Import 1 prompt");
        assert_eq!(import_label(false, Some(7473)), "Import 7,473 prompts");
        assert_eq!(import_label(false, None), "Import prompts");
        assert_eq!(preview_summary(20, Some(7473)), "First 20 of ~7,473 prompts");
        assert_eq!(preview_summary(3, Some(3)), "3 prompts");
        assert_eq!(preview_summary(1, Some(1)), "1 prompt");
        assert_eq!(preview_summary(20, None), "First 20 prompts");
        assert_eq!(preview_summary(35, None), "First 20 prompts");
    }

    #[test]
    fn a_file_name_loses_its_extension_only() {
        assert_eq!(strip_extension("prompts.md"), "prompts");
        assert_eq!(strip_extension("my.data.csv"), "my.data");
        assert_eq!(strip_extension("README"), "README");
        assert_eq!(strip_extension(".hidden"), "");
        assert_eq!(strip_extension("trailing."), "trailing.");
    }

    #[test]
    fn formats_and_split_modes_have_their_words() {
        assert_eq!(format_label("csv"), "CSV");
        assert_eq!(format_label("markdown"), "Markdown");
        assert_eq!(format_label("huggingface"), "Hugging Face");
        assert_eq!(format_label("ndjson"), "ndjson");
        assert_eq!(split_hint(Some(SplitMode::Auto)), None);
        assert_eq!(split_hint(None), None);
        assert_eq!(split_hint(Some(SplitMode::Heading)), Some("split on Markdown headings"));
        // The names are the API's own.
        for (mode, _, _) in SPLIT_OPTIONS {
            assert_eq!(serde_json::to_value(mode).unwrap(), json!(split_name(mode)));
        }
        for (format, _) in FORMAT_OPTIONS {
            assert_eq!(serde_json::to_value(format).unwrap(), json!(format_name(format)));
        }
    }
}
