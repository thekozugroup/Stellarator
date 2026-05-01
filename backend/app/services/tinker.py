"""Thin wrapper around the Tinker REST API.

Tinker exposes a Python SDK; we call its HTTP surface directly so the same
container can be driven from any agent runtime. Endpoints follow the public
docs at https://thinkingmachines.ai/tinker/. Adjust paths if the SDK changes.

Key resolution order (per-request):
  1. Per-agent IntegrationKey row (encrypted at rest) — preferred.
  2. Global env TINKER_API_KEY fallback.
  3. Neither present → TinkerKeyMissing (HTTP 412 at the API layer).
"""

from __future__ import annotations

import logging
from datetime import datetime, timezone
from typing import Any

import httpx
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from tenacity import retry, stop_after_attempt, wait_exponential_jitter

from app.core.config import settings

logger = logging.getLogger(__name__)


class TinkerError(RuntimeError):
    pass


class TinkerKeyMissing(TinkerError):
    """Raised when no Tinker API key is available for the given agent."""

    pass


async def _resolve_key(agent: str | None, session: AsyncSession | None = None) -> str:
    """Resolve the Tinker API key for *agent*.

    Resolution order:
      1. Per-agent IntegrationKey row (if agent and session provided).
      2. Global settings.tinker_api_key.
      3. Raise TinkerKeyMissing.

    The *session* parameter exists so callers without DB access (e.g. tests)
    can pass ``None`` to skip the DB lookup and rely on the env fallback.
    """
    if agent and session is not None:
        from app.models.integration import IntegrationKey  # local to avoid circular

        stmt = select(IntegrationKey).where(
            IntegrationKey.agent_id == agent,
            IntegrationKey.kind == "tinker",
        )
        result = await session.execute(stmt)
        row: IntegrationKey | None = result.scalar_one_or_none()
        if row is not None:
            from app.services.crypto import decrypt

            plaintext = decrypt(row.ciphertext)
            # Bump last_used_at — best-effort; don't let a commit failure kill the request.
            try:
                row.last_used_at = datetime.now(timezone.utc)
                await session.commit()
            except Exception:  # noqa: BLE001
                logger.warning("Failed to update last_used_at for agent=%s kind=tinker", agent)
                await session.rollback()
            return plaintext

    fallback = settings.tinker_api_key
    if fallback:
        return fallback

    raise TinkerKeyMissing(
        "No Tinker API key configured. Set TINKER_API_KEY in .env or add a per-agent key "
        "via PUT /v1/integrations/keys/tinker."
    )


class TinkerClient:
    """Async Tinker API client.

    Unlike the previous design, auth headers are built per-request so each
    call can use the right key for its agent context.
    """

    def __init__(self) -> None:
        # Shared transport/connection pool — no auth header baked in.
        self._client = httpx.AsyncClient(
            base_url=settings.tinker_base_url,
            timeout=60.0,
        )

    async def aclose(self) -> None:
        await self._client.aclose()

    @retry(stop=stop_after_attempt(3), wait=wait_exponential_jitter(initial=1, max=8))
    async def _request(
        self,
        method: str,
        path: str,
        *,
        api_key: str,
        **kw: Any,
    ) -> dict[str, Any]:
        headers = {"Authorization": f"Bearer {api_key}"}
        r = await self._client.request(method, path, headers=headers, **kw)
        if r.status_code >= 400:
            raise TinkerError(f"{method} {path} -> {r.status_code}: {r.text}")
        return r.json() if r.content else {}

    async def _key(
        self,
        agent: str | None,
        session: AsyncSession | None,
    ) -> str:
        return await _resolve_key(agent, session)

    async def create_job(
        self,
        *,
        base_model: str,
        method: str,
        hyperparams: dict[str, Any],
        dataset_mixture: list[dict[str, Any]],
        gpu_type: str,
        gpu_count: int,
        agent: str | None = None,
        session: AsyncSession | None = None,
    ) -> dict[str, Any]:
        api_key = await self._key(agent, session)
        payload = {
            "base_model": base_model,
            "method": method,
            "hyperparameters": hyperparams,
            "datasets": dataset_mixture,
            "compute": {"gpu_type": gpu_type, "gpu_count": gpu_count},
        }
        return await self._request("POST", "/jobs", api_key=api_key, json=payload)

    async def get_job(
        self,
        job_id: str,
        *,
        agent: str | None = None,
        session: AsyncSession | None = None,
    ) -> dict[str, Any]:
        api_key = await self._key(agent, session)
        return await self._request("GET", f"/jobs/{job_id}", api_key=api_key)

    async def list_jobs(
        self,
        *,
        limit: int = 10,
        agent: str | None = None,
        session: AsyncSession | None = None,
    ) -> dict[str, Any]:
        api_key = await self._key(agent, session)
        return await self._request(
            "GET", "/jobs", api_key=api_key, params={"limit": limit}
        )

    async def cancel_job(
        self,
        job_id: str,
        *,
        agent: str | None = None,
        session: AsyncSession | None = None,
    ) -> dict[str, Any]:
        api_key = await self._key(agent, session)
        return await self._request("POST", f"/jobs/{job_id}/cancel", api_key=api_key)

    async def pause_job(
        self,
        job_id: str,
        *,
        agent: str | None = None,
        session: AsyncSession | None = None,
    ) -> dict[str, Any]:
        api_key = await self._key(agent, session)
        return await self._request("POST", f"/jobs/{job_id}/pause", api_key=api_key)

    async def resume_job(
        self,
        job_id: str,
        *,
        agent: str | None = None,
        session: AsyncSession | None = None,
    ) -> dict[str, Any]:
        api_key = await self._key(agent, session)
        return await self._request("POST", f"/jobs/{job_id}/resume", api_key=api_key)

    async def stream_metrics(
        self,
        job_id: str,
        *,
        agent: str | None = None,
        session: AsyncSession | None = None,
    ):
        api_key = await self._key(agent, session)
        headers = {"Authorization": f"Bearer {api_key}"}
        async with self._client.stream(
            "GET", f"/jobs/{job_id}/metrics/stream", headers=headers
        ) as r:
            async for line in r.aiter_lines():
                if line.strip():
                    yield line


tinker = TinkerClient()
