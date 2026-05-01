"""Tests for stellarator_mcp.config base-URL resolution.

Covers:
- Env override wins over probe.
- Docker DNS failure falls back to localhost within timeout.
- Bearer token never appears in stringified errors / log output.
"""

from __future__ import annotations

import asyncio
import os
import logging
from unittest.mock import AsyncMock, patch

import pytest

# Ensure token is present for any incidental client imports.
os.environ.setdefault("STELLARATOR_TOKEN", "test-token-abc")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _reset():
    """Reset the config cache between tests."""
    import stellarator_mcp.config as cfg
    cfg.reset_cache()


# ---------------------------------------------------------------------------
# Env override
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_env_override_wins(monkeypatch):
    """STELLARATOR_BASE_URL env var takes priority; no TCP probe should run."""
    _reset()
    monkeypatch.setenv("STELLARATOR_BASE_URL", "http://custom-host:9999")

    import stellarator_mcp.config as cfg

    probe_called = False

    async def fake_tcp(host, port, timeout):
        nonlocal probe_called
        probe_called = True
        return False

    with patch.object(cfg, "_tcp_reachable", side_effect=fake_tcp):
        url = await cfg.get_base_url()

    assert url == "http://custom-host:9999"
    assert not probe_called, "TCP probe should not run when env var is set"
    _reset()


@pytest.mark.asyncio
async def test_env_override_cached(monkeypatch):
    """Second call returns cached result without re-reading env."""
    _reset()
    monkeypatch.setenv("STELLARATOR_BASE_URL", "http://cached-host:1234")

    import stellarator_mcp.config as cfg

    first = await cfg.get_base_url()
    # Mutate env — should not affect cached value.
    monkeypatch.setenv("STELLARATOR_BASE_URL", "http://changed-host:5678")
    second = await cfg.get_base_url()

    assert first == second == "http://cached-host:1234"
    _reset()


# ---------------------------------------------------------------------------
# Docker DNS failure → localhost fallback
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_docker_failure_falls_back(monkeypatch):
    """When docker probe fails, localhost URL is returned within probe timeout."""
    _reset()
    monkeypatch.delenv("STELLARATOR_BASE_URL", raising=False)

    import stellarator_mcp.config as cfg

    async def fail_probe(host, port, timeout):
        # Simulate timeout — should complete well within 1 s in tests.
        assert timeout == cfg._PROBE_TIMEOUT_S
        return False

    with patch.object(cfg, "_tcp_reachable", side_effect=fail_probe):
        url = await cfg.get_base_url()

    assert url == cfg._LOCALHOST_URL
    _reset()


@pytest.mark.asyncio
async def test_docker_success_uses_docker_url(monkeypatch):
    """When docker probe succeeds, the docker URL is used."""
    _reset()
    monkeypatch.delenv("STELLARATOR_BASE_URL", raising=False)

    import stellarator_mcp.config as cfg

    async def succeed_probe(host, port, timeout):
        return True

    with patch.object(cfg, "_tcp_reachable", side_effect=succeed_probe):
        url = await cfg.get_base_url()

    assert url == cfg._DOCKER_URL
    _reset()


@pytest.mark.asyncio
async def test_docker_probe_respects_timeout(monkeypatch):
    """Docker probe calls _tcp_reachable with _PROBE_TIMEOUT_S, not an arbitrary value."""
    _reset()
    monkeypatch.delenv("STELLARATOR_BASE_URL", raising=False)

    import stellarator_mcp.config as cfg

    seen_timeouts: list[float] = []

    async def record_probe(host, port, timeout):
        seen_timeouts.append(timeout)
        return False

    with patch.object(cfg, "_tcp_reachable", side_effect=record_probe):
        await cfg.get_base_url()

    assert seen_timeouts == [cfg._PROBE_TIMEOUT_S]
    _reset()


# ---------------------------------------------------------------------------
# Bearer token never in error output
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_token_not_leaked_in_resolution_logs(monkeypatch, caplog):
    """Resolution log messages must not contain the bearer token."""
    _reset()
    secret_token = "super-secret-bearer-9999"
    monkeypatch.setenv("STELLARATOR_TOKEN", secret_token)
    monkeypatch.delenv("STELLARATOR_BASE_URL", raising=False)

    import stellarator_mcp.config as cfg

    async def fail_probe(host, port, timeout):
        return False

    with (
        patch.object(cfg, "_tcp_reachable", side_effect=fail_probe),
        caplog.at_level(logging.DEBUG, logger="stellarator_mcp.config"),
    ):
        await cfg.get_base_url()

    for record in caplog.records:
        assert secret_token not in record.getMessage(), (
            f"Bearer token leaked in log: {record.getMessage()!r}"
        )
    _reset()


def test_token_not_in_map_error_string():
    """_map_error on the client must not include the Authorization header value."""
    import httpx
    from stellarator_mcp.client import _map_error

    secret = "bearer-leak-test-token"
    req = httpx.Request("GET", "http://localhost:8000/v1/runs", headers={"Authorization": f"Bearer {secret}"})
    resp = httpx.Response(401, request=req)

    error_msg = _map_error(resp)
    assert secret not in error_msg, f"Bearer token leaked in error: {error_msg!r}"
    assert "401" in error_msg
    assert "STELLARATOR_TOKEN" in error_msg
