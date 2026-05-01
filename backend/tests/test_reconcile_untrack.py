"""Tests: reconcile untrack on terminal status."""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock, patch

import httpx
import pytest

from app.services.reconcile import _untrack, _TERMINAL
from app.models.run import RunStatus


def test_terminal_set_contents() -> None:
    assert "succeeded" in _TERMINAL
    assert "failed" in _TERMINAL
    assert "cancelled" in _TERMINAL
    assert "running" not in _TERMINAL
    assert "queued" not in _TERMINAL


@pytest.mark.asyncio
async def test_untrack_posts_correct_url() -> None:
    """_untrack POSTs to /supervisor/untrack/{run_id} with token header."""
    mock_response = MagicMock()
    mock_response.raise_for_status = MagicMock()

    client = AsyncMock(spec=httpx.AsyncClient)
    client.post = AsyncMock(return_value=mock_response)

    await _untrack(client, "run-abc123", supervisor_token="secret-token")

    client.post.assert_awaited_once_with(
        "http://supervisor:8001/supervisor/untrack/run-abc123",
        headers={"X-Supervisor-Token": "secret-token"},
    )


@pytest.mark.asyncio
async def test_untrack_no_token_omits_header() -> None:
    mock_response = MagicMock()
    mock_response.raise_for_status = MagicMock()

    client = AsyncMock(spec=httpx.AsyncClient)
    client.post = AsyncMock(return_value=mock_response)

    await _untrack(client, "run-xyz", supervisor_token=None)

    _, kwargs = client.post.call_args
    assert "X-Supervisor-Token" not in kwargs.get("headers", {})


@pytest.mark.asyncio
async def test_untrack_failure_logs_warning(caplog: pytest.LogCaptureFixture) -> None:
    """_untrack swallows errors and logs a warning."""
    import logging

    client = AsyncMock(spec=httpx.AsyncClient)
    client.post = AsyncMock(side_effect=httpx.ConnectError("connection refused"))

    with caplog.at_level(logging.WARNING, logger="app.services.reconcile"):
        await _untrack(client, "run-fail", supervisor_token=None)

    assert any("Untrack failed" in r.message for r in caplog.records)


@pytest.mark.asyncio
async def test_reconcile_once_calls_untrack_for_terminal_tracked_run(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """When a run transitions to terminal and is in tracked set, untrack is called."""
    from app.services import reconcile as rec_mod
    from app.models.run import RunStatus

    # Build a fake run that looks terminal after refresh.
    fake_run = MagicMock()
    fake_run.id = "run-terminal"
    fake_run.tinker_job_id = "job-1"
    fake_run.status = RunStatus.running.value  # starts as running in DB

    async def fake_session_execute(_stmt: object) -> MagicMock:
        result = MagicMock()
        result.scalars.return_value.all.return_value = [fake_run]
        return result

    fake_session = AsyncMock()
    fake_session.execute = fake_session_execute
    fake_session.commit = AsyncMock()

    class _FakeSessionCtx:
        async def __aenter__(self) -> AsyncMock:
            return fake_session

        async def __aexit__(self, *_: object) -> None:
            pass

    monkeypatch.setattr(rec_mod, "SessionLocal", lambda: _FakeSessionCtx())

    untrack_calls: list[str] = []

    async def fake_untrack(
        client: object, run_id: str, supervisor_token: object
    ) -> None:
        untrack_calls.append(run_id)

    async def fake_list_supervisor_tracked(
        client: object, supervisor_token: object
    ) -> set[str]:
        return {"run-terminal"}

    async def fake_refresh(run: object) -> str | None:
        fake_run.status = RunStatus.succeeded.value
        return RunStatus.succeeded.value

    monkeypatch.setattr(rec_mod, "_list_supervisor_tracked", fake_list_supervisor_tracked)
    monkeypatch.setattr(rec_mod, "_refresh_run_status", fake_refresh)
    monkeypatch.setattr(rec_mod, "_untrack", fake_untrack)

    tinker_failures: dict[str, int] = {}
    await rec_mod._reconcile_once(
        supervisor_token="tok", tinker_failures=tinker_failures, base_interval=30.0
    )

    assert "run-terminal" in untrack_calls, "Expected untrack to be called for terminal run"
