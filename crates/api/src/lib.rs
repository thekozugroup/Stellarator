//! The Stellarator API contract: request and response bodies, as JSON types with no server
//! dependencies (no sqlx, tokio or axum), so the server crates and every client, the nsh app's
//! web build included, share one definition. `stellarator_core::schemas` re-exports them with
//! their validation (the `Validate` impls). Field names, defaults and validation messages match the
//! Python backend this replaced, except where marked "v2" (background imports, prompts-only
//! imports, presets).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `{"role": "system" | "user" | "assistant", "content": str}`. Kept as JSON so unusual content
/// (lists of parts) survives.
pub type Message = Value;
pub type JsonMap = serde_json::Map<String, Value>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
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

#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
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

#[derive(Deserialize, Debug, Clone, Default, Serialize, PartialEq)]
pub struct EndpointPatch {
    pub name: Option<String>,
    pub base_url: Option<String>,
    /// None (or absent) leaves the key unchanged; "" removes it.
    pub api_key: Option<String>,
    pub location: Option<Location>,
    pub default_concurrency: Option<i64>,
    pub headers: Option<BTreeMap<String, String>>,
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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

#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
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

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
pub struct EndpointTestOut {
    pub ok: bool,
    pub models: Vec<String>,
    pub latency_ms: Option<i64>,
    pub error: Option<String>,
}

/// v2: a starting point for the Add endpoint form (`GET /api/endpoint-presets`).
#[derive(Serialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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

#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
pub struct PromptSetPatch {
    pub name: String,
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
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

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
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

/// Same as an empty JSON object: dedupe on, everything else automatic. (A derived Default would
/// turn dedupe off for requests that leave `mapping` out.)
impl Default for ImportMapping {
    fn default() -> Self {
        serde_json::from_value(Value::Object(Default::default())).expect("defaults")
    }
}

#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
pub struct ImportPreviewIn {
    pub source: ImportSource,
    #[serde(default)]
    pub mapping: ImportMapping,
}

#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
pub struct ImportCommitIn {
    pub source: ImportSource,
    #[serde(default)]
    pub mapping: ImportMapping,
    pub name: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnKind {
    Text,
    Messages,
    Number,
    Other,
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
pub struct ColumnInfo {
    pub name: String,
    pub kind: ColumnKind,
    /// First non-empty value, shortened.
    pub example: Option<String>,
    /// v2: true when the column looks like model output (response, completion, reasoning...). Such
    /// columns are never picked automatically as the prompt.
    pub looks_like_output: bool,
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
pub struct HfSplit {
    pub config: String,
    pub split: String,
    pub num_rows: Option<i64>,
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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
    /// MCP tools for the model to call. None (the default): no tools. Left out of the JSON when
    /// None, so the parameters of runs without tools stay as they were.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<RunTools>,
}

fn d_max_rounds() -> i64 {
    8
}
fn d_tool_timeout() -> f64 {
    60.0
}
fn d_max_result_chars() -> i64 {
    20_000
}

/// Whether the model must call a tool.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    /// The model decides (`"tool_choice": "auto"`).
    #[default]
    Auto,
    /// The first request of each sample must call a tool (`"required"`); later requests of the
    /// sample use `auto`, so the model can answer once it has the results.
    Required,
}

/// One MCP server picked for a run.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RunToolServer {
    pub server_id: i64,
    /// The tool names to offer, as the server names them. None: every tool of the server.
    #[serde(default)]
    pub tools: Option<Vec<String>>,
}

/// The tools section of a run's parameters (`params.tools`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RunTools {
    pub servers: Vec<RunToolServer>,
    /// How many rounds of tool calls one sample may make (1 to 32). A round runs every call of one
    /// answer, then sends the results back; the model gets at most `max_rounds + 1` requests. An
    /// answer that still calls tools after the last round ends the sample as truncated, with finish
    /// reason `tool_rounds`. In code mode a round runs the answer's code tool calls.
    #[serde(default = "d_max_rounds")]
    pub max_rounds: i64,
    /// Seconds one tool call may take (1 to 3600). A call that takes longer returns an error result
    /// to the model.
    #[serde(default = "d_tool_timeout")]
    pub tool_timeout_s: f64,
    /// Longer tool results are cut to this many characters, with a marker (1000 to 1,000,000).
    /// Code mode: the code gets whole results; this only caps the copy of each inner call's result
    /// stored in the trace (the model's budget is `code.max_result_tokens`).
    #[serde(default = "d_max_result_chars")]
    pub max_result_chars: i64,
    #[serde(default)]
    pub tool_choice: ToolChoice,
    /// How the model gets the tools: `direct` (one function per MCP tool) or `code` (Cloudflare's
    /// Code Mode: one tool that runs JavaScript calling the MCP tools). Missing in runs made
    /// before code mode existed, which are direct.
    #[serde(default)]
    pub mode: ToolMode,
    /// Code mode settings; null (or absent) means Cloudflare's defaults in the `code_tool` format.
    /// Ignored in direct mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<CodeModeOptions>,
}

impl Default for RunTools {
    fn default() -> Self {
        serde_json::from_value(serde_json::json!({"servers": []})).expect("defaults")
    }
}

/// How a run gives the model its MCP tools (`params.tools.mode`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ToolMode {
    /// One function tool per MCP tool; the model calls them and gets each result back.
    #[default]
    Direct,
    /// Code Mode as Cloudflare's `@cloudflare/codemode` defines it: the model gets one tool whose
    /// input is `{"code": string}`, a JavaScript async arrow function. The code calls the MCP tools
    /// through generated typed namespaces in a sandbox, and only its result (and console output)
    /// goes back to the model.
    Code,
}

/// Which of Cloudflare's two Code Mode setups a run reproduces (`params.tools.code.format`). They
/// differ in what the model reads, what its code can call and what it gets back.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CodeFormat {
    /// `createCodeTool({tools, executor})` from `@cloudflare/codemode/ai`
    /// (developers.cloudflare.com/agents/tools/codemode/ai-sdk/). The tool's description holds
    /// TypeScript declarations of every tool; the result is `{"result": ..., "logs": [...]}`, an
    /// error `Error: Code execution failed: ...`.
    #[default]
    CodeTool,
    /// `createCodemodeRuntime({ctx, executor, connectors}).tool()` with one `McpConnector` per
    /// server (developers.cloudflare.com/agents/tools/codemode/mcp/ and durable-runtime/). The
    /// description lists the connectors and a workflow; the code finds tools with
    /// `codemode.search(query)` and reads their types with `codemode.describe(target)`, then calls
    /// `<connector>.<method>(args)`; the result is
    /// `{"status": "completed", "executionId": ..., "result": ..., "logs": [...]}`, an error
    /// `{"status": "error", "executionId": ..., "error": ...}`. Approvals, saved snippets and
    /// rollback are not configured: no call pauses, `codemode.run` finds no snippet.
    Runtime,
}

