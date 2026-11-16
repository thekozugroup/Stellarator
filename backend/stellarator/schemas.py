"""API request and response shapes. This file is the contract with the frontend
(`frontend/src/lib/types.ts` mirrors it)."""

from __future__ import annotations

from datetime import datetime
from typing import Any, Generic, Literal, TypeVar

from pydantic import BaseModel, ConfigDict, Field, field_validator


class ORM(BaseModel):
    model_config = ConfigDict(from_attributes=True)


Message = dict[str, Any]  # {"role": "system" | "user" | "assistant", "content": str}


# --------------------------------------------------------------------------- endpoints


class EndpointIn(BaseModel):
    name: str = Field(min_length=1, max_length=200)
    base_url: str = Field(min_length=1, max_length=2000)
    api_key: str | None = None
    location: Literal["local", "cloud"] = "local"
    default_concurrency: int = Field(default=16, ge=1, le=4096)
    headers: dict[str, str] = Field(default_factory=dict)

    @field_validator("base_url")
    @classmethod
    def _strip_url(cls, v: str) -> str:
        v = v.strip().rstrip("/")
        if not (v.startswith("http://") or v.startswith("https://")):
            raise ValueError("Base URL must start with http:// or https://")
        return v


class EndpointPatch(BaseModel):
    name: str | None = Field(default=None, min_length=1, max_length=200)
    base_url: str | None = None
    # None leaves the key unchanged; "" removes it.
    api_key: str | None = None
    location: Literal["local", "cloud"] | None = None
    default_concurrency: int | None = Field(default=None, ge=1, le=4096)
    headers: dict[str, str] | None = None

    @field_validator("base_url")
    @classmethod
    def _strip_url(cls, v: str | None) -> str | None:
        if v is None:
            return v
        v = v.strip().rstrip("/")
        if not (v.startswith("http://") or v.startswith("https://")):
            raise ValueError("Base URL must start with http:// or https://")
        return v


class EndpointOut(ORM):
    id: int
    name: str
    base_url: str
    has_api_key: bool
    api_key_hint: str | None  # last 4 characters, e.g. "…a9f2"
    location: str
    default_concurrency: int
    headers: dict[str, str]
    models: list[str]
    last_checked_at: datetime | None
    last_ok: bool | None
    last_error: str | None
    created_at: datetime
    run_count: int = 0


class EndpointTestIn(BaseModel):
    base_url: str
    api_key: str | None = None
    headers: dict[str, str] = Field(default_factory=dict)
    # When testing an existing endpoint's edit form with the key left blank, use its saved key.
    endpoint_id: int | None = None


class EndpointTestOut(BaseModel):
    ok: bool
    models: list[str]
    latency_ms: int | None
    error: str | None


# --------------------------------------------------------------------------- prompt sets


class PromptSetOut(ORM):
    id: int
    name: str
    source_kind: str
    source: dict[str, Any]
    prompt_count: int
    created_at: datetime
    run_count: int = 0


class PromptSetPatch(BaseModel):
    name: str = Field(min_length=1, max_length=300)


class PromptOut(ORM):
    id: int
    idx: int
    messages: list[Message]
    meta: dict[str, Any]


T = TypeVar("T")


class Page(BaseModel, Generic[T]):
    items: list[T]
    total: int
    offset: int
    limit: int


# --------------------------------------------------------------------------- imports

ImportFormat = Literal["auto", "text", "markdown", "csv", "tsv", "json", "jsonl", "parquet"]
# How plain text and Markdown are split into prompts.
SplitMode = Literal[
    "auto",  # pick by content: separators, then headings, then list items, then blank lines
    "line",  # one prompt per non-empty line
    "blank_line",  # paragraphs separated by one or more blank lines
    "separator",  # blocks separated by a line of --- (or ***, ___)
    "heading",  # one prompt per Markdown heading section (heading text + body)
    "list_item",  # one prompt per top-level list item
    "whole",  # the entire file is one prompt
]


class HFSource(BaseModel):
    kind: Literal["huggingface"] = "huggingface"
    dataset: str = Field(min_length=1)  # "owner/name"
    config: str | None = None
    split: str | None = None


class FileSource(BaseModel):
    kind: Literal["file"] = "file"
    upload_id: str  # returned by POST /api/imports/upload
    format: ImportFormat = "auto"


class PasteSource(BaseModel):
    kind: Literal["paste"] = "paste"
    text: str


ImportSource = HFSource | FileSource | PasteSource


class ImportMapping(BaseModel):
    """How rows become prompts. Ignored fields are kept as prompt meta."""

    # Column holding the prompt: a string, or a list of chat messages (OpenAI {role, content}
    # or ShareGPT {from, value}). None = auto-detect.
    prompt_field: str | None = None
    # Optional column holding a system prompt.
    system_field: str | None = None
    # For text and Markdown sources.
    split_mode: SplitMode = "auto"
    # Row window. limit None = all rows.
    offset: int = Field(default=0, ge=0)
    limit: int | None = Field(default=None, ge=1)
    # Drop prompts whose text is identical to an earlier one.
    dedupe: bool = True


