"""GitHub Code Search + raw file read.

Used by the research sub-agent to find working examples (e.g. "GRPO
TRL training script") and read specific files for hyperparams. Auth is
optional (a GITHUB_TOKEN raises the unauth 10 req/min cap to 30 req/min for
search and 5000/hr for contents); the service degrades gracefully on 429
or network failure by returning an empty list with a structured note.

A small in-process LRU caches responses for 60 seconds so repeated
requests inside a single research session do not consume rate budget.
"""

from __future__ import annotations

import asyncio
import base64
import logging
import time
from collections import OrderedDict
from typing import Any

import httpx

from app.core.config import settings

logger = logging.getLogger(__name__)

GITHUB_API = "https://api.github.com"
_TIMEOUT = httpx.Timeout(connect=5.0, read=15.0, write=5.0, pool=5.0)
_CACHE_TTL_S = 60.0
_CACHE_MAX = 256


class _LRUCache:
    """Tiny TTL+LRU cache. Not thread-safe across event loops, fine here."""

    def __init__(self, maxsize: int = _CACHE_MAX, ttl: float = _CACHE_TTL_S) -> None:
        self._store: OrderedDict[str, tuple[float, Any]] = OrderedDict()
        self._maxsize = maxsize
        self._ttl = ttl

    def get(self, key: str) -> Any | None:
        item = self._store.get(key)
        if item is None:
            return None
        ts, value = item
        if time.monotonic() - ts > self._ttl:
            self._store.pop(key, None)
            return None
        self._store.move_to_end(key)
        return value

    def set(self, key: str, value: Any) -> None:
        self._store[key] = (time.monotonic(), value)
        self._store.move_to_end(key)
        while len(self._store) > self._maxsize:
            self._store.popitem(last=False)


_cache = _LRUCache()
_lock = asyncio.Lock()


def _headers() -> dict[str, str]:
    h = {
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "Stellarator/0.1 (+https://stellarator.dev)",
    }
    token = settings.github_token
    if token:
        h["Authorization"] = f"Bearer {token}"
    return h


class GitHubService:
    """Thin async wrapper. All methods return JSON-serializable dicts/lists."""

    async def find_examples(
        self, query: str, lang: str = "python", limit: int = 5
    ) -> dict[str, Any]:
        """Search code via GitHub Code Search API.

        Returns ``{"items": [...], "note": str | None}``. ``items`` is a
        normalized list of ``{repo, path, html_url, score}``.
        """
        cache_key = f"search::{lang}::{limit}::{query}"
        if cached := _cache.get(cache_key):
            return cached

        q = f"{query} language:{lang}"
        params = {"q": q, "per_page": min(limit, 30)}

        async with _lock:
            try:
                async with httpx.AsyncClient(timeout=_TIMEOUT) as client:
                    r = await client.get(
                        f"{GITHUB_API}/search/code",
                        params=params,
                        headers=_headers(),
                    )
            except httpx.HTTPError as exc:
                logger.warning("github search transport error: %s", exc)
                return {"items": [], "note": f"transport_error:{type(exc).__name__}"}

        if r.status_code == 401:
            return {"items": [], "note": "unauthorized; set GITHUB_TOKEN"}
        if r.status_code == 403 or r.status_code == 429:
            return {"items": [], "note": "rate_limited"}
        if r.status_code == 422:
            return {"items": [], "note": "invalid_query"}
        if r.status_code >= 400:
            return {"items": [], "note": f"http_{r.status_code}"}

        try:
            payload = r.json()
        except ValueError:
            return {"items": [], "note": "invalid_json"}

        items = []
        for raw in payload.get("items", [])[:limit]:
            repo = raw.get("repository", {}).get("full_name", "")
            items.append(
                {
                    "repo": repo,
                    "path": raw.get("path", ""),
                    "html_url": raw.get("html_url", ""),
                    "score": raw.get("score", 0.0),
                }
            )
        result = {"items": items, "note": None}
        _cache.set(cache_key, result)
        return result

    async def read_file(
        self, repo: str, path: str, ref: str = "main"
    ) -> dict[str, Any]:
        """Fetch a file's text content. Returns ``{"content": str, "note": str|None}``.

        Empty content + note on any failure (404, oversize, binary, network).
        """
        cache_key = f"file::{repo}::{path}::{ref}"
        if cached := _cache.get(cache_key):
            return cached

        url = f"{GITHUB_API}/repos/{repo}/contents/{path}"
        params = {"ref": ref}

        try:
            async with httpx.AsyncClient(timeout=_TIMEOUT) as client:
                r = await client.get(url, params=params, headers=_headers())
        except httpx.HTTPError as exc:
            logger.warning("github read transport error: %s", exc)
            return {"content": "", "note": f"transport_error:{type(exc).__name__}"}

        if r.status_code == 404:
            return {"content": "", "note": "not_found"}
        if r.status_code in (401, 403, 429):
            return {"content": "", "note": f"http_{r.status_code}"}
        if r.status_code >= 400:
            return {"content": "", "note": f"http_{r.status_code}"}

        try:
            payload = r.json()
        except ValueError:
            return {"content": "", "note": "invalid_json"}

        if isinstance(payload, list):
            return {"content": "", "note": "is_directory"}

        encoding = payload.get("encoding")
        raw = payload.get("content", "")
        if encoding == "base64":
            try:
                decoded = base64.b64decode(raw).decode("utf-8", errors="replace")
            except (ValueError, UnicodeDecodeError):
                return {"content": "", "note": "decode_error"}
        else:
            decoded = str(raw)

        # Cap returned text so we don't blow the agent's context window.
        max_chars = 16000
        truncated = decoded[:max_chars]
        note = "truncated" if len(decoded) > max_chars else None
        result = {
            "content": truncated,
            "note": note,
            "size": payload.get("size"),
            "html_url": payload.get("html_url"),
        }
        _cache.set(cache_key, result)
        return result


github = GitHubService()
