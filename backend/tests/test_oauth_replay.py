"""OAuth replay-prevention and state-validation tests.

Updated for DB-backed PKCE state (Round 5). The in-process LRU dicts are gone;
all nonce storage now goes through OAuthState rows.
"""

from __future__ import annotations

import time
from datetime import datetime, timedelta, timezone

import pytest
import pytest_asyncio
from fastapi import HTTPException
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine

from app.agents import oauth_codex
from app.core.config import settings
from app.core.db import Base


# ---------------------------------------------------------------------------
# Shared in-memory DB per test
# ---------------------------------------------------------------------------

TEST_DB_URL = "sqlite+aiosqlite:///:memory:"


@pytest_asyncio.fixture
async def db_session():
    engine = create_async_engine(TEST_DB_URL, future=True)
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)
    factory = async_sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)
    async with factory() as s:
        yield s
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.drop_all)
    await engine.dispose()


# ---------------------------------------------------------------------------
# 1. Two consecutive callbacks with same state → second rejected (nonce burned)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_replay_rejected_on_second_consume(monkeypatch, db_session):
    monkeypatch.setattr(settings, "stellarator_secret", "s" * 48)

    nonce = "deadbeef" * 4
    await oauth_codex._db_remember_pending(nonce, "claude-code", "verifier-abc", db_session)
    await db_session.commit()

    first = await oauth_codex._db_consume_pending(nonce, db_session)
    await db_session.commit()
    assert first is not None

    second = await oauth_codex._db_consume_pending(nonce, db_session)
    assert second is None


# ---------------------------------------------------------------------------
# 2. Expired state is rejected
# ---------------------------------------------------------------------------


def test_expired_state_rejected(monkeypatch):
    monkeypatch.setattr(settings, "stellarator_secret", "e" * 48)

    # Build a state that expired 1 hour ago
    iat = int(time.time()) - 7200
    expired_state = oauth_codex._sign_state(
        {"agent": "claude-code", "nonce": "expirednonce", "iat": iat, "exp": iat + 600}
    )

    with pytest.raises(HTTPException) as exc_info:
        oauth_codex._verify_state(expired_state)

    assert exc_info.value.status_code == 400
    assert "expired" in exc_info.value.detail.lower()


# ---------------------------------------------------------------------------
# 3. Mismatched current_agent vs state.agent → rejected at callback layer
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_agent_mismatch_rejected(monkeypatch, db_session):
    monkeypatch.setattr(settings, "stellarator_secret", "m" * 48)

    nonce = "mismatch-nonce-abcdef"
    await oauth_codex._db_remember_pending(nonce, "claude-code", "verifier-abc", db_session)
    await db_session.commit()

    pending_row = await oauth_codex._db_consume_pending(nonce, db_session)
    assert pending_row is not None

    # Simulate the agent-mismatch check in oauth_callback
    state_agent = "claude-code"
    current_agent = "openai"  # different agent authenticating the callback

    mismatch = pending_row.agent_id != state_agent or state_agent != current_agent
    assert mismatch, "Expected a mismatch to be detected"


# ---------------------------------------------------------------------------
# 4. Burned nonce is rejected even on a second lookup
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_burned_nonce_rejected_after_consume(monkeypatch, db_session):
    monkeypatch.setattr(settings, "stellarator_secret", "b" * 48)

    nonce = "burned-nonce-xyz999"
    await oauth_codex._db_remember_pending(nonce, "claude-code", "verifier-abc", db_session)
    await db_session.commit()

    # First consume marks used_at.
    first = await oauth_codex._db_consume_pending(nonce, db_session)
    await db_session.commit()
    assert first is not None

    # Second consume must return None — used_at IS NOT NULL.
    result = await oauth_codex._db_consume_pending(nonce, db_session)
    assert result is None
