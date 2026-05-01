"""Thin async HTTP client for the Stellarator backend.

Auth: Bearer token from STELLARATOR_TOKEN env var.
Retries transient errors (502/503/504) up to 3 times with exponential backoff.
Maps HTTP errors to human-readable messages suitable for MCP tool errors.
Base URL is resolved via stellarator_mcp.config (env → docker probe → localhost).
"""

from __future__ import annotations

import asyncio
import logging
import os
from typing import Any

import httpx

from stellarator_mcp.config import get_base_url

logger = logging.getLogger(__name__)

_RETRYABLE = {502, 503, 504}
_MAX_RETRIES = 3
_BACKOFF_BASE = 0.5  # seconds


def _get_token() -> str:
    token = os.environ.get("STELLARATOR_TOKEN", "")
    if not token:
        raise RuntimeError(
            "STELLARATOR_TOKEN is not set. "
            "Export the bearer token before starting the MCP server."
        )
    return token


def _map_error(resp: httpx.Response) -> str:
    """Return a human-readable error string for the LLM.

    The bearer token must never appear in this string; we only include the URL
    (which is safe) and HTTP-level detail.
    """
    code = resp.status_code
    if code == 401:
        # Do NOT include Authorization header or its value.
        return (
            "HTTP 401 Unauthorized — check that STELLARATOR_TOKEN is correct "
            f"and has not expired. Backend: {resp.url}"
        )
    if code == 403:
        return (
            "HTTP 403 Forbidden — this run is owned by another agent and cannot "
            "be modified by claude-code. Inspect the run's owner field before retrying."
        )
    if code == 404:
        return f"HTTP 404 Not Found — {resp.url}"
    if code == 422:
        try:
            detail = resp.json().get("detail", resp.text)
        except (ValueError, KeyError):
            detail = resp.text
        return f"HTTP 422 Validation error: {detail}"
    try:
        body = resp.json()
    except ValueError:
        body = resp.text
    return f"HTTP {code}: {body}"


class StellaratorClient:
    """Async client for the Stellarator /v1 API."""

    def __init__(self, base_url: str | None = None) -> None:
        # Explicit override wins (used in tests); otherwise resolved lazily.
        self._explicit_base_url: str | None = base_url.rstrip("/") if base_url else None

    async def _resolved_base_url(self) -> str:
        if self._explicit_base_url is not None:
            return self._explicit_base_url
        return (await get_base_url()).rstrip("/")

    async def _build_client(self) -> httpx.AsyncClient:
        token = _get_token()
        base = await self._resolved_base_url()
        return httpx.AsyncClient(
            base_url=base,
            headers={"Authorization": f"Bearer {token}"},
            timeout=httpx.Timeout(30.0),
        )

    async def _request(
        self,
        method: str,
        path: str,
        *,
        params: dict[str, Any] | None = None,
        json: Any = None,
    ) -> Any:
        attempt = 0
        last_err: Exception | None = None
        async with await self._build_client() as client:
            while attempt < _MAX_RETRIES:
                try:
                    resp = await client.request(
                        method, path, params=params, json=json
                    )
                    if resp.status_code in _RETRYABLE and attempt < _MAX_RETRIES - 1:
                        attempt += 1
                        await asyncio.sleep(_BACKOFF_BASE * (2 ** attempt))
                        continue
                    if resp.is_error:
                        raise StellaratorAPIError(_map_error(resp), resp.status_code)
                    if resp.status_code == 204 or not resp.content:
                        return {}
                    return resp.json()
                except StellaratorAPIError:
                    raise
                except httpx.TransportError as exc:
                    last_err = exc
                    attempt += 1
                    if attempt < _MAX_RETRIES:
                        await asyncio.sleep(_BACKOFF_BASE * (2 ** attempt))
        raise StellaratorAPIError(
            f"Connection failed after {_MAX_RETRIES} retries: {type(last_err).__name__}",
            status_code=None,
        )

    # ------------------------------------------------------------------ runs

    async def create_run(self, body: dict[str, Any]) -> dict[str, Any]:
        return await self._request("POST", "/v1/runs", json=body)

    async def list_runs(
        self,
        owner: str | None = None,
        status: str | None = None,
        limit: int | None = None,
    ) -> list[dict[str, Any]]:
        params: dict[str, Any] = {}
        if owner is not None:
            params["owner"] = owner
        if status is not None:
            params["status"] = status
        if limit is not None:
            params["limit"] = limit
        return await self._request("GET", "/v1/runs", params=params)

    async def get_run(self, run_id: str) -> dict[str, Any]:
        return await self._request("GET", f"/v1/runs/{run_id}")

    async def cancel_run(self, run_id: str) -> dict[str, Any]:
        return await self._request("POST", f"/v1/runs/{run_id}/cancel")

    async def pause_run(self, run_id: str) -> dict[str, Any]:
        return await self._request("POST", f"/v1/runs/{run_id}/pause")

    async def resume_run(self, run_id: str) -> dict[str, Any]:
        return await self._request("POST", f"/v1/runs/{run_id}/resume")

    async def add_note(self, run_id: str, kind: str, body: str) -> dict[str, Any]:
        return await self._request(
            "POST", f"/v1/runs/{run_id}/notes", json={"kind": kind, "body": body}
        )

    # -------------------------------------------------------------- research

    async def search_papers(
        self, q: str, source: str = "both"
    ) -> list[dict[str, Any]]:
        return await self._request(
            "GET", "/v1/research/papers/search", params={"q": q, "source": source}
        )

    async def get_paper(self, source: str, paper_id: str) -> dict[str, Any]:
        return await self._request("GET", f"/v1/research/papers/{source}/{paper_id}")

    async def cite_paper(
        self, run_id: str, source: str, paper_id: str, note: str
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/v1/research/runs/{run_id}/cite",
            json={"source": source, "paper_id": paper_id, "note": note},
        )

    async def get_checkpoint(self, run_id: str) -> dict[str, Any]:
        """Return checkpoint readiness for *run_id*.

        Fetches the run and extracts status + checkpoint_url, returning:
        ``{"run_id": ..., "status": ..., "checkpoint_url": str|null, "ready": bool}``
        """
        run = await self._request("GET", f"/v1/runs/{run_id}")
        run_status = run.get("status", "")
        ready = run_status == "succeeded"
        url = run.get("checkpoint_url") if ready else None
        return {
            "run_id": run_id,
            "status": run_status,
            "checkpoint_url": url,
            "ready": ready,
        }

    async def pick_environment(self, env_id: str) -> dict[str, Any]:
        """Return the environment recipe for *env_id* from the backend registry.

        Delegates to GET /v1/environments/{env_id} on the backend, which
        wraps the environments catalog lookup helper.
        """
        return await self._request("GET", f"/v1/environments/{env_id}")

    # NOTE: /v1/environments/{env_id} must be registered in the backend API.
    # The backend app/api/runs.py (or a dedicated router) exposes this endpoint
    # by calling app.services.environments.pick_environment.


class StellaratorAPIError(Exception):
    """Raised when the backend returns an error response."""

    def __init__(self, message: str, status_code: int | None) -> None:
        super().__init__(message)
        self.status_code = status_code
