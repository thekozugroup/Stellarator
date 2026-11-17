//! Rows and text pieces to prompts: field plans, prompt building, dedupe, stats and previews.

use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hash, Hasher};

use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::Value;
use stellarator_core::schemas::{ColumnInfo, ColumnKind, ImportMapping, ImportPreviewOut, SplitMode, Turns};

use crate::files::TextDoc;
use crate::messages::{drops_assistant_turns, normalize_messages, py_str, Msg};
use crate::problem::{thousands, Problem, Result};
use crate::schema::{auto_prompt_field, auto_system_field, Schema};
use crate::table::{Flow, Row, Table, TableSource};
use crate::text::split_text;

pub const MAX_META_CHARS: usize = 4000;
pub const PREVIEW_PROMPTS: usize = 20;

pub const NO_TEXT_COLUMN: &str =
    "Couldn't find a text column to use as the prompt. Choose the column that holds the prompts.";
pub const NO_PROMPTS_FOUND: &str =
    "No prompts were found with these settings. Check the column or split mode and try again.";

/// A prompt ready to store: messages ending in a user turn, and scalar meta in column order.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub messages: Vec<Msg>,
    pub meta: Vec<(String, Value)>,
}

/// Serializes `meta` as a JSON object that keeps the columns' order.
pub struct MetaJson<'a>(pub &'a [(String, Value)]);

impl Serialize for MetaJson<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in self.0 {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}

impl Prompt {
    pub fn user(text: String) -> Self {
        Self { messages: vec![Msg::new("user", text)], meta: Vec::new() }
    }
    /// The final user text, which dedupe compares.
    pub fn key(&self) -> &str {
        self.messages.last().map_or("", |m| m.content.as_str())
    }
    pub fn messages_value(&self) -> Vec<Value> {
        self.messages.iter().map(Msg::to_value).collect()
    }
    pub fn meta_value(&self) -> Value {
        Value::Object(self.meta.iter().cloned().collect())
    }
    /// (messages JSON, meta JSON) as stored.
    pub fn encode(&self) -> (String, String) {
        (
            serde_json::to_string(&self.messages).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&MetaJson(&self.meta)).unwrap_or_else(|_| "{}".into()),
        )
    }
}

// ------------------------------------------------------------------------------------ plans

/// Which columns make the prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub prompt_field: String,
    pub system_field: Option<String>,
    /// Alpaca: "instruction" plus "input".
    pub input_field: Option<String>,
    pub meta_fields: Vec<String>,
    pub turns: Turns,
    pub has_conversations: bool,
    /// What is left out: "assistant turns in <column>", then model-output columns.
    pub dropped: Vec<String>,
}

/// A plan bound to one table's column positions.
pub struct Bound {
    prompt: usize,
    system: Option<usize>,
    input: Option<usize>,
    meta: Vec<(String, usize)>,
    turns: Turns,
}

impl Plan {
    pub fn bind(&self, names: &[String]) -> Result<Bound> {
        let find = |n: &str| {
            names.iter().position(|x| x == n).ok_or_else(|| {
                Problem::status(502, format!("The dataset's Parquet files have no column called '{n}'. Choose another column."))
            })
        };
        Ok(Bound {
            prompt: find(&self.prompt_field)?,
            system: self.system_field.as_deref().map(find).transpose()?,
            input: self.input_field.as_deref().map(find).transpose()?,
            meta: self
                .meta_fields
                .iter()
                .filter_map(|m| names.iter().position(|x| x == m).map(|i| (m.clone(), i)))
                .collect(),
            turns: self.turns,
        })
    }
}

impl Bound {
    pub fn columns(&self) -> Vec<usize> {
        let mut c = vec![self.prompt];
        c.extend(self.system);
        c.extend(self.input);
        c.extend(self.meta.iter().map(|(_, i)| *i));
        c.sort_unstable();
        c.dedup();
        c
    }
}

