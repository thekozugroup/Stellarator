"""OpenRouter driver — wire-compatible with OpenAI Chat Completions.

Mirrors the structure of :mod:`app.agents.openai_driver` (tool loop + SSE +
keep-alive + chunk persistence). Only the upstream URL, the auth resolution,
and the etiquette headers (``HTTP-Referer`` and ``X-Title``) differ.

Auth: per-agent ``IntegrationKey`` row of kind ``"openrouter"``, resolved via
``app.services.integrations.resolve_key``. That helper is being added in a
parallel PR; until it lands we fall back to a local stub that reads the row
directly so this module stays importable.

SSE wire format:
    event: <type>\\ndata: <json>\\nid: <monotonic>\\n\\n
Event types: delta, tool_call, tool_result, done, error.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
from typing import AsyncGenerator

import httpx
from sqlalchemy import select

from app.core.config import settings
from app.core.db import SessionLocal

from . import doom_loop, tools as agent_tools
from .persistence import ChunkPersister

logger = logging.getLogger(__name__)

MAX_ITER: int = int(os.environ.get("STELLARATOR_AGENT_MAX_ITER", "12"))
OPENROUTER_BASE = os.environ.get(
    "OPENROUTER_BASE_URL", "https://openrouter.ai/api/v1"
)
PING_INTERVAL_S = 15.0


# ---------------------------------------------------------------------------
# SSE helpers (duplicated rather than imported so each driver remains
# self-contained and independently testable)
# ---------------------------------------------------------------------------

_sse_counter: int = 0


def _next_id() -> int:
    global _sse_counter  # noqa: PLW0603
    _sse_counter += 1
    return _sse_counter


def _sse(event: str, payload: dict) -> str:
    return f"event: {event}\ndata: {json.dumps(payload)}\nid: {_next_id()}\n\n"


async def _with_keepalive(
    source: AsyncGenerator[str, None],
    interval: float = PING_INTERVAL_S,
) -> AsyncGenerator[str, None]:
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
# Key resolution — prefer the shared integrations helper if available.
# ---------------------------------------------------------------------------


async def _resolve_openrouter_key(agent_id: str) -> str:
    """Look up the per-agent OpenRouter API key (decrypted)."""
    # TODO: switch to ``app.services.integrations.resolve_key(agent, "openrouter")``
    # once the parallel PR lands. The local stub below is a drop-in replacement
    # that reads from the same ``integration_keys`` table.
    try:
        from app.services.integrations import resolve_key  # type: ignore[attr-defined]

        return await resolve_key(agent_id, "openrouter")
    except ImportError:
        # resolve_key module not yet available; fall back to direct DB read.
        # Genuine runtime errors from resolve_key must propagate — do NOT
        # catch Exception here.
        from app.models.integration import IntegrationKey
        from app.services.crypto import decrypt

        async with SessionLocal() as session:
            result = await session.execute(
                select(IntegrationKey).where(
                    IntegrationKey.agent_id == agent_id,
                    IntegrationKey.kind == "openrouter",
                )
            )
            row = result.scalar_one_or_none()
        if row is None:
            raise RuntimeError(
                f"No OpenRouter key configured for agent '{agent_id}'."
            )
        return decrypt(row.ciphertext)


# ---------------------------------------------------------------------------
# Null context manager
# ---------------------------------------------------------------------------


class _null_ctx:
    async def __aenter__(self) -> "_null_ctx":
        return self

    async def __aexit__(self, *_: object) -> None:
        pass


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


class OpenRouterDriver:
    """Drive the OpenRouter chat-completions surface with tool-call looping."""

    async def chat(
        self,
        messages: list[dict],
        agent_id: str,
        agent_token: str,
        model: str = "openrouter/auto",
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

        api_key = await _resolve_openrouter_key(agent_id)

        headers = {
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
            "HTTP-Referer": settings.openrouter_referer,
            "X-Title": settings.openrouter_title,
        }

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
            async with httpx.AsyncClient(timeout=120.0) as client:
                while iteration < MAX_ITER:
                    iteration += 1

                    # Doom-loop guard.
                    history = doom_loop.extract_history_from_messages(conversation)
                    note = doom_loop.detect(history)
                    if note:
                        conversation.append({"role": "system", "content": note})

                    payload: dict = {
                        "model": model,
                        "messages": conversation,
                        "tools": agent_tools.TOOLS,
                        "tool_choice": "auto",
                        "stream": True,
                    }

                    collected_content = ""
                    collected_tool_calls: list[dict] = []

                    try:
                        async with client.stream(
                            "POST",
                            f"{OPENROUTER_BASE}/chat/completions",
                            headers=headers,
                            json=payload,
                        ) as response:
                            if response.status_code == 401:
                                logger.error("openrouter_driver upstream 401")
                                yield await _emit(
                                    "error",
                                    {"message": "OpenRouter upstream auth failed", "status": 502},
                                )
                                return
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
                    except httpx.HTTPStatusError as exc:
                        # Surface upstream auth or other 4xx as 502 to the caller.
                        status = exc.response.status_code if exc.response is not None else 502
                        logger.error("openrouter_driver upstream status=%s", status)
                        yield await _emit(
                            "error",
                            {"message": f"OpenRouter upstream error", "status": 502},
                        )
                        return
                    except httpx.HTTPError as exc:
                        logger.error("openrouter_driver http error: %s", exc)
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

                        yield await _emit(
                            "tool_result", {"name": fn_name, "result": result[:500]}
                        )

                        conversation.append(
                            {
                                "role": "tool",
                                "tool_call_id": tc["id"],
                                "content": result,
                            }
                        )

                else:
                    logger.warning(
                        "openrouter_driver reached MAX_ITER=%d without final answer",
                        MAX_ITER,
                        extra={"run_id": last_run_id} if last_run_id else {},
                    )

            yield await _emit("done", {})
