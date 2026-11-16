"""SQLAlchemy ORM models.

Conventions
- A prompt is stored as a list of chat messages: [{"role": "system"|"user"|"assistant", "content": str}].
  The last message is always a user message.
- A sample is one request to the model for one prompt. A run with `samples_per_prompt = 3` over
  100 prompts has 300 samples.
- Timestamps are timezone-aware UTC.
"""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from sqlalchemy import (
    JSON,
    Boolean,
    DateTime,
    Float,
    ForeignKey,
    Index,
    Integer,
    String,
    Text,
    UniqueConstraint,
)
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column, relationship
from sqlalchemy.types import TypeDecorator


def utcnow() -> datetime:
    return datetime.now(timezone.utc)


class UTCDateTime(TypeDecorator):
    """A timestamp stored as UTC and always read back timezone-aware.

    SQLite has no time zone type, so a plain UTCDateTime() comes back naive and the API
    would send times without an offset, which browsers read as local time.
    """

    impl = DateTime
    cache_ok = True

    def process_bind_param(self, value: datetime | None, dialect):  # noqa: ARG002
        if value is not None and value.tzinfo is not None:
            value = value.astimezone(timezone.utc).replace(tzinfo=None)
        return value

    def process_result_value(self, value: datetime | None, dialect):  # noqa: ARG002
        if value is not None and value.tzinfo is None:
            value = value.replace(tzinfo=timezone.utc)
        return value


class Base(DeclarativeBase):
    type_annotation_map = {dict[str, Any]: JSON, list[Any]: JSON}


class Endpoint(Base):
    __tablename__ = "endpoints"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String(200))
    base_url: Mapped[str] = mapped_column(String(2000))
    api_key: Mapped[str | None] = mapped_column(Text, nullable=True)
    # "local" (your own hardware) or "cloud" (hosted API). Only changes the suggested seqs.
    location: Mapped[str] = mapped_column(String(10), default="local")
    default_concurrency: Mapped[int] = mapped_column(Integer, default=16)
    # Extra HTTP headers sent with every request, e.g. {"HTTP-Referer": "..."}
    headers: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    models: Mapped[list[Any]] = mapped_column(JSON, default=list)  # last fetched model ids
    last_checked_at: Mapped[datetime | None] = mapped_column(UTCDateTime(), nullable=True)
    last_ok: Mapped[bool | None] = mapped_column(Boolean, nullable=True)
    last_error: Mapped[str | None] = mapped_column(Text, nullable=True)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime(), default=utcnow)
    updated_at: Mapped[datetime] = mapped_column(
        UTCDateTime(), default=utcnow, onupdate=utcnow
    )


class PromptSet(Base):
    __tablename__ = "prompt_sets"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String(300))
    # "huggingface" | "file" | "paste"
    source_kind: Mapped[str] = mapped_column(String(20))
    # Where it came from, for display: {"dataset": "openai/gsm8k", "config": "main", "split": "train",
    # "prompt_field": "question"} or {"filename": "prompts.md", "format": "markdown", ...}
    source: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    prompt_count: Mapped[int] = mapped_column(Integer, default=0)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime(), default=utcnow)

    prompts: Mapped[list[Prompt]] = relationship(
        back_populates="prompt_set", cascade="all, delete-orphan", passive_deletes=True
    )


class Prompt(Base):
    __tablename__ = "prompts"
    __table_args__ = (Index("ix_prompts_set_idx", "prompt_set_id", "idx"),)

    id: Mapped[int] = mapped_column(primary_key=True)
    prompt_set_id: Mapped[int] = mapped_column(ForeignKey("prompt_sets.id", ondelete="CASCADE"))
    idx: Mapped[int] = mapped_column(Integer)  # 0-based position within the set
    messages: Mapped[list[Any]] = mapped_column(JSON)
    # Other columns from the source row (e.g. a reference answer), kept for raw exports.
    meta: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)

    prompt_set: Mapped[PromptSet] = relationship(back_populates="prompts")


# Run statuses
RUN_RUNNING = "running"
RUN_PAUSING = "pausing"  # no new requests; waiting for in-flight ones to finish
RUN_PAUSED = "paused"
RUN_COMPLETED = "completed"
RUN_STATUSES = (RUN_RUNNING, RUN_PAUSING, RUN_PAUSED, RUN_COMPLETED)


class Run(Base):
    __tablename__ = "runs"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String(300))
    endpoint_id: Mapped[int | None] = mapped_column(
        ForeignKey("endpoints.id", ondelete="SET NULL"), nullable=True
    )
    # Snapshot so the run still describes itself if the endpoint is deleted.
    endpoint_name: Mapped[str] = mapped_column(String(200))
    model: Mapped[str] = mapped_column(String(500))
    prompt_set_id: Mapped[int] = mapped_column(ForeignKey("prompt_sets.id", ondelete="CASCADE"))
    # RunParams (see schemas.RunParams) as a dict.
    params: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    concurrency: Mapped[int] = mapped_column(Integer, default=16)
    status: Mapped[str] = mapped_column(String(20), default=RUN_RUNNING)
    # Human-readable reason for the current status, e.g. "Paused after 10 failures in a row: ..."
    status_reason: Mapped[str | None] = mapped_column(Text, nullable=True)

    total: Mapped[int] = mapped_column(Integer, default=0)  # samples
    done: Mapped[int] = mapped_column(Integer, default=0)
    failed: Mapped[int] = mapped_column(Integer, default=0)
    truncated: Mapped[int] = mapped_column(Integer, default=0)  # done with finish_reason "length"
    prompt_tokens: Mapped[int] = mapped_column(Integer, default=0)
    completion_tokens: Mapped[int] = mapped_column(Integer, default=0)
    reasoning_tokens: Mapped[int] = mapped_column(Integer, default=0)
    # Seconds spent in the running state, summed across resumes. Used for averages and ETA.
    active_seconds: Mapped[float] = mapped_column(Float, default=0.0)

    created_at: Mapped[datetime] = mapped_column(UTCDateTime(), default=utcnow)
    started_at: Mapped[datetime | None] = mapped_column(UTCDateTime(), nullable=True)
    finished_at: Mapped[datetime | None] = mapped_column(UTCDateTime(), nullable=True)