/// Resolve fields. `(plan or None when no column fits, prompt_field, system_field)`. A named field
/// that doesn't exist is an error.
pub fn resolve_plan(schema: &Schema, sample: &[Row], mapping: &ImportMapping) -> Result<(Option<Plan>, Option<String>, Option<String>)> {
    let names = schema.names();
    let no_system = mapping.system_field.as_deref() == Some("");
    for (label, field, key) in [
        ("prompt", mapping.prompt_field.as_deref(), "mapping.prompt_field"),
        ("system", mapping.system_field.as_deref(), "mapping.system_field"),
    ] {
        if let Some(f) = field.filter(|f| !f.is_empty()) {
            if !names.iter().any(|n| n == f) {
                let mut shown = names.iter().take(12).cloned().collect::<Vec<_>>().join(", ");
                if names.len() > 12 {
                    shown.push_str(", …");
                }
                return Err(Problem::new(format!(
                    "There is no column called '{f}' to use as the {label}. Columns are: {shown}."
                ))
                .on(key));
            }
        }
    }
    let prompt_field = mapping.prompt_field.clone().filter(|f| !f.is_empty()).or_else(|| auto_prompt_field(schema));
    let mut system_field = if no_system {
        None
    } else {
        mapping.system_field.clone().filter(|f| !f.is_empty()).or_else(|| auto_system_field(schema, prompt_field.as_deref()))
    };
    if system_field.is_some() && system_field == prompt_field {
        system_field = None;
    }
    let Some(prompt) = prompt_field.clone() else { return Ok((None, None, system_field)) };
    let mut input_field = None;
    if prompt.to_lowercase() == "instruction" {
        for c in &schema.columns {
            if c.name.to_lowercase() == "input" && c.kind == ColumnKind::Text {
                input_field = Some(c.name.clone());
            }
        }
    }
    let used = |n: &str| n == prompt || Some(n) == system_field.as_deref() || Some(n) == input_field.as_deref();
    let meta_fields = schema
        .columns
        .iter()
        .zip(&schema.scalar)
        .filter(|(c, &scalar)| scalar && !c.looks_like_output && !used(&c.name))
        .map(|(c, _)| c.name.clone())
        .collect();
    let has_conversations = schema.kind(&prompt) == Some(&ColumnKind::Messages);
    let mut dropped = Vec::new();
    if has_conversations {
        if let Some(i) = schema.index(&prompt) {
            if sample.iter().any(|r| r.get(i).is_some_and(|v| drops_assistant_turns(v, mapping.turns))) {
                dropped.push(format!("assistant turns in {prompt}"));
            }
        }
    }
    dropped.extend(schema.columns.iter().filter(|c| c.looks_like_output && !used(&c.name)).map(|c| c.name.clone()));
    let plan = Plan {
        prompt_field: prompt,
        system_field: system_field.clone(),
        input_field,
        meta_fields,
        turns: mapping.turns,
        has_conversations,
        dropped,
    };
    Ok((Some(plan), prompt_field, system_field))
}

fn meta_value(v: &Value) -> Option<Value> {
    match v {
        Value::Bool(_) => Some(v.clone()),
        Value::String(s) => {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                Some(Value::String(s.chars().take(MAX_META_CHARS).collect()))
            }
        }
        Value::Number(_) => Some(v.clone()),
        _ => None,
    }
}

pub fn build_prompt(row: &Row, b: &Bound) -> Option<Prompt> {
    let val = row.get(b.prompt)?;
    let mut messages = match val {
        Value::Null | Value::Object(_) => return None,
        Value::Array(_) => normalize_messages(val, b.turns)?,
        other => {
            let mut text = py_str(other).trim().to_string();
            if let Some(extra) = b.input.and_then(|i| row.get(i)).filter(|v| !v.is_null()) {
                let extra = py_str(extra);
                let extra = extra.trim();
                if !extra.is_empty() {
                    text = if text.is_empty() { extra.to_string() } else { format!("{text}\n\n{extra}") };
                }
            }
            if text.is_empty() {
                return None;
            }
            vec![Msg::new("user", text)]
        }
    };
    if let Some(Value::String(sys)) = b.system.and_then(|i| row.get(i)) {
        let sys = sys.trim();
        if !sys.is_empty() {
            if messages.first().is_some_and(|m| m.role == "system") {
                messages[0] = Msg::new("system", sys);
            } else {
                messages.insert(0, Msg::new("system", sys));
            }
        }
    }
    let meta = b
        .meta
        .iter()
        .filter_map(|(name, i)| row.get(*i).and_then(meta_value).map(|v| (name.clone(), v)))
        .collect();
    Some(Prompt { messages, meta })
}

// ------------------------------------------------------------------------------------ stats

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub rows: u64,
    pub empty: u64,
    pub duplicates: u64,
}

