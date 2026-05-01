"""Chat API — in-system chat surface for Stellarator.

Driver dispatch
---------------
``MessageIn.driver`` selects the upstream LLM provider:

  - ``"openai-key"``   — OpenAI Chat Completions, bearer = settings key (or
    per-request ``api_key`` body field, never persisted). Models: ``gpt-*``,
    ``o*``.
  - ``"openrouter"``   — OpenRouter, bearer = the agent's IntegrationKey
    (``kind='openrouter'``). Models: ``openrouter/*``, ``anthropic/*``,
    ``google/*``, ``meta-llama/*`` etc.
  - ``"codex"``        — Codex OAuth flow (primary OpenAI access, see oauth_codex.py).
  - ``"claude-code"``  — RESERVED; returns 501. Use the MCP server instead.
  - ``"openai-oauth"`` — DEPRECATED; returns 410. Use Codex sign-in instead.

Legacy alias ``"openai"`` is accepted and routed to ``"openai-key"``.

Routes
------
POST   /v1/chat/sessions                                             create a new chat session
GET    /v1/chat/sessions/{id}                                        get session + message history
POST   /v1/chat/sessions/{id}/messages                               send a message, stream SSE response
GET    /v1/chat/sessions/{id}/messages/{msg_id}/stream/resume        resume an interrupted SSE stream

Stream resume protocol
----------------------
Chunks older than 7 days are eligible for cleanup (not implemented; add a
periodic task later).

The POST /messages endpoint emits a leading SSE event::

    event: message_id
    data: {"id": <int>}

before any delta/tool_call/tool_result/done/error events so the client can
record the assistant message_id and use it for resume if the connection drops.

The resume endpoint (GET .../stream/resume?after_seq=N) replays all persisted
chunks with seq > N and then tails for new ones (200ms poll) until a ``done``
chunk is seen or the client disconnects.  Standard EventSource Last-Event-ID
semantics are supported: if the ``Last-Event-ID`` header is present it
overrides ``after_seq``.
"""

from __future__ import annotations

import asyncio
import json
import uuid
from datetime import datetime

from fastapi import APIRouter, Depends, Header, HTTPException, Request
from fastapi.responses import StreamingResponse
from pydantic import BaseModel
from sqlalchemy import select, text
from sqlalchemy.ext.asyncio import AsyncSession

from app.core.auth import CurrentAgent, current_agent
from app.core.config import settings
from app.core.db import SessionLocal, get_session
from app.models.chat import ChatMessage, ChatSession

from app.agents.openai_driver import OpenAIDriver
from app.agents.codex_driver import CodexDriver
from app.agents.openrouter_driver import OpenRouterDriver

_VALID_DRIVERS = {"openai-key", "openrouter", "codex", "claude-code", "openai"}

router = APIRouter(prefix="/v1/chat", tags=["chat"])


# ---------------------------------------------------------------------------
# Pydantic schemas
# ---------------------------------------------------------------------------


class SessionCreate(BaseModel):
    system_prompt: str = ""


class SessionOut(BaseModel):
    id: str
    agent: str
    system_prompt: str
    created_at: datetime

    model_config = {"from_attributes": True}


class MessageIn(BaseModel):
    content: str
    model: str = "gpt-4o"
    # "openai-key" | "openai-oauth" | "openrouter" | "codex"
    # Legacy alias "openai" is accepted and treated as "openai-key".
    driver: str = "openai-key"
    # Per-request OpenAI API key (driver="openai-key" only). Held for the
    # lifetime of the request; never persisted, never logged.
    api_key: str | None = None


class MessageOut(BaseModel):
    id: int
    session_id: str
    role: str
    content: str
    created_at: datetime

    model_config = {"from_attributes": True}


class SessionDetail(SessionOut):
    messages: list[MessageOut] = []


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _resolve_agent_token(agent: str) -> str:
    """Return the bearer token string for an agent identity."""
    mapping = {
        "openai": settings.agent_token_openai,
        "codex": settings.agent_token_codex,
        "claude-code": settings.agent_token_claude_code,
    }
    token = mapping.get(agent, "")
    if not token:
        raise HTTPException(503, f"No bearer token configured for agent '{agent}'")
    return token


# ---------------------------------------------------------------------------
# Endpoints
# ---------------------------------------------------------------------------


@router.post("/sessions", response_model=SessionOut, status_code=201)
async def create_session(
    body: SessionCreate,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
):
    chat_session = ChatSession(
        id=str(uuid.uuid4()),
        agent=agent,
        system_prompt=body.system_prompt,
    )
    session.add(chat_session)
    await session.commit()
    await session.refresh(chat_session)
    return chat_session


