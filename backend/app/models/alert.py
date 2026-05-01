"""Run alert and research transcript models.

``RunAlert`` rows are emitted from inside training scripts (via
``trackio.alert(...)``) and consumed by orchestrator agents to decide the
next loop iteration. ``ResearchTranscript`` persists the full sub-agent
research session for audit + post-hoc analysis.
"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from sqlalchemy import JSON, DateTime, ForeignKey, Index, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from app.core.db import Base

ALERT_LEVELS = ("error", "warn", "info")


class RunAlert(Base):
    __tablename__ = "run_alerts"
    __table_args__ = (
        Index("ix_run_alerts_run_created", "run_id", "created_at"),
    )

    id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    run_id: Mapped[str] = mapped_column(
        String(40), ForeignKey("runs.id", ondelete="CASCADE"), index=True
    )
    level: Mapped[str] = mapped_column(String(16))  # error | warn | info
    title: Mapped[str] = mapped_column(String(200))
    body: Mapped[str] = mapped_column(Text, default="")
    source: Mapped[str] = mapped_column(String(64), default="training_script")
    created_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)


class ResearchTranscript(Base):
    __tablename__ = "research_transcripts"

    id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    run_id: Mapped[str | None] = mapped_column(String(40), nullable=True, index=True)
    calling_agent: Mapped[str] = mapped_column(String(64), index=True)
    task: Mapped[str] = mapped_column(Text)
    context: Mapped[str] = mapped_column(Text, default="")
    result_json: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    started_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)
    completed_at: Mapped[datetime | None] = mapped_column(DateTime, nullable=True)
