"""Tests: _hand_off_to_supervisor error routing."""

from __future__ import annotations

import asyncio
from unittest.mock import AsyncMock, MagicMock, patch, call

import httpx
import pytest


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_response(status_code: int) -> MagicMock:
    resp = MagicMock(spec=httpx.Response)
    resp.status_code = status_code

    def _raise_for_status() -> None:
        if status_code >= 400:
            raise httpx.HTTPStatusError(
                f"HTTP {status_code}",
                request=MagicMock(),
                response=resp,
            )

    resp.raise_for_status = _raise_for_status
    return resp


# ---------------------------------------------------------------------------
# 401/403 — misconfiguration: roll back + 502
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_handoff_401_raises_supervisor_misconfigured(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from app.api.runs import _hand_off_to_supervisor, _SupervisorMisconfigured

    mock_client = AsyncMock()
    mock_client.post = AsyncMock(return_value=_make_response(401))

    with patch("app.api.runs.httpx.AsyncClient") as MockCls:
        MockCls.return_value.__aenter__ = AsyncMock(return_value=mock_client)
        MockCls.return_value.__aexit__ = AsyncMock(return_value=False)

        with pytest.raises(_SupervisorMisconfigured):
            await _hand_off_to_supervisor("run-1", "job-1")


@pytest.mark.asyncio
async def test_handoff_403_raises_supervisor_misconfigured(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from app.api.runs import _hand_off_to_supervisor, _SupervisorMisconfigured

    mock_client = AsyncMock()
    mock_client.post = AsyncMock(return_value=_make_response(403))

    with patch("app.api.runs.httpx.AsyncClient") as MockCls:
        MockCls.return_value.__aenter__ = AsyncMock(return_value=mock_client)
        MockCls.return_value.__aexit__ = AsyncMock(return_value=False)

        with pytest.raises(_SupervisorMisconfigured):
            await _hand_off_to_supervisor("run-1", "job-1")


@pytest.mark.asyncio
async def test_handoff_401_logs_secret_mismatch_hint(
    monkeypatch: pytest.MonkeyPatch,
    caplog: pytest.LogCaptureFixture,
) -> None:
    import logging
    from app.api.runs import _hand_off_to_supervisor, _SupervisorMisconfigured

    mock_client = AsyncMock()
    mock_client.post = AsyncMock(return_value=_make_response(401))

    with patch("app.api.runs.httpx.AsyncClient") as MockCls:
        MockCls.return_value.__aenter__ = AsyncMock(return_value=mock_client)
        MockCls.return_value.__aexit__ = AsyncMock(return_value=False)

        with caplog.at_level(logging.ERROR, logger="app.api.runs"):
            with pytest.raises(_SupervisorMisconfigured):
                await _hand_off_to_supervisor("run-err", "job-err")

    assert any("shared secret" in r.message.lower() for r in caplog.records), (
        "Expected 'shared secret' hint in error log"
    )


# ---------------------------------------------------------------------------
# 5xx — retry 3 times then continue (no rollback)
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_handoff_503_retries_3_times_then_continues(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from app.api.runs import _hand_off_to_supervisor

    call_count = 0

    async def fast_sleep(_: float) -> None:
        pass

    monkeypatch.setattr(asyncio, "sleep", fast_sleep)

    def _post_side_effect(*_a: object, **_kw: object) -> MagicMock:
        nonlocal call_count
        call_count += 1
        return _make_response(503)

    mock_client = AsyncMock()
    mock_client.post = AsyncMock(side_effect=_post_side_effect)

    with patch("app.api.runs.httpx.AsyncClient") as MockCls:
        MockCls.return_value.__aenter__ = AsyncMock(return_value=mock_client)
        MockCls.return_value.__aexit__ = AsyncMock(return_value=False)

        # Should NOT raise — function returns after exhausting retries.
        await _hand_off_to_supervisor("run-5xx", "job-5xx")

    assert call_count == 4, f"Expected 4 total attempts (1 + 3 retries), got {call_count}"


@pytest.mark.asyncio
async def test_handoff_network_error_retries_then_continues(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from app.api.runs import _hand_off_to_supervisor

    call_count = 0

    async def fast_sleep(_: float) -> None:
        pass

    monkeypatch.setattr(asyncio, "sleep", fast_sleep)

    async def _post_fail(*_a: object, **_kw: object) -> None:
        nonlocal call_count
        call_count += 1
        raise httpx.ConnectError("connection refused")

    mock_client = AsyncMock()
    mock_client.post = _post_fail  # type: ignore[assignment]

    with patch("app.api.runs.httpx.AsyncClient") as MockCls:
        MockCls.return_value.__aenter__ = AsyncMock(return_value=mock_client)
        MockCls.return_value.__aexit__ = AsyncMock(return_value=False)

        await _hand_off_to_supervisor("run-net", "job-net")

    assert call_count == 4, f"Expected 4 total attempts, got {call_count}"


# ---------------------------------------------------------------------------
# 2xx — returns immediately without retry
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_handoff_2xx_returns_immediately(monkeypatch: pytest.MonkeyPatch) -> None:
    from app.api.runs import _hand_off_to_supervisor

    mock_client = AsyncMock()
    mock_client.post = AsyncMock(return_value=_make_response(200))

    with patch("app.api.runs.httpx.AsyncClient") as MockCls:
        MockCls.return_value.__aenter__ = AsyncMock(return_value=mock_client)
        MockCls.return_value.__aexit__ = AsyncMock(return_value=False)

        await _hand_off_to_supervisor("run-ok", "job-ok")

    assert mock_client.post.call_count == 1, "Should only call once on 2xx"
