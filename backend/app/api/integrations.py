"""Integration key management — per-agent encrypted API keys.

Endpoints (all require bearer auth):
  GET    /v1/integrations/keys            — list agent's keys (masked)
  PUT    /v1/integrations/keys/{kind}     — upsert a key
  DELETE /v1/integrations/keys/{kind}     — remove a key
  POST   /v1/integrations/keys/{kind}/test — verify a key works
"""

from __future__ import annotations

import logging
import time
from datetime import datetime, timezone
from typing import Any

import httpx
from fastapi import APIRouter, Depends, HTTPException, Request, status
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from app.core.auth import CurrentAgent
from app.core.db import get_session
from app.models.integration import ALLOWED_KINDS, IntegrationKey
from app.services.crypto import decrypt, encrypt

try:
    from app.core.rate_limit import limiter as _limiter
    _rate_limit_available = True
except ImportError:
    _rate_limit_available = False

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/integrations", tags=["integrations"])


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _mask(value: str) -> str:
    """Return a masked representation: first4…last4, or **** if short."""
    if len(value) > 12:
        return f"{value[:4]}…{value[-4:]}"
    return "****"


def _key_out(row: IntegrationKey) -> dict[str, Any]:
    try:
        plaintext = decrypt(row.ciphertext)
        masked = _mask(plaintext)
    except Exception:  # noqa: BLE001
        logger.warning(
            "Decryption failed for integration key agent=%s kind=%s — ciphertext may be corrupt",
            row.agent_id,
            row.kind,
        )
        masked = "****"
    return {
        "kind": row.kind,
        "masked": masked,
        "set_at": row.updated_at.isoformat(),
        "last_used_at": row.last_used_at.isoformat() if row.last_used_at else None,
    }


async def _get_row(
    agent: str,
    kind: str,
    session: AsyncSession,
) -> IntegrationKey | None:
    stmt = select(IntegrationKey).where(
        IntegrationKey.agent_id == agent,
        IntegrationKey.kind == kind,
    )
    result = await session.execute(stmt)
    return result.scalar_one_or_none()


def _validate_kind(kind: str) -> None:
    if kind not in ALLOWED_KINDS:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail={"error": "invalid_kind", "allowed": sorted(ALLOWED_KINDS)},
        )


# ---------------------------------------------------------------------------
# Schemas
# ---------------------------------------------------------------------------


class KeyUpsertBody(BaseModel):
    value: str


# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------