/// Where code mode puts the MCP tools (`params.tools.code.namespaces`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CodeNamespaces {
    /// One namespace per server, named after it (`github.list_issues({...})`), as Cloudflare's
    /// `McpConnector` and `createCodeTool` with one named provider per server do.
    #[default]
    Servers,
    /// Every tool under `codemode` (`codemode.list_issues({...})`), as `createCodeTool` does with
    /// one tool set. Tools of the same name on two servers become `<server>__<tool>`. Only in the
    /// `code_tool` format: in the `runtime` format `codemode` is the platform's own namespace.
    Codemode,
}

/// The code tool's default name. Cloudflare's README registers the tool as `tools: { codemode }`.
pub const CODE_TOOL_NAME: &str = "codemode";

/// Cloudflare's default description of the code tool (`DEFAULT_DESCRIPTION` in
/// `packages/codemode/src/shared.ts`). `{{types}}` is replaced by the generated declarations
/// (the first occurrence, with JavaScript's `String.replace` rules).
pub const CODE_TOOL_DESCRIPTION: &str = "Execute code to achieve a goal.

Available:
{{types}}

Write an async arrow function in JavaScript that returns the result.
Do NOT use TypeScript syntax \u{2014} no type annotations, interfaces, or generics.
Do NOT define named functions then call them \u{2014} just write the arrow function body directly.

