// Mirrors backend/stellarator/schemas.py. Keep the two in step.

export type Role = "system" | "user" | "assistant"
export interface Message {
  role: Role
  content: string
}

export interface Page<T> {
  items: T[]
  total: number
  offset: number
  limit: number
}

// ------------------------------------------------------------------ endpoints

export type EndpointLocation = "local" | "cloud"

export interface Endpoint {
  id: number
  name: string
  base_url: string
  has_api_key: boolean
  api_key_hint: string | null
  location: EndpointLocation
  default_concurrency: number
  headers: Record<string, string>
  models: string[]
  last_checked_at: string | null
  last_ok: boolean | null
  last_error: string | null
  created_at: string
  run_count: number
}

export interface EndpointIn {
  name: string
  base_url: string
  api_key?: string | null
  location: EndpointLocation
  default_concurrency: number
  headers?: Record<string, string>
}

export interface EndpointTestIn {
  base_url: string
  api_key?: string | null
  headers?: Record<string, string>
  endpoint_id?: number | null
}

export interface EndpointTestOut {
  ok: boolean
  models: string[]
  latency_ms: number | null
  error: string | null
}

export interface EndpointPreset {
  id: "openrouter" | "lmstudio" | "ollama" | "vllm" | "sglang" | "llamacpp" | "openai"
  name: string
  base_url: string
  location: EndpointLocation
  default_concurrency: number
  needs_api_key: boolean
  api_key_url: string | null
  headers: Record<string, string>
}

// ------------------------------------------------------------------ prompt sets

export type SourceKind = "huggingface" | "file" | "paste"

export type PromptSetStatus = "importing" | "ready" | "failed"

export interface PromptSet {
  id: number
  name: string
  source_kind: SourceKind
  source: Record<string, unknown>
  prompt_count: number
  created_at: string
  run_count: number
  status: PromptSetStatus
  status_detail: string | null
  rows_read: number
  rows_total: number | null
}

export interface Prompt {
  id: number
  idx: number
  messages: Message[]
  meta: Record<string, unknown>
}

// ------------------------------------------------------------------ imports

export type ImportFormat = "auto" | "text" | "markdown" | "csv" | "tsv" | "json" | "jsonl" | "parquet"
export type SplitMode = "auto" | "line" | "blank_line" | "separator" | "heading" | "list_item" | "whole"

export type ImportSource =
  | { kind: "huggingface"; dataset: string; config?: string | null; split?: string | null; all_splits?: boolean }
  | { kind: "file"; upload_id: string; format: ImportFormat }
  | { kind: "paste"; text: string }

export type Turns = "up_to_last_user" | "first_user"

export interface ImportMapping {
  prompt_field?: string | null
  system_field?: string | null
  split_mode?: SplitMode
  turns?: Turns
  offset?: number
  limit?: number | null
  dedupe?: boolean
}

export interface ColumnInfo {
  name: string
  kind: "text" | "messages" | "number" | "other"
  example: string | null
  looks_like_output: boolean
}

export interface HFSplit {
  config: string
  split: string
  num_rows: number | null
}

export interface ImportPreview {
  detected_format: string
  columns: ColumnInfo[]
  prompt_field: string | null
  system_field: string | null
  split_mode: SplitMode | null
  total_rows: number | null
  estimated_prompts: number | null
  prompts: Message[][]
  warnings: string[]
  hf_splits: HFSplit[]
  hf_config: string | null
  hf_split: string | null
  has_conversations: boolean
  dropped: string[]
}

export interface Upload {
  upload_id: string
  filename: string
  size: number
  detected_format: string
}

// ------------------------------------------------------------------ runs

export type RunStatus = "running" | "pausing" | "paused" | "completed"
export type SampleStatus = "pending" | "running" | "done" | "failed"

export interface RunParams {
  samples_per_prompt: number
  temperature: number | null
  top_p: number | null
  max_tokens: number | null
  max_tokens_field: "max_tokens" | "max_completion_tokens"
  seed: number | null
  system_prompt: string | null
  user_template: string | null
  extra_body: Record<string, unknown>
  stream: boolean
  think_open: string
  think_close: string
  max_attempts: number
  request_timeout_s: number
  pause_after_failures: number
}