@router.get("/sessions/{session_id}", response_model=SessionDetail)
async def get_session_detail(
    session_id: str,
    agent: str = CurrentAgent,
    db: AsyncSession = Depends(get_session),
):
    result = await db.execute(
        select(ChatSession).where(ChatSession.id == session_id)
    )
    chat_session = result.scalar_one_or_none()
    if not chat_session:
        raise HTTPException(404, "Session not found")
    if chat_session.agent != agent:
        raise HTTPException(403, "Not your session")

    msgs_result = await db.execute(
        select(ChatMessage)
        .where(ChatMessage.session_id == session_id)
        .order_by(ChatMessage.created_at)
    )
    messages = msgs_result.scalars().all()
    return SessionDetail(
        id=chat_session.id,
        agent=chat_session.agent,
        system_prompt=chat_session.system_prompt,
        created_at=chat_session.created_at,
        messages=[MessageOut.model_validate(m) for m in messages],
    )


@router.post("/sessions/{session_id}/messages")
async def post_message(
    session_id: str,
    body: MessageIn,
    request: Request,
    authorization: str | None = Header(default=None),
    db: AsyncSession = Depends(get_session),
):
    # Authenticate
    agent = await current_agent(authorization=authorization)

    # Fetch session
    result = await db.execute(
        select(ChatSession).where(ChatSession.id == session_id)
    )
    chat_session = result.scalar_one_or_none()
    if not chat_session:
        raise HTTPException(404, "Session not found")
    if chat_session.agent != agent:
        raise HTTPException(403, "Not your session")

    # Persist user message
    user_msg = ChatMessage(
        session_id=session_id,
        role="user",
        content=body.content,
    )
    db.add(user_msg)
    await db.commit()

    # Pre-create the assistant message row so we can return its id in the
    # leading SSE event and attach chunks to it during streaming.
    assistant_msg = ChatMessage(
        session_id=session_id,
        role="assistant",
        content="",
    )
    db.add(assistant_msg)
    await db.commit()
    await db.refresh(assistant_msg)
    assistant_message_id: int = assistant_msg.id

    # Build conversation history for the driver
    msgs_result = await db.execute(
        select(ChatMessage)
        .where(ChatMessage.session_id == session_id)
        .where(ChatMessage.role != "assistant")  # exclude the blank placeholder
        .order_by(ChatMessage.created_at)
    )
    history = msgs_result.scalars().all()

    conversation: list[dict] = []
    if chat_session.system_prompt:
        conversation.append({"role": "system", "content": chat_session.system_prompt})

    for m in history:
        msg: dict = {"role": m.role, "content": m.content}
        if m.tool_calls_json:
            msg["tool_calls"] = json.loads(m.tool_calls_json)
        conversation.append(msg)

    # Resolve the agent bearer token for internal tool calls
    agent_token = _resolve_agent_token(agent)

    # Validate driver (accept legacy "openai" alias).
    driver_choice = body.driver
    if driver_choice == "openai":
        driver_choice = "openai-key"
    if driver_choice == "openai-oauth":
        raise HTTPException(
            410,
            detail="openai-oauth deprecated; sign in with Codex instead",
        )
    if driver_choice == "claude-code":
        raise HTTPException(
            501,
            detail="Use the MCP server for Claude Code",
        )
    if driver_choice not in {"openai-key", "openrouter", "codex"}:
        raise HTTPException(
            400,
            "driver must be one of openai-key | openrouter | codex | claude-code",
        )

    async def generate():
        """Yield SSE data, persist the assistant message, handle disconnect."""
        full_content = ""
        tool_calls_summary: list[dict] = []

        # Leading event so the client can resume if the connection drops.
        yield (
            f"event: message_id\ndata: {json.dumps({'id': assistant_message_id})}\n\n"
        )

        try:
            if driver_choice == "openai-key":
                driver = OpenAIDriver()
                auth = {"kind": "key", "api_key": body.api_key} if body.api_key else None
                stream = driver._run(
                    conversation, agent_token, body.model,
                    message_id=assistant_message_id,
                    auth=auth,
                )
            elif driver_choice == "openrouter":
                router_driver = OpenRouterDriver()
                stream = router_driver._run(
                    conversation,
                    agent_id=agent,
                    agent_token=agent_token,
                    model=body.model,
                    message_id=assistant_message_id,
                )
            else:  # codex
                driver = CodexDriver()
                stream = driver._run(
                    conversation,
                    agent_id=agent,
                    agent_token=agent_token,
                    model=body.model,
                    message_id=assistant_message_id,
                )

            async for chunk in stream:
                if await request.is_disconnected():
                    break
                yield chunk

                # Parse for accumulation
                if chunk.startswith("event: ") and "\ndata: " in chunk:
                    event_line, data_line = chunk.split("\ndata: ", 1)
                    event_type = event_line[len("event: "):].strip()
                    raw_data = data_line.split("\n")[0]
                    try:
                        evt = json.loads(raw_data)
                        if event_type == "delta":
                            full_content += evt.get("content", "")
                        elif event_type == "tool_call":
                            tool_calls_summary.append(
                                {"name": evt["name"], "args": evt["args"]}
                            )
                    except (json.JSONDecodeError, KeyError):
                        pass

        finally:
            # Update the pre-created assistant row with accumulated content.
            try:
                async with db.begin():
                    await db.execute(
                        text(
                            "UPDATE chat_messages SET content=:c, tool_calls_json=:tc"
                            " WHERE id=:id"
                        ),
                        {
                            "c": full_content,
                            "tc": json.dumps(tool_calls_summary) if tool_calls_summary else None,
                            "id": assistant_message_id,
                        },
                    )
            except Exception:
                pass  # Best-effort; row already exists

    return StreamingResponse(
        generate(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "X-Accel-Buffering": "no",
        },
    )