# Sample statuses
SAMPLE_PENDING = "pending"
SAMPLE_RUNNING = "running"
SAMPLE_DONE = "done"
SAMPLE_FAILED = "failed"


class Sample(Base):
    __tablename__ = "samples"
    __table_args__ = (
        UniqueConstraint("run_id", "prompt_id", "sample_idx", name="uq_sample"),
        Index("ix_samples_run_status", "run_id", "status"),
        Index("ix_samples_prompt", "prompt_id"),
    )

    id: Mapped[int] = mapped_column(primary_key=True)
    run_id: Mapped[int] = mapped_column(ForeignKey("runs.id", ondelete="CASCADE"))
    prompt_id: Mapped[int] = mapped_column(ForeignKey("prompts.id", ondelete="CASCADE"))
    prompt_idx: Mapped[int] = mapped_column(Integer)  # copy of Prompt.idx for ordering
    sample_idx: Mapped[int] = mapped_column(Integer, default=0)
    status: Mapped[str] = mapped_column(String(10), default=SAMPLE_PENDING)
    reasoning: Mapped[str | None] = mapped_column(Text, nullable=True)
    response: Mapped[str | None] = mapped_column(Text, nullable=True)
    finish_reason: Mapped[str | None] = mapped_column(String(40), nullable=True)
    prompt_tokens: Mapped[int | None] = mapped_column(Integer, nullable=True)
    completion_tokens: Mapped[int | None] = mapped_column(Integer, nullable=True)
    reasoning_tokens: Mapped[int | None] = mapped_column(Integer, nullable=True)
    latency_ms: Mapped[int | None] = mapped_column(Integer, nullable=True)
    ttft_ms: Mapped[int | None] = mapped_column(Integer, nullable=True)  # time to first token
    attempts: Mapped[int] = mapped_column(Integer, default=0)
    error: Mapped[str | None] = mapped_column(Text, nullable=True)
    completed_at: Mapped[datetime | None] = mapped_column(UTCDateTime(), nullable=True)


class RunMetric(Base):
    """A point on the run's throughput chart, written every few seconds while running."""

    __tablename__ = "run_metrics"
    __table_args__ = (Index("ix_run_metrics_run_t", "run_id", "t"),)

    id: Mapped[int] = mapped_column(primary_key=True)
    run_id: Mapped[int] = mapped_column(ForeignKey("runs.id", ondelete="CASCADE"))
    t: Mapped[datetime] = mapped_column(UTCDateTime(), default=utcnow)
    tokens_per_s: Mapped[float] = mapped_column(Float, default=0.0)  # output tokens per second
    samples_per_min: Mapped[float] = mapped_column(Float, default=0.0)
    active: Mapped[int] = mapped_column(Integer, default=0)  # requests in flight
    concurrency: Mapped[int] = mapped_column(Integer, default=0)  # seqs setting at this time
    done: Mapped[int] = mapped_column(Integer, default=0)
    failed: Mapped[int] = mapped_column(Integer, default=0)


class PreferenceSet(Base):
    __tablename__ = "preference_sets"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String(300))
    prompt_set_id: Mapped[int] = mapped_column(ForeignKey("prompt_sets.id", ondelete="CASCADE"))
    run_ids: Mapped[list[Any]] = mapped_column(JSON, default=list)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime(), default=utcnow)


class PreferencePair(Base):
    __tablename__ = "preference_pairs"
    __table_args__ = (UniqueConstraint("preference_set_id", "prompt_id", name="uq_pair_prompt"),)

    id: Mapped[int] = mapped_column(primary_key=True)
    preference_set_id: Mapped[int] = mapped_column(
        ForeignKey("preference_sets.id", ondelete="CASCADE")
    )
    prompt_id: Mapped[int] = mapped_column(ForeignKey("prompts.id", ondelete="CASCADE"))
    chosen_sample_id: Mapped[int] = mapped_column(ForeignKey("samples.id", ondelete="CASCADE"))
    rejected_sample_id: Mapped[int] = mapped_column(ForeignKey("samples.id", ondelete="CASCADE"))
    source: Mapped[str] = mapped_column(String(20), default="manual")  # manual | rule
    created_at: Mapped[datetime] = mapped_column(UTCDateTime(), default=utcnow)
    updated_at: Mapped[datetime] = mapped_column(
        UTCDateTime(), default=utcnow, onupdate=utcnow
    )


class Setting(Base):
    __tablename__ = "settings"

    key: Mapped[str] = mapped_column(String(100), primary_key=True)
    value: Mapped[Any] = mapped_column(JSON)
