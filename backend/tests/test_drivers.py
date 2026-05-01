"""Tests for openai_driver and codex_driver hardening.

Covers:
- MAX_ITER cap logs warning and returns final state.
- Keep-alive ping appears after 15 s of idle (asyncio time mocking).
- Tool loop forwards bearer token to internal /v1.
- SSE wire format compliance (event/data/id fields).
- CancelledError cleanly aborts the stream.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
from typing import AsyncGenerator
from unittest.mock import AsyncMock, MagicMock, patch

import httpx
import pytest
import pytest_asyncio
import respx

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

os.environ.setdefault("STELLARATOR_AGENT_MAX_ITER", "2")  # keep tests fast


def _make_sse_chunk(content: str | None = None, tool_calls: list | None = None) -> str:
    """Return a fake OpenAI streaming data line."""
    delta: dict = {}
    if content:
        delta["content"] = content
    if tool_calls:
        delta["tool_calls"] = tool_calls
    chunk = {
        "choices": [{"delta": delta, "finish_reason": None}]
    }
    return f"data: {json.dumps(chunk)}\n"


_DONE_LINE = "data: [DONE]\n"


def _lines_to_stream(*lines: str) -> AsyncGenerator[str, None]:
    """Return an async generator that yields each line."""
    async def _gen():
        for line in lines:
            yield line
    return _gen()


def _parse_sse(frame: str) -> dict:
    """Parse an SSE frame into {event, data, id}."""
    result: dict = {}
    for line in frame.strip().split("\n"):
        if line.startswith("event:"):
            result["event"] = line[len("event:"):].strip()
        elif line.startswith("data:"):
            result["data"] = json.loads(line[len("data:"):].strip())
        elif line.startswith("id:"):
            result["id"] = line[len("id:"):].strip()
    return result


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

AGENT_TOKEN = "test-bearer-xyz"
INTERNAL_BASE = "http://localhost:8000"


@pytest.fixture(autouse=True)
def patch_openai_key(monkeypatch):
    from app.core import config
    monkeypatch.setattr(config.settings, "openai_api_key", "sk-test")


# ---------------------------------------------------------------------------
# Import drivers AFTER env is set
# ---------------------------------------------------------------------------

from app.agents import openai_driver as od  # noqa: E402


# ---------------------------------------------------------------------------
# SSE format compliance
# ---------------------------------------------------------------------------


@respx.mock
@pytest.mark.asyncio
async def test_sse_format_delta(monkeypatch):
    """delta events must have event:, data:, and id: fields."""
    monkeypatch.setenv("STELLARATOR_AGENT_MAX_ITER", "1")
    # Reload module-level MAX_ITER.
    import importlib
    importlib.reload(od)

    content_chunk = _make_sse_chunk(content="Hello")
    raw_lines = [content_chunk, _DONE_LINE]

    mock_response = MagicMock()
    mock_response.raise_for_status = MagicMock()
    mock_response.__aenter__ = AsyncMock(return_value=mock_response)
    mock_response.__aexit__ = AsyncMock(return_value=False)
    mock_response.aiter_lines = MagicMock(return_value=_lines_to_stream(*raw_lines))

    mock_client = MagicMock()
    mock_client.__aenter__ = AsyncMock(return_value=mock_client)
    mock_client.__aexit__ = AsyncMock(return_value=False)
    mock_client.stream = MagicMock(return_value=mock_response)

    with patch("app.agents.openai_driver.httpx.AsyncClient", return_value=mock_client):
        driver = od.OpenAIDriver()
        gen = await driver.chat([{"role": "user", "content": "hi"}], AGENT_TOKEN)
        frames = [f async for f in gen]

    delta_frames = [f for f in frames if f.startswith("event: delta")]
    assert delta_frames, "No delta frame emitted"
    parsed = _parse_sse(delta_frames[0])
    assert parsed["event"] == "delta"
    assert "content" in parsed["data"]
    assert "id" in parsed


@respx.mock
@pytest.mark.asyncio
async def test_sse_format_done(monkeypatch):
    """done event must have proper SSE shape."""
    monkeypatch.setenv("STELLARATOR_AGENT_MAX_ITER", "1")
    import importlib
    importlib.reload(od)

    raw_lines = [_make_sse_chunk(content="ok"), _DONE_LINE]

    mock_response = MagicMock()
    mock_response.raise_for_status = MagicMock()
    mock_response.__aenter__ = AsyncMock(return_value=mock_response)
    mock_response.__aexit__ = AsyncMock(return_value=False)
    mock_response.aiter_lines = MagicMock(return_value=_lines_to_stream(*raw_lines))

    mock_client = MagicMock()
    mock_client.__aenter__ = AsyncMock(return_value=mock_client)
    mock_client.__aexit__ = AsyncMock(return_value=False)
    mock_client.stream = MagicMock(return_value=mock_response)

    with patch("app.agents.openai_driver.httpx.AsyncClient", return_value=mock_client):
        driver = od.OpenAIDriver()
        gen = await driver.chat([{"role": "user", "content": "hi"}], AGENT_TOKEN)
        frames = [f async for f in gen]

    done_frames = [f for f in frames if "event: done" in f]
    assert done_frames, "No done frame emitted"
    parsed = _parse_sse(done_frames[0])
    assert parsed["event"] == "done"
    assert "id" in parsed


# ---------------------------------------------------------------------------
# MAX_ITER cap
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_max_iter_logs_warning(monkeypatch, caplog):
    """When MAX_ITER is hit, a structured warning is logged."""
    monkeypatch.setenv("STELLARATOR_AGENT_MAX_ITER", "1")
    import importlib
    importlib.reload(od)

    # Tool-call response so loop never terminates via "no tool calls".
    tc_delta = {
        "index": 0,
        "id": "call-1",
        "function": {"name": "list_runs", "arguments": "{}"},
    }
    tool_chunk = _make_sse_chunk(tool_calls=[tc_delta])
    raw_lines = [tool_chunk, _DONE_LINE]

    mock_response = MagicMock()
    mock_response.raise_for_status = MagicMock()
    mock_response.__aenter__ = AsyncMock(return_value=mock_response)
    mock_response.__aexit__ = AsyncMock(return_value=False)
    mock_response.aiter_lines = MagicMock(return_value=_lines_to_stream(*raw_lines))

    mock_client = MagicMock()
    mock_client.__aenter__ = AsyncMock(return_value=mock_client)
    mock_client.__aexit__ = AsyncMock(return_value=False)
    mock_client.stream = MagicMock(return_value=mock_response)

    async def fake_execute(name, args, token):
        return json.dumps({"runs": []})

    with (
        patch("app.agents.openai_driver.httpx.AsyncClient", return_value=mock_client),
        patch("app.agents.openai_driver.agent_tools.execute", side_effect=fake_execute),
        caplog.at_level(logging.WARNING, logger="app.agents.openai_driver"),
    ):
        driver = od.OpenAIDriver()
        gen = await driver.chat([{"role": "user", "content": "list runs"}], AGENT_TOKEN)
        frames = [f async for f in gen]

    assert any("MAX_ITER" in r.message for r in caplog.records), (
        "Expected MAX_ITER warning in logs"
    )
    # Must still emit a done frame.
    assert any("event: done" in f for f in frames)


# ---------------------------------------------------------------------------
# Keep-alive ping
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_keepalive_ping_emitted():
    """Ping comment frame is emitted after PING_INTERVAL_S of idle."""
    PING_INTERVAL = 0.05  # short for tests

    async def slow_gen() -> AsyncGenerator[str, None]:
        await asyncio.sleep(PING_INTERVAL * 3)
        yield "event: done\ndata: {}\nid: 1\n\n"

    frames: list[str] = []
    async for chunk in od._with_keepalive(slow_gen(), interval=PING_INTERVAL):
        frames.append(chunk)

    pings = [f for f in frames if f == ": ping\n\n"]
    assert pings, "Expected at least one ping frame"
    assert frames[-1] != ": ping\n\n", "Last frame should be the real chunk, not a ping"


# ---------------------------------------------------------------------------
# Bearer token forwarded to internal /v1
# ---------------------------------------------------------------------------


@respx.mock
@pytest.mark.asyncio
async def test_tool_loop_forwards_bearer_token(monkeypatch):
    """agent_tools.execute must call /v1 with the caller's Authorization header."""
    monkeypatch.setenv("STELLARATOR_INTERNAL_BASE_URL", INTERNAL_BASE)
    import importlib
    from app.agents import tools as agent_tools
    importlib.reload(agent_tools)

    route = respx.get(f"{INTERNAL_BASE}/v1/runs").mock(
        return_value=httpx.Response(200, json=[])
    )

    result = await agent_tools.execute("list_runs", {}, AGENT_TOKEN)
    assert result == "[]"

    auth = route.calls.last.request.headers.get("authorization", "")
    assert auth == f"Bearer {AGENT_TOKEN}", f"Expected bearer token, got: {auth!r}"


# ---------------------------------------------------------------------------
# CancelledError propagates cleanly
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_cancellation_aborts_stream():
    """CancelledError from the consumer propagates through _with_keepalive cleanly."""

    async def infinite_gen() -> AsyncGenerator[str, None]:
        while True:
            await asyncio.sleep(0.01)
            yield "event: delta\ndata: {}\nid: 1\n\n"

    async def consume():
        count = 0
        async for _ in od._with_keepalive(infinite_gen(), interval=60.0):
            count += 1
            if count >= 2:
                raise asyncio.CancelledError

    with pytest.raises(asyncio.CancelledError):
        await consume()
