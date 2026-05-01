"""SSE notification stream endpoint.

GET /v1/notifications/stream — authenticated via bearer token.
Pushes run_finished, alert_error, and sandbox_ready events.
Keep-alive ping sent every 15 s when no event is available.
"""

from __future__ import annotations

import json
import logging
from typing import AsyncIterator

from fastapi import APIRouter, Request
from fastapi.responses import StreamingResponse

from app.core.auth import CurrentAgent
from app.services import notifications as notif_service

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/notifications", tags=["notifications"])

_KEEPALIVE_INTERVAL = 15.0  # seconds


async def _event_stream(agent: str, request: Request) -> AsyncIterator[str]:
    """Yield SSE-formatted bytes until the client disconnects."""
    while not await request.is_disconnected():
        event = await notif_service.drain(agent, timeout=_KEEPALIVE_INTERVAL)
        if await request.is_disconnected():
            break
        if event is None:
            # Keepalive
            yield ": ping\n\n"
        else:
            payload = json.dumps(event, default=str)
            yield f"data: {payload}\n\n"


@router.get("/stream")
async def notification_stream(
    request: Request,
    agent: str = CurrentAgent,
) -> StreamingResponse:
    """Stream SSE events for the authenticated agent."""
    return StreamingResponse(
        _event_stream(agent, request),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "X-Accel-Buffering": "no",
        },
    )
