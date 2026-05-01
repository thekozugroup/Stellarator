"""Budget and cost limits."""

from __future__ import annotations

from datetime import datetime

from sqlalchemy import String
from sqlalchemy.orm import Mapped, mapped_column

from app.core.db import Base


class Budget(Base):
    __tablename__ = "budgets"

    id: Mapped[int] = mapped_column(primary_key=True)
    scope: Mapped[str] = mapped_column(String(32), index=True)  # "agent" | "run"
    scope_id: Mapped[str] = mapped_column(String(128), index=True)  # agent name or run id
    monthly_limit_usd: Mapped[float] = mapped_column(nullable=True)
    daily_limit_usd: Mapped[float] = mapped_column(nullable=True)
    alert_threshold_pct: Mapped[float] = mapped_column(default=80.0)  # alert at 80% spend
    created_at: Mapped[datetime] = mapped_column(default=datetime.utcnow)