# ---------------------------------------------------------------------------
# Resume endpoint
# ---------------------------------------------------------------------------


@router.get("/sessions/{session_id}/messages/{message_id}/stream/resume")
async def resume_stream(
    session_id: str,
    message_id: int,
    request: Request,
    after_seq: int = -1,
    last_event_id: str | None = Header(default=None, alias="last-event-id"),
    authorization: str | None = Header(default=None),
    db: AsyncSession = Depends(get_session),
):
    """Resume an interrupted SSE stream.

    Replays all chunks with seq > after_seq (or last-event-id if present),
    then tails new chunks at 200ms intervals until a ``done`` chunk is seen
    or the client disconnects.

    Auth: only the agent that owns the session may resume (404 on mismatch to
    avoid leaking existence).
    """
    agent = await current_agent(authorization=authorization)

    # Resolve session ownership — return 404 regardless of why to avoid
    # leaking existence to other agents.
    sess_result = await db.execute(
        select(ChatSession).where(ChatSession.id == session_id)
    )
    chat_session = sess_result.scalar_one_or_none()
    if not chat_session or chat_session.agent != agent:
        raise HTTPException(404, "Not found")

    # Verify the message belongs to this session.
    msg_result = await db.execute(
        select(ChatMessage).where(
            ChatMessage.id == message_id,
            ChatMessage.session_id == session_id,
        )
    )
    if msg_result.scalar_one_or_none() is None:
        raise HTTPException(404, "Not found")

    # Honour Last-Event-ID header (standard EventSource reconnect).
    start_seq: int = after_seq
    if last_event_id is not None:
        try:
            seq_int = int(last_event_id)
        except ValueError:
            raise HTTPException(400, "last-event-id must be an integer")
        if seq_int < 0:
            raise HTTPException(400, "last-event-id must not be negative")
        start_seq = max(0, min(seq_int, 2**31 - 1))

    async def tail() -> AsyncGenerator[str, None]:
        # 1. Replay already-persisted chunks.
        # Use a fresh session per poll iteration so the long-lived generator
        # does not hold the request-scoped session open for its full lifetime.
        async with SessionLocal() as _db:
            rows = await _db.execute(
                text(
                    "SELECT seq, kind, payload FROM chat_stream_chunks"
                    " WHERE message_id=:mid AND seq > :seq"
                    " ORDER BY seq"
                ),
                {"mid": message_id, "seq": start_seq},
            )
            done_seen = False
            for seq_val, kind, payload in rows:
                yield f"event: {kind}\ndata: {payload}\nid: {seq_val}\n\n"
                if kind == "done":
                    done_seen = True

            if done_seen:
                return

            # Find highest seq already replayed.
            max_rows = await _db.execute(
                text(
                    "SELECT MAX(seq) FROM chat_stream_chunks WHERE message_id=:mid"
                ),
                {"mid": message_id},
            )
            row = max_rows.fetchone()

        # 2. Tail mode: poll for new chunks every 200ms using a fresh session
        # per iteration so the DB session lifetime is bounded to each poll.
        last_seen_seq = start_seq
        if row and row[0] is not None:
            last_seen_seq = row[0]

        while True:
            if await request.is_disconnected():
                break

            await asyncio.sleep(0.2)

            async with SessionLocal() as _db:
                new_rows = await _db.execute(
                    text(
                        "SELECT seq, kind, payload FROM chat_stream_chunks"
                        " WHERE message_id=:mid AND seq > :seq"
                        " ORDER BY seq"
                    ),
                    {"mid": message_id, "seq": last_seen_seq},
                )
                for seq_val, kind, payload in new_rows:
                    last_seen_seq = seq_val
                    yield f"event: {kind}\ndata: {payload}\nid: {seq_val}\n\n"
                    if kind == "done":
                        return

    return StreamingResponse(
        tail(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "X-Accel-Buffering": "no",
        },
    )


# Satisfy type checker for the generator annotation in tail()
from typing import AsyncGenerator  # noqa: E402
