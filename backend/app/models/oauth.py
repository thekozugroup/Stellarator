"""OpenAI OAuth token storage.

Mirrors :class:`app.models.chat.CodexToken` but is dedicated to the
"Sign in with OpenAI" browser flow. Kept in its own table (rather than
extending ``codex_tokens``) so each provider can evolve independently —
schema additions like ``id_token`` or ``email`` would otherwise pollute the
codex row shape.

All token columns hold Fernet-encrypted ciphertext (see
:mod:`app.services.crypto`); never decrypt outside the driver layer.
"""

from __future__ import annotations

from datetime import datetime

from sqlalchemy import DateTime, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from app.core.db import Base


class OpenAIToken(Base):
    __tablename__ = "openai_tokens"

    agent_id: Mapped[str] = mapped_column(String(64), primary_key=True)
    access_token: Mapped[str] = mapped_column(Text)
    refresh_token: Mapped[str] = mapped_column(Text, default="")
    expires_at: Mapped[datetime | None] = mapped_column(DateTime, nullable=True)
    # Plaintext email (extracted from id_token claims) for display only —
    # never store any other id_token claims.
    email: Mapped[str | None] = mapped_column(String(320), nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)
    updated_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)