Example: async () => { const r = await codemode.searchWeb({ query: \"test\" }); return r; }";

/// The Cloudflare release whose formats code mode reproduces.
pub const CODE_MODE_FORMAT: &str = "@cloudflare/codemode 0.5.6";

/// `truncateResult`'s default budget, in tokens of 4 characters: what `max_result_tokens: null`
/// means in the `code_tool` format, and the budget of the console output in the `runtime` format.
pub const CODE_RESULT_TOKENS: i64 = 6000;

fn d_code_tool_name() -> String {
    CODE_TOOL_NAME.into()
}
fn d_code_timeout() -> f64 {
    60.0
}
fn d_code_memory_mb() -> i64 {
    128
}

/// Code mode settings (`params.tools.code`). Every default is Cloudflare's, except the memory
/// limit, which Cloudflare leaves to the Workers runtime (128 MB per isolate), and the result
/// budget of the `code_tool` format (see `max_result_tokens`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CodeModeOptions {
    /// Which Cloudflare setup to reproduce: `code_tool` (the default) or `runtime`.
    #[serde(default)]
    pub format: CodeFormat,
    /// The code tool's name (1 to 64 letters, digits, `_` or `-`). Cloudflare's docs register both
    /// setups' tools as `codemode`.
    #[serde(default = "d_code_tool_name")]
    pub tool_name: String,
    /// `code_tool`: the tool's description template, where `{{types}}` stands for the generated
    /// declarations; null: Cloudflare's default, [`CODE_TOOL_DESCRIPTION`]. `runtime`: the whole
    /// description, in place of the one the runtime writes (the `description` option of
    /// `runtime.tool()`); null: the runtime's own, which lists the connectors.
    #[serde(default)]
    pub description: Option<String>,
    /// `code_tool` only; the `runtime` format always has one connector per server (`servers`).
    #[serde(default)]
    pub namespaces: CodeNamespaces,
    /// Seconds one execution may take, its tool calls included (1 to 3600; Cloudflare: 60). Each
    /// tool call is also limited by `tool_timeout_s`.
    #[serde(default = "d_code_timeout")]
    pub timeout_s: f64,
    /// The budget of the result the model sees, in tokens of 4 characters; a larger result is
    /// shrunk the way Cloudflare's `truncateResult` does it. 0: no truncation, else 100 to
    /// 1,000,000. Null: the format's default.
    ///
    /// - `code_tool`: null means [`CODE_RESULT_TOKENS`] (6000), and the console output gets the
    ///   same budget. `createCodeTool` itself never truncates: 0 is what it sends.
    /// - `runtime`: null means no truncation, as the runtime without `transformResult`; a number
    ///   is `transformResult: (r) => truncateResult(r, {maxTokens})`. The console output is always
    ///   bounded by `truncateResult`'s default (6000 tokens), as the runtime's tool does it.
    #[serde(default)]
    pub max_result_tokens: Option<i64>,
    /// The same budget in characters; overrides `max_result_tokens * 4` when set (1,000 to
    /// 4,000,000), as `truncateResult`'s `maxChars` does.
    #[serde(default)]
    pub max_result_chars: Option<i64>,
    /// Memory one execution may use, in MB (8 to 1024).
    #[serde(default = "d_code_memory_mb")]
    pub memory_limit_mb: i64,
    /// `runtime` only: a short hint per connector, shown next to its name in the tool's
    /// description (`- \`github\` — <hint>`), as the `connectorHints` option of `runtime.tool()`.
    /// Keys are the servers' names (as given or as their namespaces); at most 2,000 characters
    /// each. Empty by default, as Cloudflare's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub connector_hints: BTreeMap<String, String>,
    /// `runtime` only: whether `codemode.describe("<connector>")` carries the `instructions` the
    /// MCP server sent when the session was opened. True by default: Cloudflare's `McpConnector`
    /// includes them whenever the MCP connection has them, which the Agents SDK's connections do.
    #[serde(default = "d_true", skip_serializing_if = "is_true")]
    pub server_instructions: bool,
}

fn is_true(v: &bool) -> bool {
    *v
}

impl CodeModeOptions {
    /// `truncateResult`'s budget for the result, as (characters, tokens); None: no truncation.
    pub fn result_budget(&self) -> Option<(i64, i64)> {
        let tokens = match (self.max_result_tokens, self.format) {
            (Some(t), _) => t,
            (None, CodeFormat::CodeTool) => CODE_RESULT_TOKENS,
            (None, CodeFormat::Runtime) => 0,
        };
        (tokens > 0).then(|| (self.max_result_chars.unwrap_or(tokens * 4), tokens))
    }

    /// `truncateResult`'s budget for the console output, as (characters, tokens).
    pub fn logs_budget(&self) -> Option<(i64, i64)> {
        match self.format {
            CodeFormat::CodeTool => self.result_budget(),
            CodeFormat::Runtime => Some((CODE_RESULT_TOKENS * 4, CODE_RESULT_TOKENS)),
        }
    }
}

impl Default for CodeModeOptions {
    fn default() -> Self {
        serde_json::from_value(Value::Object(Default::default())).expect("defaults")
    }
}

/// One tool as a run sends it: its name in the request and its name on the MCP server (they differ
/// when two selected servers have a tool of the same name: both become `<server>__<tool>`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolSnapshotTool {
    /// Direct mode: the function name sent. Code mode: how the code calls it,
    /// `<namespace>.<method>`.
    pub name: String,
    pub server_tool: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolSnapshotServer {
    pub server_id: i64,
    /// The server's name when the run was created.
    pub name: String,
    pub transport: McpTransport,
    pub tools: Vec<ToolSnapshotTool>,
}

/// The tools a run sends, fixed when the run is created (`RunOut.tools`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolSnapshot {
    pub servers: Vec<ToolSnapshotServer>,
    /// Exactly the request's `tools` array: OpenAI function definitions,
    /// `{"type": "function", "function": {"name", "description", "parameters"}}`. In code mode,
    /// the one code tool, its description holding the generated types.
    pub definitions: Vec<Value>,
    /// The mode the run uses (absent in snapshots made before code mode: direct).
    #[serde(default)]
    pub mode: ToolMode,
    /// Code mode: the code tool and how its namespace methods map to MCP tools. Absent in direct
    /// mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<CodeToolSnapshot>,
}

/// The code tool of a code mode run, as sent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct CodeToolSnapshot {
    /// The tool's name (also `definitions[0].function.name`).
    pub tool_name: String,
    /// The generated TypeScript declarations, one block per namespace (input and output types,
    /// then `declare const <namespace>: {...}`). `code_tool`: what replaced `{{types}}` in the
    /// description. `runtime`: what `codemode.describe("<connector>")` returns for each connector,
    /// joined by blank lines; the description does not hold them.
    pub types: String,
    /// The namespaces the code sees, in the order they are declared.
    pub namespaces: Vec<CodeNamespace>,
    /// The Cloudflare release whose formats were followed, [`CODE_MODE_FORMAT`].
    pub format: String,
    /// Which setup the run reproduces (`params.tools.code.format`). Absent in snapshots made
    /// before the `runtime` format: `code_tool`.
    #[serde(default)]
    pub kind: CodeFormat,
    /// `runtime`: the connectors as the runtime describes them, in order; what
    /// `codemode.search` and `codemode.describe` read. Absent in the `code_tool` format.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connectors: Vec<CodeConnector>,
}

