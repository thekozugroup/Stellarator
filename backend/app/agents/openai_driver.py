"""OpenAI Chat Completions driver for Stellarator chat agents.

Runs an agentic tool loop:
  1. Send conversation + tools to OpenAI.
  2. If the response contains tool_calls, execute each via our /v1 endpoints.
  3. Append assistant + tool results and repeat.
  4. Stop when the model returns a plain message or after MAX_ITER iterations.

Streams tokens as proper SSE frames:
    event: <type>\ndata: <json>\nid: <monotonic>\n\n

Event types: delta, tool_call, tool_result, done, error.

SSE keep-alives: a `: ping\\n\\n` comment is emitted every PING_INTERVAL_S seconds
of idle to prevent proxy reaping.
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
from app.services.crypto import decrypt, encrypt

from . import doom_loop, tools as agent_tools
from .persistence import ChunkPersister

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

MAX_ITER: int = int(os.environ.get("STELLARATOR_AGENT_MAX_ITER", "12"))
OPENAI_BASE = "https://api.openai.com/v1"
PING_INTERVAL_S = 15.0
_REFRESH_BUFFER_SECS = 60


# ---------------------------------------------------------------------------
# OAuth-token resolver (Sign in with OpenAI)
# ---------------------------------------------------------------------------


async def _resolve_oauth_access_token(agent_id: str) -> str:
    """Return a valid OpenAI OAuth access token, refreshing if expiring soon.

    Looks up :class:`app.models.oauth.OpenAIToken` for ``agent_id``. Token
    plaintext is never logged.
    """
    # Imported lazily to avoid a hard dep at module load (model lives in a
    # sibling package introduced alongside this fallback path).
    from app.models.oauth import OpenAIToken

    async with SessionLocal() as session:
        result = await session.execute(
            select(OpenAIToken).where(OpenAIToken.agent_id == agent_id)
        )
        row: OpenAIToken | None = result.scalar_one_or_none()

    if row is None:
        raise RuntimeError(
            f"No OpenAI OAuth token found for agent '{agent_id}'. "
            "Complete the OAuth flow at /v1/oauth/openai/start first."
        )

    now = datetime.now(tz=timezone.utc)
    expires_soon = (
        row.expires_at is not None
        and row.expires_at.replace(tzinfo=timezone.utc) - now
        < timedelta(seconds=_REFRESH_BUFFER_SECS)
    )

    plaintext_refresh = decrypt(row.refresh_token) if row.refresh_token else ""

    if expires_soon and plaintext_refresh:
        try:
            async with httpx.AsyncClient(timeout=30.0) as client:
                r = await client.post(
                    settings.openai_oauth_token_url,
                    data={
                        "grant_type": "refresh_token",
                        "refresh_token": plaintext_refresh,
                        "client_id": settings.openai_oauth_client_id,
                        "client_secret": settings.openai_oauth_client_secret,
                    },
                    headers={"Content-Type": "application/x-www-form-urlencoded"},
                )
                r.raise_for_status()
                token_data = r.json()
        except httpx.HTTPError as exc:
            raise RuntimeError(
                f"OpenAI OAuth refresh failed: {type(exc).__name__}"
            ) from exc

        async with SessionLocal() as session:
            result = await session.execute(
                select(OpenAIToken).where(OpenAIToken.agent_id == agent_id)
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
    """Wrap *source* so a `: ping\\n\\n` comment is yielded every *interval* seconds of idle."""
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


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


class OpenAIDriver:
    """Drive the OpenAI Chat Completions API with tool-call looping."""

    @staticmethod
    async def _resolve_bearer(auth: dict | None) -> str:
        """Pick the right OpenAI bearer token for this request.

        Only ``kind='key'`` (or ``None``) is supported; OAuth was removed in
        favour of the Codex sign-in flow.
        """
        if auth is None or auth.get("kind") == "key":
            override = (auth or {}).get("api_key")
            if override:
                return str(override)
            return settings.openai_api_key
        raise RuntimeError(f"Unknown auth kind: {auth.get('kind')!r}")

    async def chat(
        self,
        messages: list[dict],
        agent_token: str,
        model: str = "gpt-4o",
        auth: dict | None = None,
    ) -> AsyncGenerator[str, None]:
        """Yield SSE frames until the assistant replies with no tool calls.

        ``auth`` selects the upstream OpenAI credential:
          - ``None`` or ``{"kind": "key"}``  → settings.openai_api_key
          - ``{"kind": "key", "api_key": s}`` → per-request API key (not stored)
        """
        return _with_keepalive(self._run(messages, agent_token, model, auth=auth))

    async def _run(
        self,
        messages: list[dict],
        agent_token: str,
        model: str,
        message_id: int | None = None,
        auth: dict | None = None,
    ) -> AsyncGenerator[str, None]:
        iteration = 0
        conversation = list(messages)
        last_run_id: str | None = None
        seq: int = 0

        # Resolve the upstream OpenAI credential. Never log the value.
        bearer = await self._resolve_bearer(auth)
        headers = {
            "Authorization": f"Bearer {bearer}",
            "Content-Type": "application/json",
        }

        persister: ChunkPersister | None = (
            ChunkPersister(message_id) if message_id is not None else None
        )

        async def _emit(event: str, payload: dict) -> str:
            """Format SSE, persist to DB sink (non-blocking), and return the frame."""
            nonlocal seq
            frame = _sse(event, payload)
            if persister is not None:
                await persister.put(seq, event, json.dumps(payload))
                seq += 1
            return frame

        ctx = persister if persister is not None else _null_ctx()

        async with ctx:
            async with httpx.AsyncClient(timeout=120.0) as client:
                while iteration < MAX_ITER:
                    iteration += 1

                    # Doom-loop guard: inject a corrective system note if the
                    # last N tool calls show the same call repeated 3+ times.
                    history = doom_loop.extract_history_from_messages(conversation)
                    note = doom_loop.detect(history)
                    if note:
                        conversation.append({"role": "system", "content": note})

                    oai_payload: dict = {
                        "model": model,
                        "messages": conversation,
                        "tools": agent_tools.TOOLS,
                        "tool_choice": "auto",
                        "stream": True,
                    }

                    collected_content = ""
                    collected_tool_calls: list[dict] = []
                    finish_reason = ""

                    try:
                        async with client.stream(
                            "POST",
                            f"{OPENAI_BASE}/chat/completions",
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
                                finish_reason = choice.get("finish_reason") or finish_reason

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
                        logger.error("openai_driver http error: %s", exc)
                        yield await _emit("error", {"message": str(exc)})
                        return

                    # No tool calls → done.
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

                        # Track run_id for structured log context.
                        if "run_id" in fn_args:
                            last_run_id = fn_args["run_id"]

                        yield await _emit("tool_call", {"name": fn_name, "args": fn_args})

                        try:
                            result = await agent_tools.execute(fn_name, fn_args, agent_token)
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
                    # Loop exhausted MAX_ITER.
                    logger.warning(
                        "openai_driver reached MAX_ITER=%d without a final answer",
                        MAX_ITER,
                        extra={"run_id": last_run_id} if last_run_id else {},
                    )

            yield await _emit("done", {})


# ---------------------------------------------------------------------------
# Null context manager (when no message_id provided)
# ---------------------------------------------------------------------------


class _null_ctx:
    async def __aenter__(self) -> "_null_ctx":
        return self

    async def __aexit__(self, *_: object) -> None:
        pass