export interface RunCreate {
  name?: string | null
  endpoint_id: number
  model: string
  prompt_set_id: number
  concurrency: number
  params: Partial<RunParams>
}

export interface Run {
  id: number
  name: string
  endpoint_id: number | null
  endpoint_name: string
  model: string
  prompt_set_id: number
  prompt_set_name: string
  params: RunParams
  concurrency: number
  status: RunStatus
  status_reason: string | null
  total: number
  done: number
  failed: number
  truncated: number
  pending: number
  prompt_tokens: number
  completion_tokens: number
  reasoning_tokens: number
  active_seconds: number
  created_at: string
  started_at: string | null
  finished_at: string | null
}

export interface LiveSequence {
  sample_id: number
  prompt_idx: number
  sample_idx: number
  prompt_preview: string
  phase: "waiting" | "thinking" | "answering"
  tokens: number
  tail: string
  elapsed_ms: number
  attempt: number
}

export interface MetricPoint {
  t: string
  tokens_per_s: number
  samples_per_min: number
  active: number
  concurrency: number
  done: number
  failed: number
}

export interface RunLive {
  run: Run
  in_flight: number
  sequences: LiveSequence[]
  tokens_per_s: number
  samples_per_min: number
  eta_seconds: number | null
  metrics: MetricPoint[]
}

export interface Sample {
  id: number
  run_id: number
  prompt_id: number
  prompt_idx: number
  sample_idx: number
  status: SampleStatus
  reasoning: string | null
  response: string | null
  finish_reason: string | null
  prompt_tokens: number | null
  completion_tokens: number | null
  reasoning_tokens: number | null
  latency_ms: number | null
  ttft_ms: number | null
  attempts: number
  error: string | null
  completed_at: string | null
  messages: Message[]
}

export interface SampleSummary {
  id: number
  prompt_idx: number
  sample_idx: number
  status: SampleStatus
  prompt_preview: string
  response_preview: string | null
  has_reasoning: boolean
  finish_reason: string | null
  completion_tokens: number | null
  latency_ms: number | null
  error: string | null
}

// ------------------------------------------------------------------ exports

export type SFTFormat = "messages" | "sharegpt" | "alpaca" | "prompt_completion" | "raw"
export type PrefFormat = "dpo" | "kto"
export type FileType = "jsonl" | "json" | "parquet" | "csv"
export type ReasoningPlacement = "think_tags" | "reasoning_content" | "thinking" | "omit"

export interface ExportOptions {
  format: SFTFormat
  file_type: FileType
  reasoning: ReasoningPlacement
  include_system: boolean
  exclude_truncated: boolean
  require_reasoning: boolean
  first_sample_only: boolean
}

export interface PrefExportOptions {
  format: PrefFormat
  file_type: FileType
  reasoning: ReasoningPlacement
  include_system: boolean
  conversational: boolean
}

export interface ExportPreview {
  count: number
  skipped: Record<string, number>
  example: Record<string, unknown> | null
  filename: string
}

export interface HubPushIn {
  repo_id: string
  private: boolean
  split: string
}

export interface HubPushOut {
  url: string
  count: number
}

// ------------------------------------------------------------------ preferences

export interface PreferenceSet {
  id: number
  name: string
  prompt_set_id: number
  prompt_set_name: string
  run_ids: number[]
  runs: { id: number; name: string; model: string }[]
  prompt_count: number
  decided: number
  created_at: string
}

export interface Candidate {
  sample_id: number
  run_id: number
  run_name: string
  model: string
  sample_idx: number
  reasoning: string | null
  response: string | null
  finish_reason: string | null
  completion_tokens: number | null
  reasoning_tokens?: number | null
}

export interface PreferenceItem {
  prompt_id: number
  prompt_idx: number
  messages: Message[]
  candidates: Candidate[]
  chosen_sample_id: number | null
  rejected_sample_id: number | null
  source: string | null
}

// ------------------------------------------------------------------ settings

export interface Settings {
  hf_token_set: boolean
  hf_token_hint: string | null
  hf_token_from_env: boolean
  data_dir: string
  version: string
  mode: "docker" | "desktop" | "native"
}
