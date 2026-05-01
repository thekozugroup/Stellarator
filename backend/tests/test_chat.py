"""Tests for the in-system chat surface.

Strategy
--------
* Use respx to intercept the outbound OpenAI call.
* The fake response triggers one tool call (create_run) then a plain reply.
* The internal /v1/runs call is handled by the real in-process ASGI app.
* We assert the run is created in the DB owned by the right agent.
"""

from __future__ import annotations

import json
import pytest
import pytest_asyncio
import respx
from httpx import AsyncClient, Response
from sqlalchemy import select

from app.models.chat import ChatSession, ChatMessage
from app.models.run import Run

# Agent tokens (mirrors conftest patch_settings)
OPENAI_TOKEN = "test-token-openai"
CODEX_TOKEN = "test-token-codex"

_AUTH_OPENAI = {"Authorization": f"Bearer {OPENAI_TOKEN}"}


@pytest.fixture(autouse=True)
def patch_codex_token(monkeypatch):
    from app.core import config
    monkeypatch.setattr(config.settings, "agent_token_codex", CODEX_TOKEN)
    monkeypatch.setattr(config.settings, "openai_api_key", "sk-test-fake")


# ---------------------------------------------------------------------------
# Helpers: build fake OpenAI SSE streams
# ---------------------------------------------------------------------------

def _sse(obj: dict) -> bytes:
    return f"data: {json.dumps(obj)}\n\n".encode()


def _tool_call_stream(tool_name: str, tool_args: dict, call_id: str = "call_1") -> bytes:
    """Fake SSE stream with one tool call then done."""
    chunks = [
        # First chunk: assistant starts with a tool call
        {"choices": [{"delta": {"role": "assistant", "tool_calls": [
            {"index": 0, "id": call_id, "type": "function",
             "function": {"name": tool_name, "arguments": ""}}
        ]}, "finish_reason": None}]},
        # Argument chunk
        {"choices": [{"delta": {"tool_calls": [
            {"index": 0, "function": {"arguments": json.dumps(tool_args)}}
        ]}, "finish_reason": None}]},
        # Finish
        {"choices": [{"delta": {}, "finish_reason": "tool_calls"}]},
    ]
    body = b"".join(_sse(c) for c in chunks) + b"data: [DONE]\n\n"
    return body


def _text_stream(text: str) -> bytes:
    """Fake SSE stream returning plain text."""
    chunks = [
        {"choices": [{"delta": {"role": "assistant", "content": text}, "finish_reason": None}]},
        {"choices": [{"delta": {}, "finish_reason": "stop"}]},
    ]
    body = b"".join(_sse(c) for c in chunks) + b"data: [DONE]\n\n"
    return body


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_create_session(client: AsyncClient):
    r = await client.post(
        "/v1/chat/sessions",
        json={"system_prompt": "You are a helpful assistant."},
        headers=_AUTH_OPENAI,
    )
    assert r.status_code == 201
    data = r.json()
    assert data["agent"] == "openai"
    assert data["system_prompt"] == "You are a helpful assistant."
    assert "id" in data


@pytest.mark.asyncio
async def test_get_session_history(client: AsyncClient, session):
    # Create session directly in DB
    chat_session = ChatSession(
        id="sess-test-1",
        agent="openai",
        system_prompt="",
    )
    session.add(chat_session)
    msg = ChatMessage(session_id="sess-test-1", role="user", content="hello")
    session.add(msg)
    await session.commit()

    r = await client.get("/v1/chat/sessions/sess-test-1", headers=_AUTH_OPENAI)
    assert r.status_code == 200
    data = r.json()
    assert data["id"] == "sess-test-1"
    assert len(data["messages"]) == 1
    assert data["messages"][0]["content"] == "hello"


@pytest.mark.asyncio
async def test_get_session_403_wrong_agent(client: AsyncClient, session):
    chat_session = ChatSession(id="sess-other", agent="codex", system_prompt="")
    session.add(chat_session)
    await session.commit()

    r = await client.get("/v1/chat/sessions/sess-other", headers=_AUTH_OPENAI)
    assert r.status_code == 403