/// A connector of the `runtime` format: one MCP server as Cloudflare's `McpConnector` describes it
/// (`connector.describe()`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CodeConnector {
    /// The connector's name, the global the code calls (also its `CodeNamespace.name`).
    pub name: String,
    /// The MCP server's `instructions`, shown by `codemode.describe("<name>")`; null when the
    /// server sent none or `server_instructions` is off.
    pub instructions: Option<String>,
    /// The hint shown next to the name in the description (`connector_hints`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// The methods, in the server's order.
    pub tools: Vec<CodeConnectorTool>,
}

/// A method of a connector, as `codemode.search` and `codemode.describe` see it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CodeConnectorTool {
    /// The method's name ([`CodeMethod::name`]).
    pub name: String,
    pub description: Option<String>,
    /// JSON Schema of the arguments, as the MCP server declares it.
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
}

/// A global object of the sandbox, with one async method per MCP tool.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CodeNamespace {
    /// The global's name, a JavaScript identifier: the server's name through Cloudflare's
    /// `sanitizeToolName` (`my-server` becomes `my_server`), or `codemode`.
    pub name: String,
    pub methods: Vec<CodeMethod>,
}

/// `<namespace>.<name>(input)` calls `tool` on the MCP server `server`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CodeMethod {
    /// The tool's name through `sanitizeToolName` (`list-issues` becomes `list_issues`, `delete`
    /// becomes `delete_`).
    pub name: String,
    pub server_id: i64,
    /// The server's name when the run was created.
    pub server: String,
    /// The tool's name on the MCP server.
    pub tool: String,
}

impl Default for RunParams {
    fn default() -> Self {
        serde_json::from_value(Value::Object(Default::default())).expect("defaults")
    }
}

