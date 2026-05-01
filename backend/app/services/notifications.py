"""Notification fan-out service for SSE delivery.

One asyncio.Queue per agent. Run/alert handlers push events; the SSE
endpoint drains them. Queues are created lazily and never removed (bounded
by the number of distinct agents, which is small in practice).
"""

from __future__ import annotations

import asyncio
from datetime import datetime, timezone
from typing import Any

_queues: dict[str, asyncio.Queue[dict[str, Any]]] = {}


def _queue_for(agent: str) -> asyncio.Queue[dict[str, Any]]:
    if agent not in _queues:
        _queues[agent] = asyncio.Queue(maxsize=256)
    return _queues[agent]


def _now_iso() -> str:
    return datetime.now(tz=timezone.utc).isoformat()


def notify_run_finished(
    *,
    agent: str,
    run_id: str,
    run_name: str,
    status: str,
    is_sandbox: bool,
) -> None:
    """Push a run_finished or sandbox_ready event; non-blocking (drop if full)."""
    q = _queue_for(agent)
    event_type = "sandbox_ready" if (is_sandbox and status == "succeeded") else "run_finished"
    msg: dict[str, Any] = {
        "type": event_type,
        "run_id": run_id,
        "run_name": run_name,
        "message": (
            "Sandbox run completed — ready to promote."
            if event_type == "sandbox_ready"
            else f"Run finished with status: {status}."
        ),
        "level": "info" if status == "succeeded" else "error",
        "ts": _now_iso(),
    }
    try:
        q.put_nowait(msg)
    except asyncio.QueueFull:
        pass  # Drop silently; client will reconcile via polling.


def notify_alert_error(
    *,
    agent: str,
    run_id: str,
    run_name: str,
    title: str,
) -> None:
    """Push an alert_error event for an ERROR-level alert."""
    q = _queue_for(agent)
    msg: dict[str, Any] = {
        "type": "alert_error",
        "run_id": run_id,
        "run_name": run_name,
        "message": title,
        "level": "error",
        "ts": _now_iso(),
    }
    try:
        q.put_nowait(msg)
    except asyncio.QueueFull:
        pass


async def drain(agent: str, timeout: float = 15.0) -> dict[str, Any] | None:
    """Wait up to *timeout* seconds for the next event. Returns None on timeout."""
    q = _queue_for(agent)
    try:
        return await asyncio.wait_for(q.get(), timeout=timeout)
    except asyncio.TimeoutError:
        return None
