from datetime import datetime
from enum import Enum
from typing import Any

from sqlalchemy import JSON, Boolean, DateTime, Float, ForeignKey, String, Text
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.core.db import Base


class RunStatus(str, Enum):
    queued = "queued"
    running = "running"
    paused = "paused"
    succeeded = "succeeded"
    failed = "failed"
    cancelled = "cancelled"


class Run(Base):
    __tablename__ = "runs"

    id: Mapped[str] = mapped_column(String(40), primary_key=True)
    owner_agent: Mapped[str] = mapped_column(String(64), index=True)
    name: Mapped[str] = mapped_column(String(200))
    status: Mapped[str] = mapped_column(String(32), default=RunStatus.queued.value, index=True)

    base_model: Mapped[str] = mapped_column(String(200))
    method: Mapped[str] = mapped_column(String(32))  # sft | dpo | grpo | ppo | rm
    hyperparams: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    dataset_mixture: Mapped[list[dict[str, Any]]] = mapped_column(JSON, default=list)

    user_goal: Mapped[str] = mapped_column(Text, default="")
    user_context: Mapped[str] = mapped_column(Text, default="")
    agent_plan: Mapped[str] = mapped_column(Text, default="")
    citations: Mapped[list[dict[str, Any]]] = mapped_column(JSON, default=list)

    tinker_job_id: Mapped[str | None] = mapped_column(String(120), nullable=True, index=True)

    # ML Intern loop additions
    is_sandbox: Mapped[bool] = mapped_column(Boolean, default=False, server_default="0")
    preflight_json: Mapped[dict[str, Any] | None] = mapped_column(JSON, nullable=True)
    parent_run_id: Mapped[str | None] = mapped_column(
        String(40), ForeignKey("runs.id", ondelete="SET NULL"), nullable=True, index=True
    )

    gpu_type: Mapped[str] = mapped_column(String(32), default="H100")
    gpu_count: Mapped[int] = mapped_column(default=1)
    gpu_seconds: Mapped[float] = mapped_column(Float, default=0.0)
    cost_usd: Mapped[float] = mapped_column(Float, default=0.0)

    created_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)
    started_at: Mapped[datetime | None] = mapped_column(DateTime, nullable=True)
    finished_at: Mapped[datetime | None] = mapped_column(DateTime, nullable=True)

    notes: Mapped[list["RunNote"]] = relationship(back_populates="run", cascade="all, delete-orphan")
    metrics: Mapped[list["RunMetric"]] = relationship(back_populates="run", cascade="all, delete-orphan")


class RunNote(Base):
    __tablename__ = "run_notes"

    id: Mapped[int] = mapped_column(primary_key=True)
    run_id: Mapped[str] = mapped_column(ForeignKey("runs.id", ondelete="CASCADE"), index=True)
    author_agent: Mapped[str] = mapped_column(String(64))
    kind: Mapped[str] = mapped_column(String(32))  # plan | progress | warning | result
    body: Mapped[str] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)

    run: Mapped[Run] = relationship(back_populates="notes")


class RunMetric(Base):
    __tablename__ = "run_metrics"

    id: Mapped[int] = mapped_column(primary_key=True)
    run_id: Mapped[str] = mapped_column(ForeignKey("runs.id", ondelete="CASCADE"), index=True)
    step: Mapped[int] = mapped_column(default=0)
    name: Mapped[str] = mapped_column(String(64))
    value: Mapped[float] = mapped_column(Float)
    created_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)

    run: Mapped[Run] = relationship(back_populates="metrics")