#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
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

#[derive(Deserialize, Debug, Clone, Default, Serialize, PartialEq)]
pub struct RunPatch {
    pub name: Option<String>,
    pub concurrency: Option<i64>,
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

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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
    /// The tool definitions the run sends and the servers they come from; null for runs without
    /// tools.
    #[serde(default)]
    pub tools: Option<ToolSnapshot>,
    /// Tool calls run so far, over all finished samples. In code mode: the code tool's calls
    /// (executions).
    #[serde(default)]
    pub tool_calls: i64,
    /// Code mode: the MCP tool calls the executions made, over all finished samples. 0 in direct
    /// mode.
    #[serde(default)]
    pub inner_tool_calls: i64,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// No token yet.
    Waiting,
    Thinking,
    Answering,
    /// Tools are running (`LiveSequence.tools` names them).
    Tool,
    /// Code mode: the model's code is running (`LiveSequence.code` has its tool calls).
    Code,
}

/// A request in flight, for the run screen.
#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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
    /// The request of the sample in flight: 1, then one more after each round of tool calls.
    #[serde(default = "d_one")]
    pub round: i64,
    /// While `phase` is `tool`: the tools running (as the model named them), else empty. While
    /// `phase` is `code`: the code tool's name.
    #[serde(default)]
    pub tools: Vec<String>,
    /// While `phase` is `code`, the executions' tool calls; else null.
    #[serde(default)]
    pub code: Option<LiveCode>,
}

/// The tool calls of the code running for a sample (all its executions of this round together,
/// when the model called the code tool more than once in one answer).
#[derive(Serialize, Debug, Clone, Deserialize, PartialEq, Default)]
pub struct LiveCode {
    /// Tool calls the code started so far, finished or not.
    pub calls: i64,
    /// The tool calls in progress, as the code called them (`github.list_issues`), oldest first.
    pub running: Vec<String>,
}

fn d_one() -> i64 {
    1
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
pub struct MetricPoint {
    pub t: DateTime<Utc>,
    pub tokens_per_s: f64,
    pub samples_per_min: f64,
    pub active: i64,
    pub concurrency: i64,
    pub done: i64,
    pub failed: i64,
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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
    /// Runs with tools: every message after the prompt, in order (assistant turns with their tool
    /// calls, tool results, the final answer). `reasoning` and `response` above are the last
    /// assistant turn's. Null for runs without tools and for samples not finished.
    #[serde(default)]
    pub trace: Option<Vec<TraceMessage>>,
    /// Tool calls run for this sample. In code mode: the code tool's calls (executions).
    #[serde(default)]
    pub tool_calls: i64,
    /// Code mode: the MCP tool calls this sample's code made. 0 in direct mode.
    #[serde(default)]
    pub inner_tool_calls: i64,
}

/// A tool call the model made: `arguments` is the JSON text exactly as the model wrote it (it may
/// not parse).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// One assistant turn of a trace.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TraceAssistant {
    pub reasoning: Option<String>,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub latency_ms: i64,
}

/// One tool result of a trace, as sent back to the model.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TraceTool {
    pub tool_call_id: String,
    /// The tool's name as the model called it.
    pub name: String,
    /// The MCP server that ran it ("" when no server has a tool of that name, and for the code
    /// tool).
    pub server: String,
    /// The text sent to the model: the tool's text parts joined, a placeholder for each image or
    /// other part, or the error. Code mode: the code tool's result as Cloudflare's Code Mode sends
    /// it through the AI SDK. `code_tool` format: `{"result": ..., "logs": [...]}` as JSON, or
    /// `Error: Code execution failed: ...` (with the console output). `runtime` format:
    /// `{"status": "completed", "executionId": ..., "result": ..., "logs": [...]}`, or
    /// `{"status": "error", "executionId": ..., "error": ...}`.
    pub content: String,
    /// The tool reported an error, the arguments were not valid JSON, the tool is unknown, or the
    /// call timed out. The error text is still sent to the model. Code mode: the execution failed
    /// (or the input was not `{"code": string}`).
    pub is_error: bool,
    /// The result was longer than the run's `max_result_chars` and was cut. Code mode: the result
    /// or the console output was over `code.max_result_tokens` and was shrunk.
    pub truncated: bool,
    pub latency_ms: i64,
    /// Code mode: what happened inside the execution, for viewing and audit. The model saw only
    /// `content`; exports other than raw leave this out. Absent in direct mode, and when no code
    /// ran (the input was not `{"code": string}`, or the model called an unknown tool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<CodeExecution>,
}

