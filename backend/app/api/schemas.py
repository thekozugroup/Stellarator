"""Pydantic schemas for the Stellarator API."""

from __future__ import annotations

from datetime import datetime
from typing import Any, Literal

from pydantic import BaseModel, Field, field_validator

VALID_METHODS = {"sft", "dpo", "grpo", "ppo", "rm"}
VALID_NOTE_KINDS = {"plan", "progress", "warning", "result"}


# ---------------------------------------------------------------------------
# Sub-models
# ---------------------------------------------------------------------------


class DatasetEntry(BaseModel):
    name: str
    weight: float = Field(gt=0)
    source: str


# ---------------------------------------------------------------------------
# Run schemas
# ---------------------------------------------------------------------------


class RunCreate(BaseModel):
    name: str = Field(min_length=1, max_length=200)
    base_model: str = Field(min_length=1, max_length=200)
    method: str
    hyperparams: dict[str, Any] = Field(default_factory=dict)
    dataset_mixture: list[DatasetEntry]
    gpu_type: str = Field(default="H100", max_length=32)
    gpu_count: int = Field(default=1, ge=1)
    user_goal: str = ""
    user_context: str = ""
    agent_plan: str = ""
    citations: list[dict[str, Any]] = Field(default_factory=list)

    # ML Intern loop additions (all optional for backwards compat)
    is_sandbox: bool = False
    preflight_json: dict[str, Any] | None = None
    sandbox_run_id: str | None = None

    @field_validator("method")
    @classmethod
    def validate_method(cls, v: str) -> str:
        if v not in VALID_METHODS:
            raise ValueError(f"method must be one of {sorted(VALID_METHODS)}")
        return v


class RunNoteOut(BaseModel):
    id: int
    run_id: str
    author_agent: str
    kind: str
    body: str
    created_at: datetime

    model_config = {"from_attributes": True}


class RunMetricOut(BaseModel):
    id: int
    run_id: str
    step: int
    name: str
    value: float
    created_at: datetime

    model_config = {"from_attributes": True}


class RunOut(BaseModel):
    id: str
    owner_agent: str
    name: str
    status: str
    base_model: str
    method: str
    hyperparams: dict[str, Any]
    dataset_mixture: list[Any]
    user_goal: str
    user_context: str
    agent_plan: str
    citations: list[Any]
    tinker_job_id: str | None
    gpu_type: str
    gpu_count: int
    gpu_seconds: float
    cost_usd: float
    created_at: datetime
    started_at: datetime | None
    finished_at: datetime | None
    # RL-parity signals
    reward_mean: float | None = None
    percent_correct: float | None = None
    checkpoint_url: str | None = None

    model_config = {"from_attributes": True}


class RunDetail(RunOut):
    notes: list[RunNoteOut] = []
    metrics: list[RunMetricOut] = []


# ---------------------------------------------------------------------------
# Note schemas
# ---------------------------------------------------------------------------


class PromoteCreate(BaseModel):
    name: str = Field(min_length=1, max_length=200)
    gpu_type: str = Field(default="H100", max_length=32)
    gpu_count: int = Field(ge=1)
    hyperparams_overrides: dict[str, Any] = Field(default_factory=dict)
    user_goal: str = ""


class NoteCreate(BaseModel):
    kind: str
    body: str = Field(min_length=1)

    @field_validator("kind")
    @classmethod
    def validate_kind(cls, v: str) -> str:
        if v not in VALID_NOTE_KINDS:
            raise ValueError(f"kind must be one of {sorted(VALID_NOTE_KINDS)}")
        return v


# ---------------------------------------------------------------------------
# Agent / auth schemas
# ---------------------------------------------------------------------------


class WhoAmIOut(BaseModel):
    agent: str
