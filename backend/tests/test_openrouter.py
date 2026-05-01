"""OpenRouter driver — tool loop, key resolution, etiquette headers, error mapping."""

from __future__ import annotations

import json
import os
from unittest.mock import patch

import httpx
import pytest
import respx

from app.agents import openrouter_driver
from app.agents.openrouter_driver import OpenRouterDriver


# ---------------------------------------------------------------------------
# Helpers — drain an async generator into a list of frames
# ---------------------------------------------------------------------------


async def _drain(gen) -> list[str]:
    out: list[str] = []
    async for chunk in gen:
        out.append(chunk)
    return out


def _events(frames: list[str]) -> list[tuple[str, dict]]:
    """Parse SSE frames into (event, payload) pairs (ignore comments/keepalives)."""
    events: list[tuple[str, dict]] = []
    for f in frames:
        if not f.startswith("event: "):
            continue
        head, _, rest = f.partition("\ndata: ")
        evt = head[len("event: "):].strip()
        data_line = rest.split("\n")[0]
        try:
            events.append((evt, json.loads(data_line)))
        except json.JSONDecodeError:
            pass
    return events


# ---------------------------------------------------------------------------
# 1. Headers include Referer + Title; key resolved from integrations helper.
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_headers_and_key_resolution(monkeypatch):
    captured_headers: dict = {}

    async def _fake_resolve(agent_id: str, kind: str = "openrouter") -> str:
        assert agent_id == "agent-A"
        assert kind == "openrouter"
        return "OR-key-123"

    # Patch the local resolver path AND the optional integrations module.
    monkeypatch.setattr(openrouter_driver, "_resolve_openrouter_key",
                        lambda aid: _fake_resolve(aid))

    sse_body = (
        b'data: {"choices":[{"delta":{"content":"hi"},"finish_reason":"stop"}]}\n\n'
        b"data: [DONE]\n\n"
    )

    def _handler(request: httpx.Request) -> httpx.Response:
        captured_headers.update(dict(request.headers))
        return httpx.Response(200, content=sse_body, headers={"content-type": "text/event-stream"})

    with respx.mock(assert_all_called=True) as router:
        router.post(f"{openrouter_driver.OPENROUTER_BASE}/chat/completions").mock(
            side_effect=_handler
        )
        drv = OpenRouterDriver()
        frames = await _drain(
            drv._run([{"role": "user", "content": "hi"}],
                     agent_id="agent-A", agent_token="tok", model="openrouter/auto")
        )

    assert captured_headers.get("authorization") == "Bearer OR-key-123"
    assert captured_headers.get("http-referer") == "https://stellarator.dev"
    assert captured_headers.get("x-title") == "Stellarator"

    events = _events(frames)
    kinds = [e for e, _ in events]
    assert "delta" in kinds
    assert "done" in kinds


# ---------------------------------------------------------------------------
# 2. Tool loop fires — model returns tool_calls then a final answer.
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_tool_loop_fires(monkeypatch):
    monkeypatch.setattr(
        openrouter_driver,
        "_resolve_openrouter_key",
        lambda _aid: _coro("KEY"),
    )

    # Stub agent_tools.execute and tools list.
    from app.agents import tools as agent_tools

    async def _fake_execute(name, args, token):
        return json.dumps({"ok": True, "name": name})

    monkeypatch.setattr(agent_tools, "execute", _fake_execute)
    monkeypatch.setattr(agent_tools, "TOOLS", [])

    # First upstream response: a tool_call. Second: a final answer.
    first_body = (
        b'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1",'
        b'"function":{"name":"ping","arguments":"{}"}}]}}]}\n\n'
        b"data: [DONE]\n\n"
    )
    second_body = (
        b'data: {"choices":[{"delta":{"content":"final"},"finish_reason":"stop"}]}\n\n'
        b"data: [DONE]\n\n"
    )

    responses = iter([
        httpx.Response(200, content=first_body, headers={"content-type": "text/event-stream"}),
        httpx.Response(200, content=second_body, headers={"content-type": "text/event-stream"}),
    ])

    def _handler(request: httpx.Request) -> httpx.Response:
        return next(responses)

    with respx.mock(assert_all_called=True) as router:
        router.post(f"{openrouter_driver.OPENROUTER_BASE}/chat/completions").mock(
            side_effect=_handler
        )
        drv = OpenRouterDriver()
        frames = await _drain(
            drv._run([{"role": "user", "content": "ping me"}],
                     agent_id="A", agent_token="t", model="openrouter/auto")
        )

    events = _events(frames)
    kinds = [e for e, _ in events]
    assert "tool_call" in kinds
    assert "tool_result" in kinds
    assert "delta" in kinds
    assert kinds[-1] == "done"


# ---------------------------------------------------------------------------
# 3. MAX_ITER env honoured (read at module load).
# ---------------------------------------------------------------------------


def test_max_iter_env_honoured(monkeypatch):
    monkeypatch.setenv("STELLARATOR_AGENT_MAX_ITER", "7")
    import importlib

    reloaded = importlib.reload(openrouter_driver)
    try:
        assert reloaded.MAX_ITER == 7
    finally:
        # Restore default for subsequent tests.
        monkeypatch.delenv("STELLARATOR_AGENT_MAX_ITER", raising=False)
        importlib.reload(reloaded)


# ---------------------------------------------------------------------------
# 4. Upstream 401 surfaces as a 502-flavoured error event.
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_upstream_401_surfaces_as_502(monkeypatch):
    monkeypatch.setattr(
        openrouter_driver,
        "_resolve_openrouter_key",
        lambda _aid: _coro("KEY"),
    )

    with respx.mock(assert_all_called=True) as router:
        router.post(f"{openrouter_driver.OPENROUTER_BASE}/chat/completions").mock(
            return_value=httpx.Response(401, json={"error": "no"})
        )
        drv = OpenRouterDriver()
        frames = await _drain(
            drv._run([{"role": "user", "content": "hi"}],
                     agent_id="A", agent_token="t", model="openrouter/auto")
        )

    events = _events(frames)
    error_events = [(e, p) for e, p in events if e == "error"]
    assert error_events, "expected an SSE error event for upstream 401"
    _, payload = error_events[0]
    assert payload.get("status") == 502


# ---------------------------------------------------------------------------
# Tiny helper for awaitable lambdas
# ---------------------------------------------------------------------------


async def _coro(value):
    return value
