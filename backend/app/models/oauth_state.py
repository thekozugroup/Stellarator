"""DB-backed OAuth PKCE state — replaces in-process LRU dicts.

Makes OAuth flows safe across multiple workers: a nonce can only be consumed
once (used_at IS NULL guard + SET used_at) regardless of which worker handles
the callback.
"""

from __future__ import annotations

from datetime import datetime

from sqlalchemy import DateTime, Index, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from app.core.db import Base


class OAuthState(Base):
    __tablename__ = "oauth_states"

    id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    # "codex" or "openai"
    provider: Mapped[str] = mapped_column(String(32), nullable=False)
    agent_id: Mapped[str] = mapped_column(String(128), nullable=False)
    # secrets.token_hex(16) — globally unique, used as the lookup key
    nonce: Mapped[str] = mapped_column(String(64), unique=True, nullable=False)
    code_verifier: Mapped[str] = mapped_column(Text, nullable=False)
    expires_at: Mapped[datetime] = mapped_column(DateTime, nullable=False)
    # SET on first successful callback; NULL means not yet consumed.
    used_at: Mapped[datetime | None] = mapped_column(DateTime, nullable=True)

    __table_args__ = (
        Index("ix_oauth_states_nonce_expires", "nonce", "expires_at"),
    )
