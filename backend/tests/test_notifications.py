"""Tests for the notifications service and SSE endpoint."""

from __future__ import annotations

import asyncio
import json
import pytest

from app.services import notifications as notif_service


# ---------------------------------------------------------------------------
# Service unit tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_finished_event_delivered():
    """notify_run_finished pushes a run_finished event to the agent's queue."""
    # Reset module-level queue for this agent
    notif_service._queues.pop("agent-a", None)

    notif_service.notify_run_finished(
        agent="agent-a",
        run_id="run1",
        run_name="Test Run",
        status="failed",
        is_sandbox=False,
    )

    event = await notif_service.drain("agent-a", timeout=1.0)
    assert event is not None
    assert event["type"] == "run_finished"
    assert event["run_id"] == "run1"
    assert event["level"] == "error"


@pytest.mark.asyncio
async def test_sandbox_ready_event_type():
    """Succeeded sandbox emits sandbox_ready not run_finished."""
    notif_service._queues.pop("agent-b", None)

    notif_service.notify_run_finished(
        agent="agent-b",
        run_id="sbx1",
        run_name="My Sandbox",
        status="succeeded",
        is_sandbox=True,
    )

    event = await notif_service.drain("agent-b", timeout=1.0)
    assert event is not None
    assert event["type"] == "sandbox_ready"
    assert event["level"] == "info"


@pytest.mark.asyncio
async def test_cross_agent_isolation():
    """Events pushed for agent-x must NOT appear on agent-y's queue."""
    notif_service._queues.pop("agent-x", None)
    notif_service._queues.pop("agent-y", None)

    notif_service.notify_run_finished(
        agent="agent-x",
        run_id="runX",
        run_name="X Run",
        status="succeeded",
        is_sandbox=False,
    )

    # agent-y should time out with no event
    event_y = await notif_service.drain("agent-y", timeout=0.1)
    assert event_y is None

    # agent-x should receive the event
    event_x = await notif_service.drain("agent-x", timeout=1.0)
    assert event_x is not None
    assert event_x["run_id"] == "runX"


@pytest.mark.asyncio
async def test_alert_error_event():
    """notify_alert_error pushes alert_error event."""
    notif_service._queues.pop("agent-c", None)

    notif_service.notify_alert_error(
        agent="agent-c",
        run_id="run2",
        run_name="Alert Run",
        title="NaN detected in loss",
    )

    event = await notif_service.drain("agent-c", timeout=1.0)
    assert event is not None
    assert event["type"] == "alert_error"
    assert event["run_id"] == "run2"
    assert event["message"] == "NaN detected in loss"


@pytest.mark.asyncio
async def test_drain_returns_none_on_timeout():
    """drain() returns None when no event is available within timeout."""
    notif_service._queues.pop("agent-empty", None)
    result = await notif_service.drain("agent-empty", timeout=0.05)
    assert result is None


# ---------------------------------------------------------------------------
# SSE endpoint integration test
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_sse_endpoint_delivers_event(session, client, patch_settings):
    """SSE /stream delivers queued run_finished event as data: ... line."""
    from tests.conftest import CLAUDE_CODE_TOKEN

    notif_service._queues.pop("claude-code", None)

    # Pre-load an event before the request so the stream returns immediately
    notif_service.notify_run_finished(
        agent="claude-code",
        run_id="runSSE",
        run_name="SSE Run",
        status="succeeded",
        is_sandbox=False,
    )

    headers = {"Authorization": f"Bearer {CLAUDE_CODE_TOKEN}"}
    # Use a short stream read — collect first chunk
    async with client.stream("GET", "/v1/notifications/stream", headers=headers) as resp:
        assert resp.status_code == 200
        assert "text/event-stream" in resp.headers["content-type"]
        chunk = b""
        async for raw in resp.aiter_bytes():
            chunk += raw
            if b"data:" in chunk:
                break

    assert b"data:" in chunk
    # Extract the JSON payload
    for line in chunk.decode().splitlines():
        if line.startswith("data:"):
            payload = json.loads(line[len("data:"):].strip())
            assert payload["run_id"] == "runSSE"
            assert payload["type"] == "run_finished"
            break
    else:
        pytest.fail("No data: line found in SSE response")