@router.get("/keys")
async def list_keys(
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> list[dict[str, Any]]:
    """Return all integration keys for the current agent (values masked)."""
    stmt = select(IntegrationKey).where(IntegrationKey.agent_id == agent)
    result = await session.execute(stmt)
    rows = result.scalars().all()
    return [_key_out(r) for r in rows]


@router.put("/keys/{kind}")
async def upsert_key(
    kind: str,
    body: KeyUpsertBody,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> dict[str, Any]:
    """Store (or update) an encrypted key. Returns the masked value."""
    _validate_kind(kind)
    if not body.value.strip():
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail={"error": "value_empty"},
        )

    ciphertext = encrypt(body.value)
    now = datetime.now(timezone.utc)

    # Use dialect-native upsert to avoid a read-then-INSERT race under
    # concurrent requests for the same (agent_id, kind).
    try:
        from sqlalchemy.dialects.sqlite import insert as sqlite_insert

        stmt = (
            sqlite_insert(IntegrationKey)
            .values(
                agent_id=agent,
                kind=kind,
                ciphertext=ciphertext,
                created_at=now,
                updated_at=now,
            )
            .on_conflict_do_update(
                index_elements=["agent_id", "kind"],
                set_={"ciphertext": ciphertext, "updated_at": now},
            )
        )
        await session.execute(stmt)
        await session.commit()
    except ImportError:
        # Non-SQLite dialect: fall back to optimistic read-then-write with a
        # single IntegrityError retry (catches the race window).
        for attempt in range(2):
            try:
                row = await _get_row(agent, kind, session)
                if row is None:
                    row = IntegrationKey(
                        agent_id=agent,
                        kind=kind,
                        ciphertext=ciphertext,
                        created_at=now,
                        updated_at=now,
                    )
                    session.add(row)
                else:
                    row.ciphertext = ciphertext
                    row.updated_at = now
                await session.commit()
                break
            except IntegrityError:
                await session.rollback()
                if attempt == 1:
                    raise

    # Re-fetch to return consistent data regardless of which path ran.
    row = await _get_row(agent, kind, session)
    assert row is not None  # just inserted / updated

    logger.info("Integration key upserted: agent=%s kind=%s", agent, kind)
    return _key_out(row)


@router.delete("/keys/{kind}", status_code=status.HTTP_204_NO_CONTENT, response_model=None)
async def delete_key(
    kind: str,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> None:
    """Remove an integration key for the current agent."""
    _validate_kind(kind)
    row = await _get_row(agent, kind, session)
    if row is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail={"error": "key_not_found", "kind": kind},
        )
    await session.delete(row)
    await session.commit()
    logger.info("Integration key deleted: agent=%s kind=%s", agent, kind)


from app.core.rate_limit import maybe_limit


@router.post("/keys/{kind}/test")
@maybe_limit("10/minute")
async def test_key(
    request: Request,
    kind: str,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> dict[str, Any]:
    """Verify the key works by hitting the provider API. Returns {ok, latency_ms, error?}.

    Rate limited: 10/minute per Authorization header when slowapi is installed.
    """
    _validate_kind(kind)

    row = await _get_row(agent, kind, session)
    if row is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail={"error": "key_not_found", "kind": kind},
        )

    try:
        plaintext = decrypt(row.ciphertext)
    except Exception as exc:  # noqa: BLE001
        logger.warning(
            "Decryption failed during test for agent=%s kind=%s — ciphertext may be corrupt",
            agent,
            kind,
        )
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail={"error": "decryption_failed"},
        ) from exc

    t0 = time.monotonic()

    if kind == "tinker":
        url = f"{_tinker_base()}/jobs"
        headers = {"Authorization": f"Bearer {plaintext}"}
        params = {"limit": 1}
        try:
            async with httpx.AsyncClient(timeout=10.0) as client:
                r = await client.get(url, headers=headers, params=params)
            latency_ms = int((time.monotonic() - t0) * 1000)
            if r.status_code >= 400:
                return {"ok": False, "latency_ms": latency_ms, "error": f"HTTP {r.status_code}"}
            return {"ok": True, "latency_ms": latency_ms}
        except Exception as exc:  # noqa: BLE001
            latency_ms = int((time.monotonic() - t0) * 1000)
            return {"ok": False, "latency_ms": latency_ms, "error": str(exc)}

    elif kind == "openrouter":
        url = "https://openrouter.ai/api/v1/models"
        headers = {"Authorization": f"Bearer {plaintext}"}
        try:
            async with httpx.AsyncClient(timeout=10.0) as client:
                r = await client.get(url, headers=headers)
            latency_ms = int((time.monotonic() - t0) * 1000)
            if r.status_code >= 400:
                return {"ok": False, "latency_ms": latency_ms, "error": f"HTTP {r.status_code}"}
            return {"ok": True, "latency_ms": latency_ms}
        except Exception as exc:  # noqa: BLE001
            latency_ms = int((time.monotonic() - t0) * 1000)
            return {"ok": False, "latency_ms": latency_ms, "error": str(exc)}

    # Should not reach here after _validate_kind but be exhaustive.
    raise HTTPException(status_code=400, detail={"error": "unsupported_kind"})


def _tinker_base() -> str:
    from app.core.config import settings
    return settings.tinker_base_url.rstrip("/")
