"""OpenAI OAuth flow — replay defence, expiry, redirect_uri, exchange + persist.

Updated for DB-backed PKCE state (Round 5).
"""

from __future__ import annotations

import time
from datetime import datetime, timedelta, timezone

import httpx
import pytest
import pytest_asyncio
import respx
from fastapi import HTTPException
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine

from app.agents import oauth_openai
from app.core.config import settings
from app.core.db import Base
from app.models.oauth import OpenAIToken
from app.models.oauth_state import OAuthState
from app.services.crypto import decrypt

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
# Helpers
# ---------------------------------------------------------------------------


def _make_state(nonce: str, agent: str, secret: str) -> str:
    """Build a signed state payload the same way oauth_start does."""
    from unittest.mock import patch
    with patch.object(settings, "stellarator_secret", secret):
        iat = int(time.time())
        return oauth_openai._sign_state(
            {"agent": agent, "nonce": nonce, "iat": iat, "exp": iat + 600}
        )


# ---------------------------------------------------------------------------
# 1. Replay rejected — second DB consume returns None.
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_replay_rejected(monkeypatch, db_session):
    monkeypatch.setattr(settings, "stellarator_secret", "s" * 48)
    nonce = "feedface" * 4
    await oauth_openai._db_remember_pending(nonce, "claude-code", "verifier-xyz", db_session)
    await db_session.commit()

    first = await oauth_openai._db_consume_pending(nonce, db_session)
    await db_session.commit()
    assert first is not None

    second = await oauth_openai._db_consume_pending(nonce, db_session)
    assert second is None


# ---------------------------------------------------------------------------
# 2. Expired state rejected.
# ---------------------------------------------------------------------------


def test_expired_state_rejected(monkeypatch):
    monkeypatch.setattr(settings, "stellarator_secret", "e" * 48)
    iat = int(time.time()) - 7200
    state = oauth_openai._sign_state(
        {"agent": "claude-code", "nonce": "expirednonce", "iat": iat, "exp": iat + 600}
    )
    with pytest.raises(HTTPException) as exc:
        oauth_openai._verify_state(state)
    assert exc.value.status_code == 400
    assert "expired" in exc.value.detail.lower()


# ---------------------------------------------------------------------------
# 3. redirect_uri honoured from env when set.
# ---------------------------------------------------------------------------


def test_redirect_uri_from_env(monkeypatch):
    monkeypatch.setattr(
        settings, "openai_oauth_redirect_uri", "https://example.test/cb"
    )

    class _Req:
        def url_for(self, _name: str) -> str:
            return "https://NOT-USED.test/cb"

    assert oauth_openai._resolve_redirect_uri(_Req()) == "https://example.test/cb"


def test_redirect_uri_falls_back_to_url_for(monkeypatch):
    monkeypatch.setattr(settings, "openai_oauth_redirect_uri", "")

    class _Req:
        def url_for(self, name: str) -> str:
            assert name == "openai_oauth_callback"
            return "https://derived.test/oauth/callback"

    assert (
        oauth_openai._resolve_redirect_uri(_Req())
        == "https://derived.test/oauth/callback"
    )


# ---------------------------------------------------------------------------
# 4. Code+verifier exchange, mocked, persists encrypted.
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_callback_exchanges_and_persists_encrypted(monkeypatch, db_session):
    monkeypatch.setattr(settings, "stellarator_secret", "k" * 48)
    monkeypatch.setattr(settings, "openai_oauth_client_id", "client-123")
    monkeypatch.setattr(settings, "openai_oauth_client_secret", "secret-abc")
    monkeypatch.setattr(settings, "openai_oauth_redirect_uri", "https://app.test/cb")
    monkeypatch.setattr(settings, "openai_oauth_token_url", "https://auth.test/token")

    nonce = "feedface" * 4
    agent = "openai"
    await oauth_openai._db_remember_pending(nonce, agent, "verifier-xyz", db_session)
    await db_session.commit()

    state = _make_state(nonce, agent, "k" * 48)

    captured: dict = {}

    with respx.mock(assert_all_called=True) as router:
        route = router.post("https://auth.test/token").mock(
            return_value=httpx.Response(
                200,
                json={
                    "access_token": "AT-secret",
                    "refresh_token": "RT-secret",
                    "expires_in": 3600,
                    "id_token": _make_id_token({"email": "u@e.com"}),
                },
            )
        )

        class _Req:
            def url_for(self, _name: str) -> str:
                return "https://app.test/cb"

        # Direct invocation — bypass full FastAPI plumbing.
        await oauth_openai.oauth_callback(
            request=_Req(),  # type: ignore[arg-type]
            code="auth-code-xyz",
            state=state,
            agent=agent,
            session=db_session,
        )

        assert route.called
        body = _form(route.calls.last.request.content)
        captured.update(body)

    # Verify persisted row.
    row = (
        await db_session.execute(select(OpenAIToken).where(OpenAIToken.agent_id == agent))
    ).scalar_one()
    assert row.access_token.startswith("v1:")
    assert row.refresh_token.startswith("v1:")
    assert decrypt(row.access_token) == "AT-secret"
    assert decrypt(row.refresh_token) == "RT-secret"
    assert row.email == "u@e.com"
    assert row.expires_at is not None

    # Verify the exchange payload included the verifier and the env settings.
    assert captured.get("client_id") == "client-123"
    assert captured.get("redirect_uri") == "https://app.test/cb"
    assert captured.get("code_verifier") == "verifier-xyz"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_id_token(claims: dict) -> str:
    """Build a fake unsigned id_token with just enough structure for parsing."""
    import base64
    import json

    def _b64(b: bytes) -> str:
        return base64.urlsafe_b64encode(b).decode("ascii").rstrip("=")

    header = _b64(b'{"alg":"none","typ":"JWT"}')
    payload = _b64(json.dumps(claims).encode("utf-8"))
    sig = _b64(b"sig-not-verified")
    return f"{header}.{payload}.{sig}"


def _form(content: bytes) -> dict:
    """Decode form-urlencoded request body into a flat dict."""
    from urllib.parse import parse_qsl

    return dict(parse_qsl(content.decode("utf-8")))