class ImportPreviewIn(BaseModel):
    source: HFSource | FileSource | PasteSource = Field(discriminator="kind")
    mapping: ImportMapping = Field(default_factory=ImportMapping)


class ColumnInfo(BaseModel):
    name: str
    kind: Literal["text", "messages", "number", "other"]
    example: str | None  # first non-empty value, shortened


class HFSplit(BaseModel):
    config: str
    split: str
    num_rows: int | None


class ImportPreviewOut(BaseModel):
    detected_format: str  # "huggingface", "csv", "markdown", ...
    columns: list[ColumnInfo]  # empty for text/markdown/paste
    # Field choices after auto-detection, so the UI can preselect them.
    prompt_field: str | None
    system_field: str | None
    split_mode: SplitMode | None  # set for text/markdown/paste
    total_rows: int | None  # rows in the source (before offset/limit/dedupe), when known
    estimated_prompts: int | None  # prompts the import will create, when known
    prompts: list[list[Message]]  # first prompts as they will be stored (up to 20)
    warnings: list[str]
    # Hugging Face only: every config/split, so the UI can offer a choice.
    hf_splits: list[HFSplit] = Field(default_factory=list)
    hf_config: str | None = None
    hf_split: str | None = None


class ImportCommitIn(ImportPreviewIn):
    name: str = Field(min_length=1, max_length=300)


class UploadOut(BaseModel):
    upload_id: str
    filename: str
    size: int
    detected_format: str


# --------------------------------------------------------------------------- runs


class RunParams(BaseModel):
    samples_per_prompt: int = Field(default=1, ge=1, le=64)
    temperature: float | None = Field(default=0.7, ge=0, le=2)
    top_p: float | None = Field(default=None, gt=0, le=1)
    max_tokens: int | None = Field(default=8192, ge=1)
    # Some APIs (OpenAI reasoning models) want max_completion_tokens instead of max_tokens.
    max_tokens_field: Literal["max_tokens", "max_completion_tokens"] = "max_tokens"
    seed: int | None = None
    # Replaces any system message from the prompt set when set.
    system_prompt: str | None = None
    # Wraps each user prompt; must contain {prompt}. e.g. "Solve step by step.\n\n{prompt}"
    user_template: str | None = None
    # Merged into the request body last, e.g. {"chat_template_kwargs": {"enable_thinking": true}}
    extra_body: dict[str, Any] = Field(default_factory=dict)
    stream: bool = True
    # Tags used to split thinking from the answer when the server returns them inline.
    think_open: str = "<think>"
    think_close: str = "</think>"
    max_attempts: int = Field(default=4, ge=1, le=20)
    request_timeout_s: float = Field(default=1800, ge=10)
    # Pause the run after this many samples fail in a row (0 = never).
    pause_after_failures: int = Field(default=10, ge=0)

    @field_validator("user_template")
    @classmethod
    def _template_has_slot(cls, v: str | None) -> str | None:
        if v is not None and v.strip() and "{prompt}" not in v:
            raise ValueError("The prompt template must contain {prompt}")
        return v or None


class RunCreate(BaseModel):
    name: str | None = Field(default=None, max_length=300)  # default: "<model> on <prompt set>"
    endpoint_id: int
    model: str = Field(min_length=1)
    prompt_set_id: int
    concurrency: int = Field(default=16, ge=1, le=4096)
    params: RunParams = Field(default_factory=RunParams)


class RunPatch(BaseModel):
    name: str | None = Field(default=None, min_length=1, max_length=300)
    concurrency: int | None = Field(default=None, ge=1, le=4096)


class RunOut(ORM):
    id: int
    name: str
    endpoint_id: int | None
    endpoint_name: str
    model: str
    prompt_set_id: int
    prompt_set_name: str
    params: RunParams
    concurrency: int
    status: Literal["running", "pausing", "paused", "completed"]
    status_reason: str | None
    total: int
    done: int
    failed: int
    truncated: int
    pending: int  # total - done - failed - in flight
    prompt_tokens: int
    completion_tokens: int
    reasoning_tokens: int
    active_seconds: float
    created_at: datetime
    started_at: datetime | None
    finished_at: datetime | None


class LiveSequence(BaseModel):
    """A request in flight, for the run screen."""

    sample_id: int
    prompt_idx: int
    sample_idx: int
    prompt_preview: str  # first ~160 chars of the last user message
    phase: Literal["waiting", "thinking", "answering"]  # waiting = no token yet
    tokens: int  # streamed so far (chunk count when the server does not report usage)
    tail: str  # last ~240 characters streamed (thinking or answer, per phase)
    elapsed_ms: int
    attempt: int