/// How a code execution ended.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeStatus {
    /// The code returned (its value, possibly undefined, went back to the model).
    Ok,
    /// The code threw (an uncaught exception, also from a tool call).
    Error,
    /// The code is not valid JavaScript.
    SyntaxError,
    /// The execution ran past `code.timeout_s` (also in a tight loop: it is interrupted).
    Timeout,
    /// The execution ran out of its `code.memory_limit_mb`.
    MemoryLimit,
}

/// One code execution: the code tool's call, from the model's code to what it returned.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CodeExecution {
    /// The code as it ran: the model's `code` after Cloudflare's `normalizeCode` (fences stripped,
    /// wrapped in an async arrow function when it was not one).
    pub code: String,
    pub status: CodeStatus,
    /// The MCP tool calls the code made, in the order it made them.
    pub calls: Vec<InnerToolCall>,
    /// The console output, in full: `console.log` lines as is, `console.warn` with `[warn] `,
    /// `console.error` with `[error] ` (as Cloudflare's executor captures them).
    pub logs: Vec<String>,
    /// The value the code returned, as JSON text, before any truncation; null when it returned
    /// undefined or failed.
    pub result: Option<String>,
    /// Why it failed: the error's message (in the `code_tool` format, the text after
    /// `Code execution failed: ` in what the model saw); null when it returned.
    pub error: Option<String>,
    pub latency_ms: i64,
}

/// A tool call the model's code made inside an execution.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct InnerToolCall {
    /// As the code called it: `<namespace>.<method>`.
    pub method: String,
    /// The MCP server and the tool's name there; both "" when the namespace has no such method.
    pub server: String,
    pub tool: String,
    /// The first argument the code passed (the tool's arguments), as JSON; null when none.
    pub arguments: Value,
    /// What the code got back: a string as is, any other value as JSON text. For an error, the
    /// message of the exception the call threw in the code.
    pub result: String,
    /// The call threw in the code: the tool reported an error, failed or timed out, the method does
    /// not exist, or the execution ended before the call finished.
    pub is_error: bool,
    /// `result` is stored cut to the run's `max_result_chars` (the code got all of it).
    pub truncated: bool,
    pub latency_ms: i64,
}

/// A message of a sample's trace, tagged by `role`: `{"role": "assistant", ...TraceAssistant}` or
/// `{"role": "tool", ...TraceTool}`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum TraceMessage {
    Assistant(TraceAssistant),
    Tool(TraceTool),
}

/// A row in a run's sample table (text shortened).
#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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
    /// Tool calls run for this sample (code mode: executions).
    #[serde(default)]
    pub tool_calls: i64,
    /// Code mode: the MCP tool calls this sample's code made.
    #[serde(default)]
    pub inner_tool_calls: i64,
}

// ------------------------------------------------------------------------------------ MCP servers

/// How Stellarator talks to an MCP server: a child process on stdin/stdout, or streamable HTTP.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
}

impl McpTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
        }
    }
    pub fn parse(s: &str) -> Self {
        if s == "http" {
            Self::Http
        } else {
            Self::Stdio
        }
    }
}

/// `POST /api/mcp-servers`. Stdio servers need `command`; HTTP servers need `url`.
#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
pub struct McpServerIn {
    /// Letters, digits, `_` and `-`; 1 to 64 characters; unique (ignoring case).
    pub name: String,
    pub transport: McpTransport,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables for the process (stdio). Secret: never returned.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
    /// HTTP headers sent with every request (http), e.g. Authorization. Secret: never returned.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default = "d_true")]
    pub enabled: bool,
}

/// `PATCH /api/mcp-servers/{id}`. Absent fields stay as they are.
///
/// `env` and `headers`, when given, are the complete new set of names: a name with a string value
/// gets that value, a name with `null` keeps its saved value (422 if it has none), and a saved name
/// left out is removed.
#[derive(Deserialize, Debug, Clone, Default, Serialize, PartialEq)]
pub struct McpServerPatch {
    pub name: Option<String>,
    pub transport: Option<McpTransport>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<BTreeMap<String, Option<String>>>,
    pub url: Option<String>,
    pub headers: Option<BTreeMap<String, Option<String>>>,
    pub enabled: Option<bool>,
}