impl Stats {
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.empty > 0 {
            let one = self.empty == 1;
            out.push(format!(
                "{} {} had an empty prompt and {} skipped.",
                thousands(self.empty),
                if one { "row" } else { "rows" },
                if one { "was" } else { "were" }
            ));
        }
        if self.duplicates > 0 {
            let one = self.duplicates == 1;
            out.push(format!(
                "{} duplicate {} dropped.",
                thousands(self.duplicates),
                if one { "prompt was" } else { "prompts were" }
            ));
        }
        out
    }
}

/// The hash is already a hash: use it as is.
#[derive(Default)]
pub struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8) | b as u64;
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

/// Seen prompts as 64-bit hashes: about 18 bytes per prompt instead of the text.
#[derive(Default)]
pub struct Dedupe {
    seen: HashSet<u64, BuildHasherDefault<IdentityHasher>>,
}

impl Dedupe {
    /// True the first time `text` is seen.
    pub fn first(&mut self, text: &str) -> bool {
        let mut h = std::hash::DefaultHasher::new();
        text.hash(&mut h);
        self.seen.insert(h.finish())
    }
}

/// Turns rows into prompts, counting skipped and duplicate ones.
pub struct Builder {
    pub bound: Bound,
    pub stats: Stats,
    dedupe: Option<Dedupe>,
}

impl Builder {
    pub fn new(bound: Bound, dedupe: bool) -> Self {
        Self { bound, stats: Stats::default(), dedupe: dedupe.then(Dedupe::default) }
    }
    pub fn take(&mut self, row: &Row) -> Option<Prompt> {
        self.stats.rows += 1;
        let Some(p) = build_prompt(row, &self.bound) else {
            self.stats.empty += 1;
            return None;
        };
        if let Some(d) = self.dedupe.as_mut() {
            if !d.first(p.key()) {
                self.stats.duplicates += 1;
                return None;
            }
        }
        Some(p)
    }
}

/// Text pieces to prompts, windowed and deduplicated.
pub struct TextPrompts {
    pub stats: Stats,
    dedupe: Option<Dedupe>,
}

impl TextPrompts {
    pub fn new(dedupe: bool) -> Self {
        Self { stats: Stats::default(), dedupe: dedupe.then(Dedupe::default) }
    }
    pub fn take(&mut self, piece: &str) -> Option<Prompt> {
        self.stats.rows += 1;
        if let Some(d) = self.dedupe.as_mut() {
            if !d.first(piece) {
                self.stats.duplicates += 1;
                return None;
            }
        }
        Some(Prompt::user(piece.to_string()))
    }
}

pub fn window<'a, T>(items: &'a [T], mapping: &ImportMapping) -> &'a [T] {
    let start = (mapping.offset.max(0) as usize).min(items.len());
    let end = match mapping.limit {
        Some(l) => start.saturating_add(l.max(0) as usize).min(items.len()),
        None => items.len(),
    };
    &items[start..end]
}

/// Read prompts from a table through a plan: `on_prompt` gets each prompt, returning Stop to end.
pub fn scan_prompts(
    table: &mut dyn Table,
    plan: &Plan,
    offset: u64,
    limit: Option<u64>,
    dedupe: bool,
    on_prompt: &mut dyn FnMut(Prompt) -> Flow,
) -> Result<Stats> {
    let bound = plan.bind(table.names())?;
    let columns = bound.columns();
    let mut builder = Builder::new(bound, dedupe);
    let mut sink = |row: Row| match builder.take(&row) {
        Some(p) => on_prompt(p),
        None => Flow::Continue,
    };
    table.scan(Some(&columns), offset, limit, &mut sink)?;
    Ok(builder.stats)
}

// ------------------------------------------------------------------------------------ previews

pub fn empty_preview(format: &str) -> ImportPreviewOut {
    ImportPreviewOut {
        detected_format: format.to_string(),
        columns: Vec::new(),
        prompt_field: None,
        system_field: None,
        split_mode: None,
        total_rows: None,
        estimated_prompts: None,
        prompts: Vec::new(),
        warnings: Vec::new(),
        hf_splits: Vec::new(),
        hf_config: None,
        hf_split: None,
        has_conversations: false,
        dropped: Vec::new(),
    }
}