class MetricPoint(BaseModel):
    t: datetime
    tokens_per_s: float
    samples_per_min: float
    active: int
    concurrency: int
    done: int
    failed: int


class RunLive(BaseModel):
    run: RunOut
    in_flight: int
    sequences: list[LiveSequence]  # up to 64, oldest first
    tokens_per_s: float  # over the last ~10 s
    samples_per_min: float  # over the last ~60 s
    eta_seconds: float | None  # None when unknown
    metrics: list[MetricPoint]  # downsampled to at most 240 points


class SampleOut(ORM):
    id: int
    run_id: int
    prompt_id: int
    prompt_idx: int
    sample_idx: int
    status: Literal["pending", "running", "done", "failed"]
    reasoning: str | None
    response: str | None
    finish_reason: str | None
    prompt_tokens: int | None
    completion_tokens: int | None
    reasoning_tokens: int | None
    latency_ms: int | None
    ttft_ms: int | None
    attempts: int
    error: str | None
    completed_at: datetime | None
    messages: list[Message] = Field(default_factory=list)  # the prompt as sent


class SampleSummary(BaseModel):
    """A row in a run's sample table (text shortened)."""

    id: int
    prompt_idx: int
    sample_idx: int
    status: Literal["pending", "running", "done", "failed"]
    prompt_preview: str
    response_preview: str | None
    has_reasoning: bool
    finish_reason: str | None
    completion_tokens: int | None
    latency_ms: int | None
    error: str | None


# --------------------------------------------------------------------------- exports

SFTFormat = Literal["messages", "sharegpt", "alpaca", "prompt_completion", "raw"]
PrefFormat = Literal["dpo", "kto"]
FileType = Literal["jsonl", "json", "parquet", "csv"]
# Where the thinking trace goes in the exported assistant turn.
ReasoningPlacement = Literal["think_tags", "reasoning_content", "thinking", "omit"]


class ExportOptions(BaseModel):
    format: SFTFormat = "messages"
    file_type: FileType = "jsonl"
    reasoning: ReasoningPlacement = "think_tags"
    include_system: bool = True
    exclude_truncated: bool = True
    require_reasoning: bool = False
    first_sample_only: bool = False


class PrefExportOptions(BaseModel):
    format: PrefFormat = "dpo"
    file_type: FileType = "jsonl"
    reasoning: ReasoningPlacement = "think_tags"
    include_system: bool = True
    # True: prompt/chosen/rejected are message lists (TRL conversational). False: plain strings.
    conversational: bool = True


class ExportPreviewOut(BaseModel):
    count: int  # records the export will contain
    skipped: dict[str, int]  # reason -> count, e.g. {"truncated": 12, "failed": 3}
    example: dict[str, Any] | None  # the first record exactly as it will be written
    filename: str


class HubPushIn(BaseModel):
    repo_id: str = Field(min_length=3)  # "owner/name"
    private: bool = True
    split: str = "train"


class HubPushOut(BaseModel):
    url: str
    count: int


# --------------------------------------------------------------------------- preferences


class PreferenceSetCreate(BaseModel):
    name: str = Field(min_length=1, max_length=300)
    run_ids: list[int] = Field(min_length=1)


class PreferenceSetOut(ORM):
    id: int
    name: str
    prompt_set_id: int
    prompt_set_name: str
    run_ids: list[int]
    runs: list[dict[str, Any]]  # [{"id", "name", "model"}]
    prompt_count: int  # prompts with at least two finished candidates
    decided: int  # prompts with a pair
    created_at: datetime


class Candidate(BaseModel):
    sample_id: int
    run_id: int
    run_name: str
    model: str
    sample_idx: int
    reasoning: str | None
    response: str | None
    finish_reason: str | None
    completion_tokens: int | None


class PreferenceItem(BaseModel):
    prompt_id: int
    prompt_idx: int
    messages: list[Message]
    candidates: list[Candidate]
    chosen_sample_id: int | None
    rejected_sample_id: int | None
    source: str | None


class PairIn(BaseModel):
    chosen_sample_id: int
    rejected_sample_id: int


class AutoPairIn(BaseModel):
    """Prefer every finished sample of one run over another run's, prompt by prompt."""

    chosen_run_id: int
    rejected_run_id: int
    overwrite: bool = False  # replace pairs already made by hand


class AutoPairOut(BaseModel):
    created: int
    skipped: int


# --------------------------------------------------------------------------- settings


class SettingsOut(BaseModel):
    hf_token_set: bool
    hf_token_hint: str | None
    hf_token_from_env: bool
    data_dir: str
    version: str


class SettingsIn(BaseModel):
    # None leaves it unchanged; "" removes it.
    hf_token: str | None = None
