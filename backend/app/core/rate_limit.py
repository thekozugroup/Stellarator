"""Shared SlowAPI rate-limiter instance.

Key function: the Authorization header value (bearer token) so each agent
gets its own bucket; falls back to the string "anon" for unauthenticated
requests so they share a single restrictive bucket.

**Storage backend:** reads ``STELLARATOR_RATE_LIMIT_STORAGE`` env. If unset,
defaults to in-memory — which means multi-worker deployments effectively
multiply each limit by worker count. For accurate enforcement under
``uvicorn --workers N`` (or behind gunicorn), set this to a redis URI such
as ``redis://localhost:6379/0`` (requires ``slowapi[redis]`` extras).

Usage in a router::

    from app.core.rate_limit import limiter, maybe_limit

    @router.post("/sensitive")
    @maybe_limit("10/minute")
    async def my_view(request: Request, ...):
        ...

Wire SlowAPIMiddleware in main.py (already done via the try/except pattern).
"""

from __future__ import annotations

import logging
import os
from typing import Any, Callable

from slowapi import Limiter

logger = logging.getLogger(__name__)


def _key_from_auth(request) -> str:  # type: ignore[type-arg]
    return request.headers.get("authorization", "anon")


_storage_uri = os.environ.get("STELLARATOR_RATE_LIMIT_STORAGE", "").strip()
if _storage_uri:
    limiter = Limiter(key_func=_key_from_auth, storage_uri=_storage_uri)
else:
    limiter = Limiter(key_func=_key_from_auth)
    if int(os.environ.get("WEB_CONCURRENCY", "1")) > 1:
        logger.warning(
            "Rate limiter using in-memory storage with WEB_CONCURRENCY>1; "
            "limits are enforced per-worker. Set STELLARATOR_RATE_LIMIT_STORAGE."
        )


def maybe_limit(spec: str) -> Callable[[Any], Any]:
    """Apply a SlowAPI limit if slowapi is installed; no-op otherwise.

    Use this instead of redefining a local ``_test_key_decorator`` lambda in
    every router. Behavior matches ``limiter.limit(spec)`` when slowapi is
    available.
    """
    try:
        return limiter.limit(spec)
    except Exception:  # pragma: no cover - slowapi import-time guard
        return lambda f: f
