"""Per-agent WebSocket connection semaphores.

Enforces two limits simultaneously:
  - per_agent: maximum concurrent WS connections from a single agent identity.
  - global_: maximum concurrent WS connections across all agents.

Both slots must be acquired before a connection proceeds; both are released
in a finally-block on the caller side.

TTL eviction:
  Entries idle for >3600s are opportunistically evicted on each acquire().
  The dict is also capped at 256 entries; the oldest (by last_used_at) is
  evicted on overflow.  Monotonic clock is used throughout.
"""

from __future__ import annotations

import asyncio
import logging
import time

logger = logging.getLogger(__name__)

_MAX_AGENTS = 256
_IDLE_TTL_SECS = 3600.0


class PerAgentSemaphore:
    """Dual-slot semaphore: per-agent cap + process-wide global cap."""

    def __init__(self, *, per_agent: int = 16, global_: int = 64) -> None:
        self._per_agent_cap = per_agent
        self._global = asyncio.Semaphore(global_)
        self._agents: dict[str, asyncio.Semaphore] = {}
        self._last_used: dict[str, float] = {}
        self._lock = asyncio.Lock()

    def _evict_lru_locked(self) -> None:
        """Evict entries idle >TTL; then evict oldest if still over cap.

        Must be called with self._lock held.
        """
        now = time.monotonic()
        # TTL-based eviction (only idle entries — value == per_agent_cap means
        # no active connections on that semaphore).
        ttl_expired = [
            agent
            for agent, ts in self._last_used.items()
            if (now - ts) > _IDLE_TTL_SECS
            and self._agents.get(agent) is not None
            and self._agents[agent]._value == self._per_agent_cap  # type: ignore[attr-defined]
        ]
        for agent in ttl_expired:
            self._agents.pop(agent, None)
            self._last_used.pop(agent, None)

        # Cap eviction: remove oldest by last_used_at until within limit.
        while len(self._agents) >= _MAX_AGENTS:
            oldest = min(self._last_used, key=self._last_used.__getitem__)
            # Only remove if no active connections on the semaphore.
            sem = self._agents.get(oldest)
            if sem is not None and sem._value < self._per_agent_cap:  # type: ignore[attr-defined]
                # Active connections — skip to prevent premature eviction.
                break
            self._agents.pop(oldest, None)
            self._last_used.pop(oldest, None)

    async def _semaphore_for(self, agent: str) -> asyncio.Semaphore:
        """Return (creating if absent) the per-agent semaphore.

        Opportunistically evicts stale entries and enforces the agent-count cap.
        """
        async with self._lock:
            self._evict_lru_locked()
            if agent not in self._agents:
                self._agents[agent] = asyncio.Semaphore(self._per_agent_cap)
            self._last_used[agent] = time.monotonic()
            return self._agents[agent]

    async def acquire(self, agent: str) -> None:
        """Acquire both global and per-agent slots (global first).

        Atomic: if per-agent acquire fails, global is released before raising.
        """
        await self._global.acquire()
        per = await self._semaphore_for(agent)
        try:
            await per.acquire()
        except BaseException:
            self._global.release()
            raise

    def release(self, agent: str) -> None:
        """Release both slots. Caller must guarantee a prior successful acquire."""
        sem = self._agents.get(agent)
        if sem is not None:
            sem.release()
            self._last_used[agent] = time.monotonic()
        else:
            logger.error("release() called for unknown agent %r — semaphore leak possible", agent)
        self._global.release()


# Process-wide singleton used by the WS proxy endpoint.
ws_connection_slots = PerAgentSemaphore(per_agent=16, global_=64)

__all__ = ["PerAgentSemaphore", "ws_connection_slots"]
