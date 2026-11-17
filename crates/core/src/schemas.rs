//! The API contract: request and response bodies. `frontend/src/lib/types.ts` mirrors this file;
//! change both together. Field names, defaults and validation messages match the Python backend
//! this replaced, except where marked "v2" (background imports, prompts-only imports, presets).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{FieldErrors, Validate};
use crate::no_validation;

/// `{"role": "system" | "user" | "assistant", "content": str}`. Kept as JSON so unusual content
/// (lists of parts) survives.
pub type Message = Value;
pub type JsonMap = serde_json::Map<String, Value>;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

fn check_len(errors: &mut FieldErrors, field: &str, value: &str, min: usize, max: usize) {
    let n = value.chars().count();
    if n < min {
        errors.add(field, if min == 1 { "Required.".to_string() } else { format!("At least {min} characters.") });
    } else if n > max {
        errors.add(field, format!("At most {max} characters."));
    }
}

fn check_range_i(errors: &mut FieldErrors, field: &str, v: i64, min: i64, max: i64) {
    if v < min || v > max {
        errors.add(field, format!("Use a whole number from {min} to {max}."));
    }
}

/// Strip and drop a trailing slash; must start with http:// or https://.
pub fn normalize_base_url(errors: &mut FieldErrors, field: &str, url: &mut String) {
    let v = url.trim().trim_end_matches('/').to_string();
    if !(v.starts_with("http://") || v.starts_with("https://")) {
        errors.add(field, "Base URL must start with http:// or https://");
    }
    *url = v;
}

// ------------------------------------------------------------------------------------ endpoints

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Location {
    #[default]
    Local,
    Cloud,
}

impl Location {
    pub fn as_str(self) -> &'static str {
        match self {
            Location::Local => "local",
            Location::Cloud => "cloud",
        }
    }
}

fn d_concurrency() -> i64 {
    16
}

