"""Integration key model — per-agent encrypted API keys stored in the DB."""

from __future__ import annotations

from datetime import datetime

from sqlalchemy import String, Text, UniqueConstraint
from sqlalchemy.orm import Mapped, mapped_column

from app.core.db import Base

ALLOWED_KINDS = frozenset({"tinker", "openrouter"})


class IntegrationKey(Base):
    __tablename__ = "integration_keys"
    __table_args__ = (
        UniqueConstraint("agent_id", "kind", name="uq_integration_keys_agent_kind"),
    )

    id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    agent_id: Mapped[str] = mapped_column(String(128), index=True, nullable=False)
    kind: Mapped[str] = mapped_column(String(32), nullable=False)
    # Fernet-encrypted, v1:<token> envelope — never NULL.
    ciphertext: Mapped[str] = mapped_column(Text, nullable=False)
    created_at: Mapped[datetime] = mapped_column(default=datetime.utcnow, nullable=False)
    updated_at: Mapped[datetime] = mapped_column(
        default=datetime.utcnow, onupdate=datetime.utcnow, nullable=False
    )
    last_used_at: Mapped[datetime | None] = mapped_column(nullable=True)
