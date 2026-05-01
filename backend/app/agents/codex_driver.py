"""Codex OAuth driver for Stellarator chat agents.

API choice: we call ``POST /v1/chat/completions`` on the Codex endpoint
(``https://api.openai.com/v1`` by default for Codex CLI compatibility, but
overridable via CODEX_BASE_URL env var).  The Codex /v1/responses endpoint is
specific to the Responses API (assistants-style); we prefer the chat
completions surface because it is identical in shape to the OpenAI driver,
making the loop reusable and the codebase consistent.

Token lifecycle:
  - Access tokens are stored in the ``codex_tokens`` table keyed by agent_id.
  - Before each request we check ``expires_at``; if within 60 s we refresh
    using the stored refresh_token and the configured OAuth credentials.
  - Never log token values.

SSE wire format:
    event: <type>\ndata: <json>\nid: <monotonic>\n\n

Event types: delta, tool_call, tool_result, done, error.

Keep-alives: ``: ping\\n\\n`` comment frames are emitted every PING_INTERVAL_S
seconds of idle so upstream proxies do not reap the connection.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
from datetime import datetime, timedelta, timezone
from typing import AsyncGenerator

import httpx
from sqlalchemy import select

from app.core.config import settings
from app.core.db import SessionLocal
from app.models.chat import CodexToken
from app.services.crypto import decrypt, encrypt

from . import doom_loop, tools as agent_tools
from .persistence import ChunkPersister

logger = logging.getLogger(__name__)

MAX_ITER: int = int(os.environ.get("STELLARATOR_AGENT_MAX_ITER", "12"))
CODEX_BASE = os.environ.get("CODEX_BASE_URL", "https://api.openai.com/v1")
TOKEN_URL = "https://auth.openai.com/oauth/token"
PING_INTERVAL_S = 15.0
_REFRESH_BUFFER_SECS = 60

# ---------------------------------------------------------------------------
# SSE helpers
# ---------------------------------------------------------------------------

_sse_counter: int = 0


def _next_id() -> int:
    global _sse_counter  # noqa: PLW0603
    _sse_counter += 1
    return _sse_counter


def _sse(event: str, payload: dict) -> str:
    """Format a proper SSE frame."""
    return f"event: {event}\ndata: {json.dumps(payload)}\nid: {_next_id()}\n\n"


async def _with_keepalive(
    source: AsyncGenerator[str, None],
    interval: float = PING_INTERVAL_S,
) -> AsyncGenerator[str, None]:
    """Wrap *source* yielding ``: ping\\n\\n`` comment frames every *interval* s of idle."""
    pending: asyncio.Task[str | None] | None = None

    async def _anext_or_none() -> str | None:
        try:
            return await source.__anext__()  # type: ignore[attr-defined]
        except StopAsyncIteration:
            return None

    try:
        while True:
            pending = asyncio.ensure_future(_anext_or_none())
            try:
                chunk = await asyncio.wait_for(asyncio.shield(pending), timeout=interval)
            except asyncio.TimeoutError:
                yield ": ping\n\n"
                chunk = await pending
            if chunk is None:
                break
            yield chunk
    except asyncio.CancelledError:
        if pending is not None and not pending.done():
            pending.cancel()
        raise


async def _get_access_token(agent_id: str) -> str:
    """Return a valid access token for agent_id, refreshing if needed."""
    async with SessionLocal() as session:
        result = await session.execute(
            select(CodexToken).where(CodexToken.agent_id == agent_id)
        )
        row: CodexToken | None = result.scalar_one_or_none()

    if row is None:
        raise RuntimeError(
            f"No Codex OAuth token found for agent '{agent_id}'. "
            "Complete the OAuth flow at /v1/oauth/codex/start first."
        )

    now = datetime.now(tz=timezone.utc)
    expires_soon = (
        row.expires_at is not None
        and row.expires_at.replace(tzinfo=timezone.utc) - now
        < timedelta(seconds=_REFRESH_BUFFER_SECS)
    )

    # Tokens are stored encrypted at rest; decrypt on read.
    plaintext_refresh = decrypt(row.refresh_token) if row.refresh_token else ""

    if expires_soon and plaintext_refresh:
        try:
            async with httpx.AsyncClient(timeout=30.0) as client:
                r = await client.post(
                    TOKEN_URL,
                    data={
                        "grant_type": "refresh_token",
                        "refresh_token": plaintext_refresh,
                        "client_id": settings.codex_oauth_client_id,
                        "client_secret": settings.codex_oauth_client_secret,
                    },
                    headers={"Content-Type": "application/x-www-form-urlencoded"},
                )
                r.raise_for_status()
                token_data = r.json()
        except httpx.HTTPError as exc:
            raise RuntimeError(f"Token refresh failed: {type(exc).__name__}") from exc

        async with SessionLocal() as session:
            result = await session.execute(
                select(CodexToken).where(CodexToken.agent_id == agent_id)
            )
            row = result.scalar_one()
            new_access = token_data["access_token"]
            new_refresh = token_data.get("refresh_token", plaintext_refresh)
            row.access_token = encrypt(new_access)
            row.refresh_token = encrypt(new_refresh)
            if token_data.get("expires_in"):
                row.expires_at = now + timedelta(seconds=int(token_data["expires_in"]))
            row.updated_at = now
            await session.commit()
            return new_access

    return decrypt(row.access_token) if row.access_token else ""


# ---------------------------------------------------------------------------
# Null context manager (when no message_id provided)
# ---------------------------------------------------------------------------


class _null_ctx:
    async def __aenter__(self) -> "_null_ctx":
        return self

    async def __aexit__(self, *_: object) -> None:
        pass


class CodexDriver:
    """Drive the Codex API (chat.completions surface) with OAuth tokens."""

    async def chat(
        self,
        messages: list[dict],
        agent_id: str,
        agent_token: str,
        model: str = "gpt-4o",
    ) -> AsyncGenerator[str, None]:
        return _with_keepalive(self._run(messages, agent_id, agent_token, model))

    async def _run(
        self,
        messages: list[dict],
        agent_id: str,
        agent_token: str,
        model: str,
        message_id: int | None = None,
    ) -> AsyncGenerator[str, None]:
        iteration = 0
        conversation = list(messages)
        last_run_id: str | None = None
        seq: int = 0

        persister: ChunkPersister | None = (
            ChunkPersister(message_id) if message_id is not None else None
        )

        async def _emit(event: str, payload: dict) -> str:
            nonlocal seq
            frame = _sse(event, payload)
            if persister is not None:
                await persister.put(seq, event, json.dumps(payload))
                seq += 1
            return frame

        ctx = persister if persister is not None else _null_ctx()

        async with ctx:
            while iteration < MAX_ITER:
                iteration += 1

                # Doom-loop guard.
                history = doom_loop.extract_history_from_messages(conversation)
                note = doom_loop.detect(history)
                if note:
                    conversation.append({"role": "system", "content": note})

                access_token = await _get_access_token(agent_id)

                headers = {
                    "Authorization": f"Bearer {access_token}",
                    "Content-Type": "application/json",
                }

                oai_payload = {
                    "model": model,
                    "messages": conversation,
                    "tools": agent_tools.TOOLS,
                    "tool_choice": "auto",
                    "stream": True,
                }

                collected_content = ""
                collected_tool_calls: list[dict] = []

                try:
                    async with httpx.AsyncClient(timeout=120.0) as client:
                        async with client.stream(
                            "POST",
                            f"{CODEX_BASE}/chat/completions",
                            headers=headers,
                            json=oai_payload,
                        ) as response:
                            response.raise_for_status()

                            async for line in response.aiter_lines():
                                if not line.startswith("data: "):
                                    continue
                                raw = line[len("data: "):]
                                if raw.strip() == "[DONE]":
                                    break

                                try:
                                    chunk = json.loads(raw)
                                except json.JSONDecodeError:
                                    continue

                                choice = chunk["choices"][0] if chunk.get("choices") else {}
                                delta = choice.get("delta", {})

                                if delta.get("content"):
                                    collected_content += delta["content"]
                                    yield await _emit("delta", {"content": delta["content"]})

                                for tc_delta in delta.get("tool_calls", []):
                                    idx = tc_delta["index"]
                                    while len(collected_tool_calls) <= idx:
                                        collected_tool_calls.append(
                                            {
                                                "id": "",
                                                "type": "function",
                                                "function": {"name": "", "arguments": ""},
                                            }
                                        )
                                    tc = collected_tool_calls[idx]
                                    if tc_delta.get("id"):
                                        tc["id"] = tc_delta["id"]
                                    fn = tc_delta.get("function", {})
                                    if fn.get("name"):
                                        tc["function"]["name"] += fn["name"]
                                    if fn.get("arguments"):
                                        tc["function"]["arguments"] += fn["arguments"]

                except asyncio.CancelledError:
                    raise
                except httpx.HTTPError as exc:
                    logger.error("codex_driver http error: %s", exc)
                    yield await _emit("error", {"message": str(exc)})
                    return

                if not collected_tool_calls:
                    break

                conversation.append(
                    {
                        "role": "assistant",
                        "content": collected_content or None,
                        "tool_calls": collected_tool_calls,
                    }
                )

                for tc in collected_tool_calls:
                    fn_name = tc["function"]["name"]
                    try:
                        fn_args = json.loads(tc["function"]["arguments"] or "{}")
                    except json.JSONDecodeError:
                        fn_args = {}

                    if "run_id" in fn_args:
                        last_run_id = fn_args["run_id"]

                    yield await _emit("tool_call", {"name": fn_name, "args": fn_args})

                    try:
                        result = await agent_tools.execute(
                            fn_name, fn_args, agent_token, agent=agent_id
                        )
                    except asyncio.CancelledError:
                        raise
                    except httpx.HTTPError as exc:
                        result = json.dumps({"error": str(exc)})

                    yield await _emit("tool_result", {"name": fn_name, "result": result[:500]})

                    conversation.append(
                        {
                            "role": "tool",
                            "tool_call_id": tc["id"],
                            "content": result,
                        }
                    )

            else:
                # Loop exhausted MAX_ITER without a final plain-text reply.
                logger.warning(
                    "codex_driver reached MAX_ITER=%d without a final answer",
                    MAX_ITER,
                    extra={"run_id": last_run_id} if last_run_id else {},
                )

            yield await _emit("done", {})
