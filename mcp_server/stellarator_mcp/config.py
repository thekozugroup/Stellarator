"""Base-URL resolution for the Stellarator MCP server.

Resolution order:
  1. STELLARATOR_BASE_URL env var (if set).
  2. http://backend:8000  (docker-compose internal DNS) — probed with a 1 s TCP connect.
  3. http://localhost:8000 (fallback for local dev).

Result is cached after the first successful resolution so subsequent calls are free.
The bearer token is never stored or logged in this module.
"""

from __future__ import annotations

import asyncio
import logging
import os
from typing import Final

logger = logging.getLogger(__name__)

_DOCKER_URL: Final = "http://backend:8000"
_LOCALHOST_URL: Final = "http://localhost:8000"
_PROBE_TIMEOUT_S: Final = 1.0

# Module-level cache; only mutated once inside _resolve() under the lock.
_cached_url: str | None = None
_lock = asyncio.Lock()


async def _tcp_reachable(host: str, port: int, timeout: float) -> bool:
    """Return True iff a TCP connection to host:port succeeds within *timeout* seconds."""
    try:
        _, writer = await asyncio.wait_for(
            asyncio.open_connection(host, port),
            timeout=timeout,
        )
        writer.close()
        try:
            await writer.wait_closed()
        except Exception:  # pragma: no cover – platform quirk
            pass
        return True
    except (OSError, asyncio.TimeoutError):
        return False


async def _resolve() -> str:
    """Resolve and cache the base URL. Called once; subsequent calls return cached value."""
    global _cached_url  # noqa: PLW0603

    async with _lock:
        # Double-checked locking — another coroutine may have resolved while we waited.
        if _cached_url is not None:
            return _cached_url

        env_url = os.environ.get("STELLARATOR_BASE_URL", "").strip()
        if env_url:
            logger.debug("stellarator base_url from env: %s", env_url)
            _cached_url = env_url
            return _cached_url

        # Probe docker-internal DNS.
        reachable = await _tcp_reachable("backend", 8000, _PROBE_TIMEOUT_S)
        if reachable:
            logger.debug("stellarator base_url resolved to docker: %s", _DOCKER_URL)
            _cached_url = _DOCKER_URL
        else:
            logger.debug(
                "docker probe failed within %.1fs, falling back to %s",
                _PROBE_TIMEOUT_S,
                _LOCALHOST_URL,
            )
            _cached_url = _LOCALHOST_URL

        return _cached_url


async def get_base_url() -> str:
    """Return the resolved (and cached) backend base URL.

    Safe to call from multiple coroutines concurrently; resolution runs exactly once.
    """
    if _cached_url is not None:
        return _cached_url
    return await _resolve()


def reset_cache() -> None:
    """Reset the cached URL — for use in tests only."""
    global _cached_url  # noqa: PLW0603
    _cached_url = None