pub fn preview_text(doc: &TextDoc, mapping: &ImportMapping) -> ImportPreviewOut {
    let (pieces, mode) = split_text(&doc.text, mapping.split_mode, doc.markdown());
    let win = window(&pieces, mapping);
    let mut tp = TextPrompts::new(mapping.dedupe);
    let mut prompts = Vec::new();
    let mut n = 0i64;
    for piece in win {
        if let Some(p) = tp.take(piece) {
            n += 1;
            if prompts.len() < PREVIEW_PROMPTS {
                prompts.push(p.messages_value());
            }
        }
    }
    let mut warnings = doc.warnings.clone();
    warnings.extend(tp.stats.warnings());
    if prompts.is_empty() {
        warnings.push("No prompts found in this text.".into());
    }
    let mut out = empty_preview(doc.kind);
    out.split_mode = Some(mode);
    out.total_rows = Some(pieces.len() as i64);
    out.estimated_prompts = Some(n);
    out.prompts = prompts;
    out.warnings = warnings;
    out
}

pub struct PreviewOptions {
    /// Count every prompt (default: when the table is small).
    pub full_pass: Option<bool>,
    /// False when the table already holds just the requested window.
    pub apply_window: bool,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self { full_pass: None, apply_window: true }
    }
}

pub fn preview_table(src: &mut TableSource, mapping: &ImportMapping, opts: PreviewOptions) -> Result<ImportPreviewOut> {
    let (plan, prompt_field, system_field) = resolve_plan(&src.schema, &src.sample, mapping)?;
    let table = src.table.as_mut();
    let mut total = table.total_rows().map(|t| t as i64);
    let (offset, limit) = if opts.apply_window {
        (mapping.offset.max(0) as u64, mapping.limit.map(|l| l.max(0) as u64))
    } else {
        (0, None)
    };
    let mut warnings: Vec<String> = Vec::new();
    let mut prompts: Vec<Vec<Value>> = Vec::new();
    let mut estimated: Option<i64> = None;
    match &plan {
        None => {
            warnings.extend(table.warnings());
            warnings.push("No column looks like a prompt. Choose the column that holds the prompts.".into());
        }
        Some(plan) => {
            scan_prompts(table, plan, offset, limit, mapping.dedupe, &mut |p| {
                prompts.push(p.messages_value());
                if prompts.len() >= PREVIEW_PROMPTS {
                    Flow::Stop
                } else {
                    Flow::Continue
                }
            })?;
            let full_pass = opts.full_pass.unwrap_or_else(|| table.small());
            let mut stat_warnings = Vec::new();
            if full_pass {
                let mut n = 0i64;
                let stats = scan_prompts(table, plan, offset, limit, mapping.dedupe, &mut |_| {
                    n += 1;
                    Flow::Continue
                })?;
                estimated = Some(n);
                if total.is_none() && mapping.offset == 0 && mapping.limit.is_none() {
                    total = Some(stats.rows as i64);
                }
                stat_warnings = stats.warnings();
                if !opts.apply_window && (mapping.offset > 0 || mapping.limit.is_some()) {
                    estimated = None;
                }
            }
            warnings.extend(table.warnings());
            warnings.extend(stat_warnings);
            if prompts.is_empty() {
                warnings.push("No prompts could be read with these settings.".into());
            }
        }
    }
    if estimated.is_none() {
        if let Some(t) = total {
            let lim = mapping.limit.unwrap_or(t);
            estimated = Some(lim.min(t - mapping.offset).max(0));
        }
    }
    let mut out = empty_preview(table.format());
    out.columns = src.schema.columns.clone();
    out.prompt_field = prompt_field;
    out.system_field = system_field;
    out.total_rows = total;
    out.estimated_prompts = estimated;
    out.prompts = prompts;
    out.warnings = warnings;
    if let Some(p) = plan {
        out.has_conversations = p.has_conversations;
        out.dropped = p.dropped;
    }
    Ok(out)
}

/// For tests and logs.
pub fn column_names(cols: &[ColumnInfo]) -> Vec<&str> {
    cols.iter().map(|c| c.name.as_str()).collect()
}

/// Text docs: the pieces a commit will import (after the window), or an error when there are none.
pub fn text_pieces(doc: &TextDoc, mapping: &ImportMapping) -> (Vec<String>, SplitMode) {
    split_text(&doc.text, mapping.split_mode, doc.markdown())
}