/// A secret's name and the end of its value, e.g. `{"name": "GITHUB_TOKEN", "hint": "…a9f2"}`
/// (`"…"` for values of 8 characters or fewer, null for an empty value).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SecretHint {
    pub name: String,
    pub hint: Option<String>,
}

/// The outcome of the last check (`POST /api/mcp-servers/{id}/check`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum McpStatus {
    /// Never checked, or changed since the last check.
    #[default]
    Unchecked,
    Ok,
    Error,
}

impl McpStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unchecked => "unchecked",
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "ok" => Self::Ok,
            "error" => Self::Error,
            _ => Self::Unchecked,
        }
    }
}

/// A tool as the MCP server lists it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    /// JSON Schema of the arguments.
    pub input_schema: Value,
    /// JSON Schema of the structured result, when the server declares one (code mode types the
    /// method's result with it). Absent otherwise, and in lists saved before code mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct McpServerOut {
    pub id: i64,
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    /// Names and hints only; values are never returned.
    pub env: Vec<SecretHint>,
    pub url: Option<String>,
    /// Names and hints only; values are never returned.
    pub headers: Vec<SecretHint>,
    pub enabled: bool,
    pub status: McpStatus,
    /// Why the last check failed.
    pub error: Option<String>,
    pub checked_at: Option<DateTime<Utc>>,
    /// The name and version the server reported at the last check, e.g. "github-mcp 1.2.0".
    pub server_info: Option<String>,
    /// The tools found by the last successful check.
    pub tools: Vec<McpTool>,
    /// Null when this server can be used here; otherwise why not (a stdio server in server mode
    /// without STELLARATOR_ALLOW_STDIO_MCP=1).
    pub blocked: Option<String>,
    /// Runs that use this server.
    pub run_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// `POST /api/mcp-servers/import`: pasted JSON in any common shape: `{"mcpServers": {...}}`
/// (Claude Desktop, Cursor, Windsurf), `{"servers": {...}}` (VS Code), `{"name": {...}}`, or one
/// server object.
#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
pub struct McpImportIn {
    pub text: String,
    /// Check each created server before answering (connect and list its tools).
    #[serde(default)]
    pub check: bool,
}

/// A pasted entry and what happened to it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct McpImportNote {
    /// The entry's name in the pasted JSON ("" for a single unnamed object).
    pub entry: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct McpImportOut {
    pub created: Vec<McpServerOut>,
    /// Entries not imported, with the reason.
    pub errors: Vec<McpImportNote>,
    /// Entries imported with a change: renamed because the name was taken, or a legacy SSE URL
    /// saved as streamable HTTP.
    pub notes: Vec<McpImportNote>,
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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
            other => Err(serde::de::Error::custom(format!(
                "invalid value: \"{other}\", expected true or false"
            ))),
        },
    }
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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

#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
pub struct HubPushIn {
    /// "owner/name"
    pub repo_id: String,
    #[serde(default = "d_true")]
    pub private: bool,
    #[serde(default = "d_split")]
    pub split: String,
}

#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
pub struct RunHubIn {
    #[serde(default = "default_export_options")]
    pub options: ExportOptions,
    pub target: HubPushIn,
}

#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
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

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
pub struct HubPushOut {
    pub url: String,
    pub count: i64,
}

// ------------------------------------------------------------------------------------ preferences