#[derive(Deserialize, Debug, Clone)]
pub struct EndpointIn {
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub location: Location,
    #[serde(default = "d_concurrency")]
    pub default_concurrency: i64,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

impl Validate for EndpointIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        check_len(e, "name", &self.name, 1, 200);
        check_len(e, "base_url", &self.base_url, 1, 2000);
        normalize_base_url(e, "base_url", &mut self.base_url);
        check_range_i(e, "default_concurrency", self.default_concurrency, 1, 4096);
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct EndpointPatch {
    pub name: Option<String>,
    pub base_url: Option<String>,
    /// None (or absent) leaves the key unchanged; "" removes it.
    pub api_key: Option<String>,
    pub location: Option<Location>,
    pub default_concurrency: Option<i64>,
    pub headers: Option<BTreeMap<String, String>>,
}

impl Validate for EndpointPatch {
    fn validate(&mut self, e: &mut FieldErrors) {
        if let Some(n) = &self.name {
            check_len(e, "name", n, 1, 200);
        }
        if let Some(u) = &mut self.base_url {
            normalize_base_url(e, "base_url", u);
        }
        if let Some(c) = self.default_concurrency {
            check_range_i(e, "default_concurrency", c, 1, 4096);
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct EndpointOut {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub has_api_key: bool,
    /// Last 4 characters, e.g. "…a9f2".
    pub api_key_hint: Option<String>,
    pub location: String,
    pub default_concurrency: i64,
    pub headers: BTreeMap<String, String>,
    pub models: Vec<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub last_ok: Option<bool>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub run_count: i64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct EndpointTestIn {
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// When testing an existing endpoint's edit form with the key left blank, use its saved key.
    #[serde(default)]
    pub endpoint_id: Option<i64>,
}

impl Validate for EndpointTestIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        normalize_base_url(e, "base_url", &mut self.base_url);
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct EndpointTestOut {
    pub ok: bool,
    pub models: Vec<String>,
    pub latency_ms: Option<i64>,
    pub error: Option<String>,
}

/// v2: a starting point for the Add endpoint form (`GET /api/endpoint-presets`).
#[derive(Serialize, Debug, Clone)]
pub struct EndpointPreset {
    /// "openrouter" | "lmstudio" | "ollama" | "vllm" | "sglang" | "llamacpp" | "openai"
    pub id: &'static str,
    pub name: &'static str,
    /// Already adjusted for where Stellarator runs (localhost, or host.docker.internal in Docker).
    pub base_url: String,
    pub location: Location,
    pub default_concurrency: i64,
    pub needs_api_key: bool,
    /// Where to get a key, when one is needed.
    pub api_key_url: Option<&'static str>,
    pub headers: BTreeMap<String, String>,
}

// ------------------------------------------------------------------------------------ prompt sets

/// v2: imports run in the background.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PromptSetStatus {
    Importing,
    Ready,
    Failed,
}

impl PromptSetStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Importing => "importing",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "importing" => Self::Importing,
            "failed" => Self::Failed,
            _ => Self::Ready,
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct PromptSetOut {
    pub id: i64,
    pub name: String,
    pub source_kind: String,
    pub source: Value,
    /// Prompts stored so far (grows while importing).
    pub prompt_count: i64,
    pub created_at: DateTime<Utc>,
    pub run_count: i64,
    /// v2
    pub status: PromptSetStatus,
    /// v2: why it failed, or what it is doing ("Reading train"), else null.
    pub status_detail: Option<String>,
    /// v2: source rows read so far, and the total when known.
    pub rows_read: i64,
    pub rows_total: Option<i64>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PromptSetPatch {
    pub name: String,
}

impl Validate for PromptSetPatch {
    fn validate(&mut self, e: &mut FieldErrors) {
        check_len(e, "name", &self.name, 1, 300);
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct PromptOut {
    pub id: i64,
    pub idx: i64,
    pub messages: Vec<Message>,
    pub meta: Value,
}

// ------------------------------------------------------------------------------------ imports

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ImportFormat {
    #[default]
    Auto,
    Text,
    Markdown,
    Csv,
    Tsv,
    Json,
    Jsonl,
    Parquet,
}

/// How plain text and Markdown are split into prompts.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SplitMode {
    /// Pick by content: separators, then headings, then list items, then blank lines.
    #[default]
    Auto,
    Line,
    BlankLine,
    Separator,
    Heading,
    ListItem,
    Whole,
}

/// v2: which part of a conversation becomes the prompt. Assistant turns after the chosen user turn
/// are always dropped, so a distilled dataset's responses never enter the prompt set.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Turns {
    /// Everything up to and including the last user turn (system message and earlier turns kept).
    #[default]
    UpToLastUser,
    /// The system message (if any) and the first user turn only.
    FirstUser,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ImportSource {
    Huggingface {
        /// "owner/name" or a huggingface.co URL.
        dataset: String,
        #[serde(default)]
        config: Option<String>,
        #[serde(default)]
        split: Option<String>,
        /// v2: import every split of the config (split is ignored).
        #[serde(default)]
        all_splits: bool,
    },
    File {
        /// Returned by POST /api/imports/upload.
        upload_id: String,
        #[serde(default)]
        format: ImportFormat,
    },
    Paste {
        text: String,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct ImportMapping {
    /// Column holding the prompt: a string, or a list of chat messages (OpenAI {role, content} or
    /// ShareGPT {from, value}). None = auto-detect.
    #[serde(default)]
    pub prompt_field: Option<String>,
    /// Column holding a system prompt. None = detect automatically, "" = none.
    #[serde(default)]
    pub system_field: Option<String>,
    #[serde(default)]
    pub split_mode: SplitMode,
    /// v2
    #[serde(default)]
    pub turns: Turns,
    /// Row window, applied to source rows. limit None = all rows.
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub limit: Option<i64>,
    /// Drop prompts whose final user text equals an earlier one.
    #[serde(default = "d_true")]
    pub dedupe: bool,
}

fn d_true() -> bool {
    true
}

impl ImportMapping {
    fn check(&self, e: &mut FieldErrors) {
        if self.offset < 0 {
            e.add("mapping.offset", "Use 0 or more.");
        }
        if let Some(l) = self.limit {
            if l < 1 {
                e.add("mapping.limit", "Use 1 or more, or leave empty to import all.");
            }
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct ImportPreviewIn {
    pub source: ImportSource,
    #[serde(default)]
    pub mapping: ImportMapping,
}

impl Validate for ImportPreviewIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        self.mapping.check(e);
        if let ImportSource::Huggingface { dataset, .. } = &self.source {
            if dataset.trim().is_empty() {
                e.add("source.dataset", "Required.");
            }
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct ImportCommitIn {
    pub source: ImportSource,
    #[serde(default)]
    pub mapping: ImportMapping,
    pub name: String,
}

impl Validate for ImportCommitIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        self.mapping.check(e);
        check_len(e, "name", &self.name, 1, 300);
        if let ImportSource::Huggingface { dataset, .. } = &self.source {
            if dataset.trim().is_empty() {
                e.add("source.dataset", "Required.");
            }
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ColumnKind {
    Text,
    Messages,
    Number,
    Other,
}

#[derive(Serialize, Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub kind: ColumnKind,
    /// First non-empty value, shortened.
    pub example: Option<String>,
    /// v2: true when the column looks like model output (response, completion, reasoning...). Such
    /// columns are never picked automatically as the prompt.
    pub looks_like_output: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct HfSplit {
    pub config: String,
    pub split: String,
    pub num_rows: Option<i64>,
}

#[derive(Serialize, Debug, Clone)]
pub struct ImportPreviewOut {
    /// "huggingface", "csv", "markdown", ...
    pub detected_format: String,
    /// Empty for text/markdown/paste.
    pub columns: Vec<ColumnInfo>,
    pub prompt_field: Option<String>,
    pub system_field: Option<String>,
    /// Set for text/markdown/paste.
    pub split_mode: Option<SplitMode>,
    /// Rows in the source (before offset/limit/dedupe), when known.
    pub total_rows: Option<i64>,
    /// Prompts the import will create, when known.
    pub estimated_prompts: Option<i64>,
    /// The first prompts as they will be stored (up to 20).
    pub prompts: Vec<Vec<Message>>,
    pub warnings: Vec<String>,
    /// Hugging Face only: every config/split.
    pub hf_splits: Vec<HfSplit>,
    pub hf_config: Option<String>,
    pub hf_split: Option<String>,
    /// v2: the prompts are taken from conversations, so `turns` applies.
    pub has_conversations: bool,
    /// v2: assistant turns or output columns that the import leaves out, e.g. ["messages: assistant
    /// turns", "response", "reasoning"]. Lets the UI say "Responses are left out" once.
    pub dropped: Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct UploadOut {
    pub upload_id: String,
    pub filename: String,
    pub size: i64,
    pub detected_format: String,
}

// ------------------------------------------------------------------------------------ runs

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MaxTokensField {
    #[default]
    #[serde(rename = "max_tokens")]
    MaxTokens,
    #[serde(rename = "max_completion_tokens")]
    MaxCompletionTokens,
}

impl MaxTokensField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaxTokens => "max_tokens",
            Self::MaxCompletionTokens => "max_completion_tokens",
        }
    }
}

fn d_samples() -> i64 {
    1
}
fn d_temperature() -> Option<f64> {
    Some(0.7)
}
fn d_max_tokens() -> Option<i64> {
    Some(8192)
}
fn d_think_open() -> String {
    "<think>".into()
}
fn d_think_close() -> String {
    "</think>".into()
}
fn d_attempts() -> i64 {
    4
}
fn d_timeout() -> f64 {
    1800.0
}
fn d_pause_after() -> i64 {
    10
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RunParams {
    #[serde(default = "d_samples")]
    pub samples_per_prompt: i64,
    #[serde(default = "d_temperature")]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default = "d_max_tokens")]
    pub max_tokens: Option<i64>,
    /// Some APIs (OpenAI reasoning models) want max_completion_tokens instead of max_tokens.
    #[serde(default)]
    pub max_tokens_field: MaxTokensField,
    #[serde(default)]
    pub seed: Option<i64>,
    /// Replaces any system message from the prompt set when set.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Wraps each user prompt; must contain {prompt}.
    #[serde(default)]
    pub user_template: Option<String>,
    /// Merged into the request body last.
    #[serde(default)]
    pub extra_body: JsonMap,
    #[serde(default = "d_true")]
    pub stream: bool,
    #[serde(default = "d_think_open")]
    pub think_open: String,
    #[serde(default = "d_think_close")]
    pub think_close: String,
    #[serde(default = "d_attempts")]
    pub max_attempts: i64,
    #[serde(default = "d_timeout")]
    pub request_timeout_s: f64,
    /// Pause the run after this many samples fail in a row (0 = never).
    #[serde(default = "d_pause_after")]
    pub pause_after_failures: i64,
}

impl Default for RunParams {
    fn default() -> Self {
        serde_json::from_value(Value::Object(Default::default())).expect("defaults")
    }
}

impl RunParams {
    pub fn check(&mut self, prefix: &str, e: &mut FieldErrors) {
        let f = |name: &str| format!("{prefix}{name}");
        check_range_i(e, &f("samples_per_prompt"), self.samples_per_prompt, 1, 64);
        if let Some(t) = self.temperature {
            if !(0.0..=2.0).contains(&t) {
                e.add(&f("temperature"), "Use a number from 0 to 2.");
            }
        }
        if let Some(p) = self.top_p {
            if !(p > 0.0 && p <= 1.0) {
                e.add(&f("top_p"), "Use a number above 0, at most 1.");
            }
        }
        if let Some(m) = self.max_tokens {
            if m < 1 {
                e.add(&f("max_tokens"), "Use 1 or more.");
            }
        }
        check_range_i(e, &f("max_attempts"), self.max_attempts, 1, 20);
        if self.request_timeout_s < 10.0 {
            e.add(&f("request_timeout_s"), "Use at least 10 seconds.");
        }
        if self.pause_after_failures < 0 {
            e.add(&f("pause_after_failures"), "Use 0 or more.");
        }
        // An empty template means none.
        if let Some(t) = &self.user_template {
            if t.trim().is_empty() {
                self.user_template = None;
            } else if !t.contains("{prompt}") {
                e.add(&f("user_template"), "The prompt template must contain {prompt}");
            }
        }
        if self.system_prompt.as_deref().is_some_and(|s| s.is_empty()) {
            self.system_prompt = None;
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct RunCreate {
    /// Default: "<model> on <prompt set name>".
    #[serde(default)]
    pub name: Option<String>,
    pub endpoint_id: i64,
    pub model: String,
    pub prompt_set_id: i64,
    #[serde(default = "d_concurrency")]
    pub concurrency: i64,
    #[serde(default)]
    pub params: RunParams,
}

impl Validate for RunCreate {
    fn validate(&mut self, e: &mut FieldErrors) {
        if let Some(n) = &self.name {
            if n.trim().is_empty() {
                self.name = None;
            } else {
                check_len(e, "name", n, 0, 300);
            }
        }
        check_len(e, "model", &self.model, 1, 500);
        check_range_i(e, "concurrency", self.concurrency, 1, 4096);
        self.params.check("params.", e);
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct RunPatch {
    pub name: Option<String>,
    pub concurrency: Option<i64>,
}

impl Validate for RunPatch {
    fn validate(&mut self, e: &mut FieldErrors) {
        if let Some(n) = &self.name {
            check_len(e, "name", n, 1, 300);
        }
        if let Some(c) = self.concurrency {
            check_range_i(e, "concurrency", c, 1, 4096);
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    /// No new requests; waiting for in-flight ones to finish.
    Pausing,
    Paused,
    Completed,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Pausing => "pausing",
            Self::Paused => "paused",
            Self::Completed => "completed",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "pausing" => Self::Pausing,
            "paused" => Self::Paused,
            _ => Self::Completed,
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct RunOut {
    pub id: i64,
    pub name: String,
    pub endpoint_id: Option<i64>,
    pub endpoint_name: String,
    pub model: String,
    pub prompt_set_id: i64,
    pub prompt_set_name: String,
    pub params: RunParams,
    pub concurrency: i64,
    pub status: RunStatus,
    pub status_reason: Option<String>,
    pub total: i64,
    pub done: i64,
    pub failed: i64,
    pub truncated: i64,
    /// total - done - failed - in flight
    pub pending: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub active_seconds: f64,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// No token yet.
    Waiting,
    Thinking,
    Answering,
}

/// A request in flight, for the run screen.
#[derive(Serialize, Debug, Clone)]
pub struct LiveSequence {
    pub sample_id: i64,
    pub prompt_idx: i64,
    pub sample_idx: i64,
    /// First ~160 chars of the last user message.
    pub prompt_preview: String,
    pub phase: Phase,
    /// Streamed so far (chunk count when the server does not report usage).
    pub tokens: i64,
    /// Last ~240 characters streamed (thinking or answer, per phase).
    pub tail: String,
    pub elapsed_ms: i64,
    pub attempt: i64,
}

#[derive(Serialize, Debug, Clone)]
pub struct MetricPoint {
    pub t: DateTime<Utc>,
    pub tokens_per_s: f64,
    pub samples_per_min: f64,
    pub active: i64,
    pub concurrency: i64,
    pub done: i64,
    pub failed: i64,
}

#[derive(Serialize, Debug, Clone)]
pub struct RunLive {
    pub run: RunOut,
    pub in_flight: i64,
    /// Up to 64, oldest first.
    pub sequences: Vec<LiveSequence>,
    /// Over the last ~10 s.
    pub tokens_per_s: f64,
    /// Over the last ~60 s.
    pub samples_per_min: f64,
    pub eta_seconds: Option<f64>,
    /// Downsampled to at most 240 points.
    pub metrics: Vec<MetricPoint>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SampleStatus {
    Pending,
    Running,
    Done,
    Failed,
}

impl SampleStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "done" => Self::Done,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct SampleOut {
    pub id: i64,
    pub run_id: i64,
    pub prompt_id: i64,
    pub prompt_idx: i64,
    pub sample_idx: i64,
    pub status: SampleStatus,
    pub reasoning: Option<String>,
    pub response: Option<String>,
    pub finish_reason: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub latency_ms: Option<i64>,
    pub ttft_ms: Option<i64>,
    pub attempts: i64,
    pub error: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    /// The prompt as sent (after system_prompt and user_template).
    pub messages: Vec<Message>,
}

/// A row in a run's sample table (text shortened).
#[derive(Serialize, Debug, Clone)]
pub struct SampleSummary {
    pub id: i64,
    pub prompt_idx: i64,
    pub sample_idx: i64,
    pub status: SampleStatus,
    pub prompt_preview: String,
    pub response_preview: Option<String>,
    pub has_reasoning: bool,
    pub finish_reason: Option<String>,
    pub completion_tokens: Option<i64>,
    pub latency_ms: Option<i64>,
    pub error: Option<String>,
}

// ------------------------------------------------------------------------------------ exports

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SftFormat {
    #[default]
    Messages,
    Sharegpt,
    Alpaca,
    PromptCompletion,
    Raw,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PrefFormat {
    #[default]
    Dpo,
    Kto,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    #[default]
    Jsonl,
    Json,
    Parquet,
    Csv,
}

/// Where the thinking trace goes in the exported assistant turn.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningPlacement {
    #[default]
    ThinkTags,
    ReasoningContent,
    Thinking,
    Omit,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExportOptions {
    #[serde(default)]
    pub format: SftFormat,
    #[serde(default)]
    pub file_type: FileType,
    #[serde(default)]
    pub reasoning: ReasoningPlacement,
    #[serde(default = "d_true", deserialize_with = "de_bool")]
    pub include_system: bool,
    #[serde(default = "d_true", deserialize_with = "de_bool")]
    pub exclude_truncated: bool,
    #[serde(default, deserialize_with = "de_bool")]
    pub require_reasoning: bool,
    #[serde(default, deserialize_with = "de_bool")]
    pub first_sample_only: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrefExportOptions {
    #[serde(default)]
    pub format: PrefFormat,
    #[serde(default)]
    pub file_type: FileType,
    #[serde(default)]
    pub reasoning: ReasoningPlacement,
    #[serde(default = "d_true", deserialize_with = "de_bool")]
    pub include_system: bool,
    /// True: prompt/chosen/rejected are message lists (TRL conversational). False: plain strings.
    #[serde(default = "d_true", deserialize_with = "de_bool")]
    pub conversational: bool,
}

/// Accepts JSON booleans and the query-string forms "true"/"false"/"1"/"0".
fn de_bool<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum B {
        B(bool),
        S(String),
    }
    match B::deserialize(d)? {
        B::B(b) => Ok(b),
        B::S(s) => match s.as_str() {
            "true" | "1" | "True" => Ok(true),
            "false" | "0" | "False" => Ok(false),
            other => Err(serde::de::Error::custom(format!("invalid value: \"{other}\", expected true or false"))),
        },
    }
}

no_validation!(ExportOptions, PrefExportOptions);

#[derive(Serialize, Debug, Clone)]
pub struct ExportPreviewOut {
    /// Records the export will contain.
    pub count: i64,
    /// Reason -> count, e.g. {"truncated": 12, "failed": 3}.
    pub skipped: BTreeMap<String, i64>,
    /// The first record exactly as it will be written.
    pub example: Option<Value>,
    pub filename: String,
}

fn d_split() -> String {
    "train".into()
}

#[derive(Deserialize, Debug, Clone)]
pub struct HubPushIn {
    /// "owner/name"
    pub repo_id: String,
    #[serde(default = "d_true")]
    pub private: bool,
    #[serde(default = "d_split")]
    pub split: String,
}

impl HubPushIn {
    pub fn check(&self, prefix: &str, e: &mut FieldErrors) {
        check_len(e, &format!("{prefix}repo_id"), &self.repo_id, 3, 200);
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct RunHubIn {
    #[serde(default = "default_export_options")]
    pub options: ExportOptions,
    pub target: HubPushIn,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PrefHubIn {
    #[serde(default = "default_pref_options")]
    pub options: PrefExportOptions,
    pub target: HubPushIn,
}

fn default_export_options() -> ExportOptions {
    serde_json::from_value(Value::Object(Default::default())).expect("defaults")
}
fn default_pref_options() -> PrefExportOptions {
    serde_json::from_value(Value::Object(Default::default())).expect("defaults")
}

impl Validate for RunHubIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        self.target.check("target.", e);
    }
}
impl Validate for PrefHubIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        self.target.check("target.", e);
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct HubPushOut {
    pub url: String,
    pub count: i64,
}

// ------------------------------------------------------------------------------------ preferences

#[derive(Deserialize, Debug, Clone)]
pub struct PreferenceSetCreate {
    pub name: String,
    pub run_ids: Vec<i64>,
}

impl Validate for PreferenceSetCreate {
    fn validate(&mut self, e: &mut FieldErrors) {
        check_len(e, "name", &self.name, 1, 300);
        if self.run_ids.is_empty() {
            e.add("run_ids", "Pick at least one run.");
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct RunRef {
    pub id: i64,
    pub name: String,
    pub model: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct PreferenceSetOut {
    pub id: i64,
    pub name: String,
    pub prompt_set_id: i64,
    pub prompt_set_name: String,
    pub run_ids: Vec<i64>,
    pub runs: Vec<RunRef>,
    /// Prompts with at least two finished candidates.
    pub prompt_count: i64,
    /// Prompts with a pair.
    pub decided: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Debug, Clone)]
pub struct Candidate {
    pub sample_id: i64,
    pub run_id: i64,
    pub run_name: String,
    pub model: String,
    pub sample_idx: i64,
    pub reasoning: Option<String>,
    pub response: Option<String>,
    pub finish_reason: Option<String>,
    pub completion_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
}

#[derive(Serialize, Debug, Clone)]
pub struct PreferenceItem {
    pub prompt_id: i64,
    pub prompt_idx: i64,
    pub messages: Vec<Message>,
    pub candidates: Vec<Candidate>,
    pub chosen_sample_id: Option<i64>,
    pub rejected_sample_id: Option<i64>,
    pub source: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PairIn {
    pub chosen_sample_id: i64,
    pub rejected_sample_id: i64,
}

// Pair rules (different samples, same prompt, finished, in the set's runs) need the database and
// must answer 404 before 422, so the preferences routes check them in the handler.
no_validation!(PairIn);

/// Prefer every finished sample of one run over another run's, prompt by prompt.
#[derive(Deserialize, Debug, Clone)]
pub struct AutoPairIn {
    pub chosen_run_id: i64,
    pub rejected_run_id: i64,
    /// Replace pairs already made by hand.
    #[serde(default)]
    pub overwrite: bool,
}

no_validation!(AutoPairIn);

#[derive(Serialize, Debug, Clone)]
pub struct AutoPairOut {
    pub created: i64,
    pub skipped: i64,
}

// ------------------------------------------------------------------------------------ settings

#[derive(Serialize, Debug, Clone)]
pub struct SettingsOut {
    pub hf_token_set: bool,
    pub hf_token_hint: Option<String>,
    pub hf_token_from_env: bool,
    pub data_dir: String,
    pub version: String,
    /// v2: "docker" | "desktop" | "native"
    pub mode: &'static str,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct SettingsIn {
    /// None leaves it unchanged; "" removes it.
    #[serde(default)]
    pub hf_token: Option<String>,
}

no_validation!(SettingsIn);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::parse_json;

    #[test]
    fn run_params_defaults_match_python() {
        let p = RunParams::default();
        assert_eq!(p.samples_per_prompt, 1);
        assert_eq!(p.temperature, Some(0.7));
        assert_eq!(p.max_tokens, Some(8192));
        assert!(p.stream);
        assert_eq!(p.think_open, "<think>");
        assert_eq!(p.max_attempts, 4);
        assert_eq!(p.pause_after_failures, 10);
        // An explicit null clears a defaulted field.
        let p: RunParams = serde_json::from_str(r#"{"temperature": null}"#).unwrap();
        assert_eq!(p.temperature, None);
    }

    #[test]
    fn endpoint_validation_reports_fields() {
        let mut v: EndpointIn = parse_json(br#"{"name": "x", "base_url": "localhost:8000"}"#).unwrap();
        let mut e = FieldErrors::new();
        v.validate(&mut e);
        let err = e.finish().unwrap_err();
        assert_eq!(err.status.as_u16(), 422);
        assert_eq!(err.fields["base_url"], "Base URL must start with http:// or https://");
    }

    #[test]
    fn missing_field_has_path() {
        let err = parse_json::<RunCreate>(br#"{"endpoint_id": 1, "prompt_set_id": 2}"#).unwrap_err();
        assert_eq!(err.fields.get("model").map(String::as_str), Some("Field required."));
    }

    #[test]
    fn import_source_tagged() {
        let s: ImportSource =
            serde_json::from_str(r#"{"kind":"huggingface","dataset":"openai/gsm8k","all_splits":true}"#).unwrap();
        assert!(matches!(s, ImportSource::Huggingface { all_splits: true, .. }));
    }

    #[test]
    fn export_options_from_query_strings() {
        let o: ExportOptions = serde_json::from_value(serde_json::json!({"exclude_truncated": "false"})).unwrap();
        assert!(!o.exclude_truncated);
    }
}
