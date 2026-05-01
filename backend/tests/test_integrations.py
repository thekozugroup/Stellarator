"""Tests for /v1/integrations endpoints and TinkerClient key resolution."""

from __future__ import annotations

from datetime import datetime, timezone
from unittest.mock import AsyncMock, patch

import pytest
import pytest_asyncio
import respx
import httpx
from httpx import AsyncClient, Response

from app.models.integration import IntegrationKey
from app.services import crypto as crypto_mod
from app.services.crypto import decrypt, encrypt
from app.services.tinker import TinkerKeyMissing, _resolve_key

from tests.conftest import CLAUDE_CODE_TOKEN, OPENAI_TOKEN

pytestmark = pytest.mark.asyncio

CC_HEADERS = {"Authorization": f"Bearer {CLAUDE_CODE_TOKEN}"}
OA_HEADERS = {"Authorization": f"Bearer {OPENAI_TOKEN}"}


# ---------------------------------------------------------------------------
# PUT — upsert
# ---------------------------------------------------------------------------


async def test_put_key_encrypts_at_rest(client: AsyncClient, session):
    """Stored ciphertext must differ from the plaintext value."""
    resp = await client.put(
        "/v1/integrations/keys/tinker",
        json={"value": "sk-test-abc123"},
        headers=CC_HEADERS,
    )
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert "masked" in data

    # Verify ciphertext in the DB differs from plaintext.
    from sqlalchemy import select
    stmt = select(IntegrationKey).where(
        IntegrationKey.agent_id == "claude-code",
        IntegrationKey.kind == "tinker",
    )
    result = await session.execute(stmt)
    row = result.scalar_one()
    assert row.ciphertext != "sk-test-abc123"
    assert row.ciphertext.startswith("v1:")
    # Decryption must round-trip correctly.
    assert decrypt(row.ciphertext) == "sk-test-abc123"


async def test_put_key_idempotent_upsert(client: AsyncClient, session):
    """Second PUT for same agent+kind updates rather than inserting a new row."""
    await client.put(
        "/v1/integrations/keys/tinker",
        json={"value": "first-key"},
        headers=CC_HEADERS,
    )
    await client.put(
        "/v1/integrations/keys/tinker",
        json={"value": "second-key"},
        headers=CC_HEADERS,
    )
    from sqlalchemy import select, func
    stmt = select(func.count()).select_from(IntegrationKey).where(
        IntegrationKey.agent_id == "claude-code",
        IntegrationKey.kind == "tinker",
    )
    count = (await session.execute(stmt)).scalar_one()
    assert count == 1

    stmt2 = select(IntegrationKey).where(
        IntegrationKey.agent_id == "claude-code",
        IntegrationKey.kind == "tinker",
    )
    row = (await session.execute(stmt2)).scalar_one()
    assert decrypt(row.ciphertext) == "second-key"


async def test_put_key_invalid_kind(client: AsyncClient):
    resp = await client.put(
        "/v1/integrations/keys/unknown",
        json={"value": "abc"},
        headers=CC_HEADERS,
    )
    assert resp.status_code == 422


async def test_put_key_empty_value(client: AsyncClient):
    resp = await client.put(
        "/v1/integrations/keys/tinker",
        json={"value": "   "},
        headers=CC_HEADERS,
    )
    assert resp.status_code == 422


# ---------------------------------------------------------------------------
# GET — list (masked, never plaintext)
# ---------------------------------------------------------------------------


async def test_get_keys_returns_masked(client: AsyncClient):
    await client.put(
        "/v1/integrations/keys/tinker",
        json={"value": "sk-longkey-xxxxxxxxxxxxxxxx"},
        headers=CC_HEADERS,
    )
    resp = await client.get("/v1/integrations/keys", headers=CC_HEADERS)
    assert resp.status_code == 200
    keys = resp.json()
    assert len(keys) == 1
    masked = keys[0]["masked"]
    # Must not contain full key.
    assert "sk-longkey-xxxxxxxxxxxxxxxx" not in masked
    # First 4 chars of original should appear.
    assert masked.startswith("sk-l")
    assert masked.endswith("xxxx")


async def test_get_keys_short_value_masked_as_stars(client: AsyncClient):
    await client.put(
        "/v1/integrations/keys/tinker",
        json={"value": "short"},
        headers=CC_HEADERS,
    )
    resp = await client.get("/v1/integrations/keys", headers=CC_HEADERS)
    assert resp.status_code == 200
    assert resp.json()[0]["masked"] == "****"


async def test_get_keys_empty_for_new_agent(client: AsyncClient):
    resp = await client.get("/v1/integrations/keys", headers=CC_HEADERS)
    assert resp.status_code == 200
    assert resp.json() == []


# ---------------------------------------------------------------------------
# DELETE
# ---------------------------------------------------------------------------


async def test_delete_key(client: AsyncClient):
    await client.put(
        "/v1/integrations/keys/tinker",
        json={"value": "to-delete"},
        headers=CC_HEADERS,
    )
    resp = await client.delete("/v1/integrations/keys/tinker", headers=CC_HEADERS)
    assert resp.status_code == 204

    resp2 = await client.get("/v1/integrations/keys", headers=CC_HEADERS)
    assert resp2.json() == []


async def test_delete_key_not_found(client: AsyncClient):
    resp = await client.delete("/v1/integrations/keys/tinker", headers=CC_HEADERS)
    assert resp.status_code == 404


# ---------------------------------------------------------------------------
# Owner isolation — agent A's key not visible to agent B
# ---------------------------------------------------------------------------


