"""Security-focused tests for Stellarator backend."""

from __future__ import annotations

import asyncio
import logging
import time
from typing import Any

import pytest
import pytest_asyncio
from httpx import ASGITransport, AsyncClient

from app.agents import oauth_codex
from app.core import config as config_mod
from app.core.config import settings
from app.core.logging_filter import REDACTED, SecretRedactingFilter
from app.main import app
from app.services.tinker import TinkerError


# ---------------------------------------------------------------------------
# 1. Empty token rejected
# ---------------------------------------------------------------------------

def test_empty_token_rejected():
    # All tokens cleared.
    s = config_mod.Settings(
        agent_token_claude_code="",
        agent_token_openai="",
        agent_token_codex="",
    )
    assert s.agent_for_token("") is None
    assert s.agent_for_token("anything") is None


def test_known_token_matches():
    s = config_mod.Settings(
        agent_token_claude_code="abcdefgh-1234",
        agent_token_openai="",
        agent_token_codex="",
    )
    assert s.agent_for_token("abcdefgh-1234") == "claude-code"
    assert s.agent_for_token("wrong") is None


# ---------------------------------------------------------------------------
# 2. Constant-time compare smoke test
# ---------------------------------------------------------------------------

def test_timing_safe_compare_smoke():
    """Smoke test: comparing equal-length wrong tokens shouldn't blow up
    and should still return None. We don't measure wall time (too noisy
    in CI), but we exercise the iteration path."""
    s = config_mod.Settings(
        agent_token_claude_code="A" * 64,
        agent_token_openai="B" * 64,
        agent_token_codex="C" * 64,
    )
    for cand in ("X" * 64, "A" * 63 + "Z", "B" * 63 + "Z"):
        assert s.agent_for_token(cand) is None
    assert s.agent_for_token("A" * 64) == "claude-code"


# ---------------------------------------------------------------------------
# 3 & 4. WebSocket auth
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_ws_without_token_rejected_4401():
    from starlette.testclient import TestClient
    from starlette.websockets import WebSocketDisconnect

    with TestClient(app) as client:
        with pytest.raises(WebSocketDisconnect) as exc:
            with client.websocket_connect("/v1/runs/abc/stream"):
                pass
        assert exc.value.code == 4401


@pytest.mark.asyncio
async def test_ws_with_bad_token_rejected_4401():
    from starlette.testclient import TestClient
    from starlette.websockets import WebSocketDisconnect

    with TestClient(app) as client:
        with pytest.raises(WebSocketDisconnect) as exc:
            with client.websocket_connect("/v1/runs/abc/stream?token=nope"):
                pass
        assert exc.value.code == 4401


# ---------------------------------------------------------------------------
# 5. create_run TinkerError -> 502
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_create_run_tinker_error_502(client: AsyncClient, monkeypatch):
    import app.services.tinker as tinker_mod

    async def boom(**kw: Any):
        raise TinkerError("upstream down")

    monkeypatch.setattr(tinker_mod.tinker, "create_job", boom)

    payload = {
        "name": "t",
        "base_model": "llama-3.1-8b",
        "method": "lora",
        "hyperparams": {"lr": 1e-4},
        "dataset_mixture": [],
        "gpu_type": "h100",
        "gpu_count": 1,
        "user_goal": "g",
        "user_context": "c",
        "agent_plan": "p",
        "citations": [],
    }
    headers = {"Authorization": "Bearer test-token-claude-code"}
    r = await client.post("/v1/runs/", json=payload, headers=headers)
    assert r.status_code == 502, r.text
    body = r.json()
    assert body["detail"]["error"] == "tinker_unavailable"


# ---------------------------------------------------------------------------
# 6. Log filter redacts known token
# ---------------------------------------------------------------------------

def test_log_filter_redacts_known_token(caplog):
    secret = "supersecret-token-abcdefgh-1234"
    flt = SecretRedactingFilter(lambda: [secret])

    record = logging.LogRecord(
        name="test", level=logging.INFO, pathname=__file__, lineno=1,
        msg="bearer leaked: %s", args=(secret,), exc_info=None,
    )
    flt.filter(record)
    rendered = record.getMessage()
    assert secret not in rendered
    assert REDACTED in rendered

    # And in record.msg directly.
    record2 = logging.LogRecord(
        name="test", level=logging.INFO, pathname=__file__, lineno=1,
        msg=f"bearer leaked: {secret}", args=None, exc_info=None,
    )
    flt.filter(record2)
    assert secret not in record2.getMessage()


# ---------------------------------------------------------------------------
# 7. OAuth state replay rejected
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_oauth_state_replay_rejected(monkeypatch):
    """DB-backed nonce replay prevention: second consume returns None."""
    monkeypatch.setattr(settings, "stellarator_secret", "x" * 48)

    from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine
    from app.core.db import Base
    from app.agents.oauth_codex import _db_remember_pending, _db_consume_pending

    engine = create_async_engine("sqlite+aiosqlite:///:memory:", future=True)
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)

    factory = async_sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)
    nonce = "deadbeef" * 4

    async with factory() as s:
        await _db_remember_pending(nonce, "claude-code", "verifier-xyz", s)
        await s.commit()

    async with factory() as s:
        first = await _db_consume_pending(nonce, s)
        await s.commit()
    assert first is not None

    async with factory() as s:
        second = await _db_consume_pending(nonce, s)
    assert second is None  # replay rejected

    await engine.dispose()


# ---------------------------------------------------------------------------
# 8. OAuth state expired rejected
# ---------------------------------------------------------------------------

def test_oauth_state_expired_rejected(monkeypatch):
    monkeypatch.setattr(settings, "stellarator_secret", "y" * 48)
    iat = int(time.time()) - 10_000
    state = oauth_codex._sign_state(
        {"agent": "claude-code", "nonce": "n", "iat": iat, "exp": iat + 600}
    )
    from fastapi import HTTPException
    with pytest.raises(HTTPException) as exc:
        oauth_codex._verify_state(state)
    assert exc.value.status_code == 400
    assert "expired" in exc.value.detail.lower()


# ---------------------------------------------------------------------------
# 9. redirect_uri honored from env
# ---------------------------------------------------------------------------

def test_redirect_uri_honored_from_env(monkeypatch):
    monkeypatch.setattr(
        settings, "codex_oauth_redirect_uri",
        "https://example.test/cb",
    )

    class DummyReq:
        def url_for(self, name: str) -> str:  # pragma: no cover
            raise AssertionError("should not be called when env is set")

    assert oauth_codex._resolve_redirect_uri(DummyReq()) == "https://example.test/cb"


def test_redirect_uri_falls_back_to_request(monkeypatch):
    monkeypatch.setattr(settings, "codex_oauth_redirect_uri", "")

    class DummyReq:
        def url_for(self, name: str) -> str:
            return f"http://test.local/{name}"

    assert oauth_codex._resolve_redirect_uri(DummyReq()) == "http://test.local/oauth_callback"