@pytest.mark.asyncio
async def test_tool_loop_create_run(client: AsyncClient, session, monkeypatch):
    """Full tool loop: agent calls create_run, run is persisted owned by 'openai'.

    The tool executor (agent_tools.execute) uses httpx against localhost:8000.
    We patch it to use the in-process ASGI client so no real network call is
    made for internal /v1 routes, while still intercepting the outbound OpenAI
    call with respx.
    """
    import app.agents.tools as agent_tools

    run_args = {
        "name": "Test Run",
        "base_model": "llama-3-8b",
        "method": "sft",
        "dataset_mixture": [{"name": "alpaca", "weight": 1.0, "source": "huggingface"}],
    }

    # Patch execute to route internal calls through the test ASGI client
    # and capture what was returned so we can assert on it.
    created_runs: list[dict] = []

    async def fake_execute(name: str, args: dict, agent_token: str) -> str:
        if name == "create_run":
            r = await client.post(
                "/v1/runs/",
                json=args,
                headers={"Authorization": f"Bearer {agent_token}"},
            )
            if r.status_code >= 400:
                return json.dumps({"error": f"create_run failed: {r.status_code} {r.text}"})
            data = r.json()
            created_runs.append(data)
            return r.text
        return json.dumps({"ok": True})

    monkeypatch.setattr(agent_tools, "execute", fake_execute)

    # Create a session
    create_r = await client.post(
        "/v1/chat/sessions",
        json={"system_prompt": ""},
        headers=_AUTH_OPENAI,
    )
    assert create_r.status_code == 201
    sess_id = create_r.json()["id"]

    # First OpenAI call → tool_call (create_run)
    first_response = Response(
        200,
        content=_tool_call_stream("create_run", run_args),
        headers={"content-type": "text/event-stream"},
    )
    # Second OpenAI call (after tool result) → plain text
    second_response = Response(
        200,
        content=_text_stream("Done! I created the run for you."),
        headers={"content-type": "text/event-stream"},
    )

    with respx.mock(assert_all_called=False) as mock:
        oai_route = mock.post("https://api.openai.com/v1/chat/completions").mock(
            side_effect=[first_response, second_response]
        )

        r = await client.post(
            f"/v1/chat/sessions/{sess_id}/messages",
            json={"content": "Create a test run", "driver": "openai", "model": "gpt-4o"},
            headers=_AUTH_OPENAI,
        )
        # SSE responses are 200
        assert r.status_code == 200

        # OpenAI was called at least once (tool loop)
        assert oai_route.call_count >= 1

    # Verify the run was created via the API, owned by 'openai'.
    # In-memory SQLite uses per-connection isolation, so we verify via the
    # captured API response rather than querying the test session directly.
    assert len(created_runs) >= 1, "create_run tool should have been called"
    run_data = created_runs[0]
    assert run_data.get("name") == "Test Run"
    assert run_data.get("owner_agent") == "openai"
    assert run_data.get("method") == "sft"


@pytest.mark.asyncio
async def test_session_not_found(client: AsyncClient):
    r = await client.post(
        "/v1/chat/sessions/nonexistent/messages",
        json={"content": "hello"},
        headers=_AUTH_OPENAI,
    )
    assert r.status_code == 404


@pytest.mark.asyncio
async def test_invalid_driver(client: AsyncClient, session):
    create_r = await client.post(
        "/v1/chat/sessions",
        json={"system_prompt": ""},
        headers=_AUTH_OPENAI,
    )
    sess_id = create_r.json()["id"]

    r = await client.post(
        f"/v1/chat/sessions/{sess_id}/messages",
        json={"content": "hi", "driver": "unknown"},
        headers=_AUTH_OPENAI,
    )
    assert r.status_code == 400


@pytest.mark.asyncio
async def test_unauthenticated(client: AsyncClient):
    r = await client.post("/v1/chat/sessions", json={})
    assert r.status_code == 401


@pytest.mark.asyncio
async def test_openai_oauth_driver_returns_410(client: AsyncClient, session):
    """driver='openai-oauth' must return 410 Gone (deprecated)."""
    create_r = await client.post(
        "/v1/chat/sessions",
        json={"system_prompt": ""},
        headers=_AUTH_OPENAI,
    )
    sess_id = create_r.json()["id"]

    r = await client.post(
        f"/v1/chat/sessions/{sess_id}/messages",
        json={"content": "hi", "driver": "openai-oauth"},
        headers=_AUTH_OPENAI,
    )
    assert r.status_code == 410
    assert "Codex" in r.json()["detail"]


@pytest.mark.asyncio
async def test_claude_code_driver_returns_501(client: AsyncClient, session):
    """driver='claude-code' must return 501 Not Implemented (use MCP)."""
    create_r = await client.post(
        "/v1/chat/sessions",
        json={"system_prompt": ""},
        headers=_AUTH_OPENAI,
    )
    sess_id = create_r.json()["id"]

    r = await client.post(
        f"/v1/chat/sessions/{sess_id}/messages",
        json={"content": "hi", "driver": "claude-code"},
        headers=_AUTH_OPENAI,
    )
    assert r.status_code == 501
    assert "MCP" in r.json()["detail"]
