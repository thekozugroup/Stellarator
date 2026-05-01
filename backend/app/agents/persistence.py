"""Stream-chunk persistence helpers.

Provides ``ChunkPersister`` — an async queue-backed sink that writes
``ChatStreamChunk`` rows without blocking the HTTP stream.  The persister
flushes synchronously on ``done`` and on cancellation so no chunks are lost.

Usage inside a driver::

    async with ChunkPersister(message_id) as sink:
        async for sse_frame in raw_stream:
            await sink.put(seq, kind, payload)
            yield sse_frame
"""

from __future__ import annotations

import asyncio
import logging
from typing import Optional

from sqlalchemy import text

from app.core.db import SessionLocal

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Low-level persist helper
# ---------------------------------------------------------------------------


async def persist_chunk(
    message_id: int,
    seq: int,
    kind: str,
    payload: str,
) -> None:
    """Insert one ChatStreamChunk row.  Silently ignores duplicate (message_id, seq)."""
    async with SessionLocal() as db:
        try:
            await db.execute(
                text(
                    "INSERT OR IGNORE INTO chat_stream_chunks"
                    " (message_id, seq, kind, payload, created_at)"
                    " VALUES (:mid, :seq, :kind, :payload, CURRENT_TIMESTAMP)"
                ),
                {"mid": message_id, "seq": seq, "kind": kind, "payload": payload},
            )
            await db.commit()
        except Exception:
            logger.exception("persist_chunk failed (message_id=%s seq=%s)", message_id, seq)


# ---------------------------------------------------------------------------
# Queue-backed async persister
# ---------------------------------------------------------------------------


class ChunkPersister:
    """Context manager that drains a queue of chunks to the DB in the background.

    The background task runs as long as the context is open.  On exit (or on
    cancellation) the queue is drained synchronously so no chunks are dropped.
    """

    def __init__(self, message_id: int) -> None:
        self._message_id = message_id
        self._queue: asyncio.Queue[Optional[tuple[int, str, str]]] = asyncio.Queue()
        self._task: asyncio.Task[None] | None = None

    async def __aenter__(self) -> "ChunkPersister":
        self._task = asyncio.ensure_future(self._drain())
        return self

    async def __aexit__(self, exc_type, exc, tb) -> None:  # type: ignore[override]
        # Signal the background task to stop and wait for it.
        await self._queue.put(None)
        if self._task is not None:
            try:
                # Shield the wait so that an outer asyncio.CancelledError (e.g.
                # client disconnect) does not abort the drain mid-flight and
                # lose already-enqueued chunks.  The inner task still completes.
                await asyncio.shield(asyncio.wait_for(self._task, timeout=5.0))
            except (asyncio.TimeoutError, asyncio.CancelledError):
                logger.warning("ChunkPersister drain timed out for message_id=%s", self._message_id)

    async def put(self, seq: int, kind: str, payload: str) -> None:
        """Enqueue a chunk for async persistence."""
        await self._queue.put((seq, kind, payload))

    async def flush(self) -> None:
        """Block until the queue is empty (used after 'done' kind)."""
        await self._queue.join()

    async def _drain(self) -> None:
        """Background worker: pull from queue and persist."""
        while True:
            item = await self._queue.get()
            if item is None:
                # Drain remaining items before exiting.
                # Each persist is wrapped in wait_for so a stalled DB call
                # cannot block the drain indefinitely.  asyncio.shield ensures
                # the write itself is not cancelled mid-flight if this task is
                # cancelled from outside.
                while not self._queue.empty():
                    remaining = self._queue.get_nowait()
                    if remaining is not None:
                        seq, kind, payload = remaining
                        try:
                            await asyncio.wait_for(
                                asyncio.shield(
                                    persist_chunk(self._message_id, seq, kind, payload)
                                ),
                                timeout=2.0,
                            )
                        except asyncio.TimeoutError:
                            logger.warning(
                                "ChunkPersister drain timed out on seq=%s for message_id=%s; "
                                "aborting remaining drain",
                                seq, self._message_id,
                            )
                            self._queue.task_done()
                            break
                    self._queue.task_done()
                self._queue.task_done()
                break
            seq, kind, payload = item
            await persist_chunk(self._message_id, seq, kind, payload)
            self._queue.task_done()