async def test_owner_isolation(client: AsyncClient):
    await client.put(
        "/v1/integrations/keys/tinker",
        json={"value": "claude-secret-key"},
        headers=CC_HEADERS,
    )
    # openai agent should see no keys.
    resp = await client.get("/v1/integrations/keys", headers=OA_HEADERS)
    assert resp.status_code == 200
    assert resp.json() == []


async def test_owner_isolation_delete(client: AsyncClient):
    """Agent B cannot delete agent A's key."""
    await client.put(
        "/v1/integrations/keys/tinker",
        json={"value": "claude-secret-key"},
        headers=CC_HEADERS,
    )
    resp = await client.delete("/v1/integrations/keys/tinker", headers=OA_HEADERS)
    assert resp.status_code == 404


# ---------------------------------------------------------------------------
# 412 when no key and no env fallback
# ---------------------------------------------------------------------------


async def test_resolve_key_raises_when_no_key(monkeypatch):
    from app.core import config
    monkeypatch.setattr(config.settings, "tinker_api_key", "")

    with pytest.raises(TinkerKeyMissing):
        await _resolve_key(agent=None, session=None)


async def test_resolve_key_uses_env_fallback(monkeypatch):
    from app.core import config
    monkeypatch.setattr(config.settings, "tinker_api_key", "env-key-abc")

    result = await _resolve_key(agent=None, session=None)
    assert result == "env-key-abc"


async def test_create_run_412_when_no_tinker_key(client: AsyncClient, monkeypatch):
    """create_run returns 412 when TinkerKeyMissing is raised."""
    from app.core import config
    monkeypatch.setattr(config.settings, "tinker_api_key", "")

    # Replace fake_create_job (from conftest) with one that raises TinkerKeyMissing.
    import app.services.tinker as tinker_mod

    async def raise_missing(**kw):
        raise TinkerKeyMissing("no key")

    monkeypatch.setattr(tinker_mod.tinker, "create_job", raise_missing)

    payload = {
        "name": "Test Run",
        "base_model": "meta-llama/Llama-3-8b",
        "method": "sft",
        "hyperparams": {"lr": 1e-4, "epochs": 3},
        "dataset_mixture": [{"name": "alpaca", "weight": 1.0, "source": "huggingface"}],
        "gpu_type": "H100",
        "gpu_count": 1,
        "user_goal": "test",
        "user_context": "test",
        "agent_plan": "test",
        "citations": [],
    }
    resp = await client.post("/v1/runs/", json=payload, headers=CC_HEADERS)
    assert resp.status_code == 412
    data = resp.json()
    assert data["detail"]["error"] == "tinker_key_missing"


# ---------------------------------------------------------------------------
# last_used_at bumped on resolve
# ---------------------------------------------------------------------------


async def test_last_used_at_bumped(client: AsyncClient, session):
    """After _resolve_key uses the per-agent row, last_used_at is set."""
    await client.put(
        "/v1/integrations/keys/tinker",
        json={"value": "sk-bumped-key-xxxx"},
        headers=CC_HEADERS,
    )

    from sqlalchemy import select
    stmt = select(IntegrationKey).where(
        IntegrationKey.agent_id == "claude-code",
        IntegrationKey.kind == "tinker",
    )
    row = (await session.execute(stmt)).scalar_one()
    assert row.last_used_at is None

    # Call resolve directly with the same session.
    key = await _resolve_key(agent="claude-code", session=session)
    assert key == "sk-bumped-key-xxxx"

    await session.refresh(row)
    assert row.last_used_at is not None


# ---------------------------------------------------------------------------
# POST /test — hits right URL with right bearer (respx)
# ---------------------------------------------------------------------------


@respx.mock
async def test_key_tinker(client: AsyncClient, monkeypatch):
    """POST /test for tinker hits /jobs?limit=1 with the stored bearer."""
    from app.core import config
    monkeypatch.setattr(
        config.settings, "tinker_base_url", "https://api.tinker.thinkingmachines.ai/v1"
    )

    await client.put(
        "/v1/integrations/keys/tinker",
        json={"value": "sk-real-tinker-key"},
        headers=CC_HEADERS,
    )

    route = respx.get(
        "https://api.tinker.thinkingmachines.ai/v1/jobs",
    ).mock(return_value=Response(200, json={"jobs": []}))

    resp = await client.post("/v1/integrations/keys/tinker/test", headers=CC_HEADERS)
    assert resp.status_code == 200
    data = resp.json()
    assert data["ok"] is True
    assert "latency_ms" in data
    assert route.called
    # Verify the bearer was correct.
    sent_auth = route.calls[0].request.headers.get("authorization", "")
    assert sent_auth == "Bearer sk-real-tinker-key"


@respx.mock
async def test_key_openrouter(client: AsyncClient):
    """POST /test for openrouter hits OpenRouter models endpoint."""
    await client.put(
        "/v1/integrations/keys/openrouter",
        json={"value": "sk-or-test-key"},
        headers=CC_HEADERS,
    )

    route = respx.get("https://openrouter.ai/api/v1/models").mock(
        return_value=Response(200, json={"data": []})
    )

    resp = await client.post("/v1/integrations/keys/openrouter/test", headers=CC_HEADERS)
    assert resp.status_code == 200
    assert resp.json()["ok"] is True
    assert route.called
    sent_auth = route.calls[0].request.headers.get("authorization", "")
    assert sent_auth == "Bearer sk-or-test-key"


# ---------------------------------------------------------------------------
# Unauthenticated requests rejected
# ---------------------------------------------------------------------------


async def test_requires_auth(client: AsyncClient):
    resp = await client.get("/v1/integrations/keys")
    assert resp.status_code == 401