#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
pub struct PreferenceSetCreate {
    pub name: String,
    pub run_ids: Vec<i64>,
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
pub struct RunRef {
    pub id: i64,
    pub name: String,
    pub model: String,
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
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
    /// Tool calls run for this sample (code mode: executions).
    #[serde(default)]
    pub tool_calls: i64,
    /// Code mode: the MCP tool calls this sample's code made.
    #[serde(default)]
    pub inner_tool_calls: i64,
    /// The sample's trace (runs with tools), as in `SampleOut.trace`.
    #[serde(default)]
    pub trace: Option<Vec<TraceMessage>>,
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
pub struct PreferenceItem {
    pub prompt_id: i64,
    pub prompt_idx: i64,
    pub messages: Vec<Message>,
    pub candidates: Vec<Candidate>,
    pub chosen_sample_id: Option<i64>,
    pub rejected_sample_id: Option<i64>,
    pub source: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
pub struct PairIn {
    pub chosen_sample_id: i64,
    pub rejected_sample_id: i64,
}

// Pair rules (different samples, same prompt, finished, in the set's runs) need the database and
// must answer 404 before 422, so the preferences routes check them in the handler.

/// Prefer every finished sample of one run over another run's, prompt by prompt.
#[derive(Deserialize, Debug, Clone, Serialize, PartialEq)]
pub struct AutoPairIn {
    pub chosen_run_id: i64,
    pub rejected_run_id: i64,
    /// Replace pairs already made by hand.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
pub struct AutoPairOut {
    pub created: i64,
    pub skipped: i64,
}

// ------------------------------------------------------------------------------------ settings

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SettingsOut {
    pub hf_token_set: bool,
    pub hf_token_hint: Option<String>,
    pub hf_token_from_env: bool,
    pub data_dir: String,
    pub version: String,
    /// v2: "docker" | "desktop" | "native"
    pub mode: &'static str,
    /// Stdio MCP servers can be added and run: always in the desktop app; in server mode only with
    /// STELLARATOR_ALLOW_STDIO_MCP=1.
    pub stdio_mcp_allowed: bool,
}

#[derive(Deserialize, Debug, Clone, Default, Serialize, PartialEq)]
pub struct SettingsIn {
    /// None leaves it unchanged; "" removes it.
    #[serde(default)]
    pub hf_token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_saved_before_code_mode_are_direct() {
        let old: RunTools = serde_json::from_value(serde_json::json!({
            "servers": [{"server_id": 1, "tools": null}], "max_rounds": 8, "tool_timeout_s": 60.0,
            "max_result_chars": 20000, "tool_choice": "auto"
        }))
        .unwrap();
        assert_eq!((old.mode, old.code.as_ref()), (ToolMode::Direct, None));
        let saved = serde_json::to_value(&old).unwrap();
        assert_eq!(saved["mode"], "direct");
        assert!(saved.get("code").is_none());
        let snap: ToolSnapshot = serde_json::from_value(serde_json::json!({"servers": [], "definitions": []})).unwrap();
        assert_eq!((snap.mode, snap.code), (ToolMode::Direct, None));
    }

    #[test]
    fn code_options_default_to_cloudflare() {
        let o = CodeModeOptions::default();
        assert_eq!(o.tool_name, "codemode");
        assert_eq!((o.description.as_deref(), o.namespaces), (None, CodeNamespaces::Servers));
        assert_eq!((o.format, o.max_result_tokens, o.server_instructions), (CodeFormat::CodeTool, None, true));
        assert!(o.connector_hints.is_empty());
        // The result budget follows the format: 6000 tokens for the code tool, none for the runtime.
        assert_eq!((o.result_budget(), o.logs_budget()), (Some((24000, 6000)), Some((24000, 6000))));
        let r: CodeModeOptions = serde_json::from_value(serde_json::json!({"format": "runtime"})).unwrap();
        assert_eq!((r.format, r.result_budget(), r.logs_budget()), (CodeFormat::Runtime, None, Some((24000, 6000))));
        let r: CodeModeOptions =
            serde_json::from_value(serde_json::json!({"format": "runtime", "max_result_tokens": 500, "max_result_chars": 1000})).unwrap();
        assert_eq!((r.result_budget(), r.logs_budget()), (Some((1000, 500)), Some((24000, 6000))));
        let zero: CodeModeOptions = serde_json::from_value(serde_json::json!({"max_result_tokens": 0})).unwrap();
        assert_eq!(zero.result_budget(), None);
        // Defaults stay out of the stored settings; a snapshot made before the runtime format reads as code_tool.
        let saved = serde_json::to_value(&o).unwrap();
        assert!(saved.get("connector_hints").is_none() && saved.get("server_instructions").is_none());
        let old: CodeToolSnapshot = serde_json::from_value(serde_json::json!({
            "tool_name": "codemode", "types": "", "namespaces": [], "format": "@cloudflare/codemode 0.5.6"})).unwrap();
        assert_eq!((old.kind, old.connectors.len()), (CodeFormat::CodeTool, 0));
        assert_eq!((o.timeout_s, o.max_result_tokens, o.max_result_chars, o.memory_limit_mb), (60.0, None, None, 128));
        let t: RunTools = serde_json::from_value(serde_json::json!({"servers": [], "mode": "code"})).unwrap();
        assert_eq!((t.mode, t.code), (ToolMode::Code, None));
    }
}
