"""HTTP surface for run alerts.

  * POST /v1/runs/{run_id}/alerts - training-script ingest (must be the run's
    owner agent for now; a per-run scoped token can be layered later).
  * GET  /v1/runs/{run_id}/alerts - any authenticated agent reads.
"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query, status
from pydantic import BaseModel, Field
from sqlalchemy.ext.asyncio import AsyncSession

from app.core.auth import CurrentAgent, require_owner
from app.core.db import get_session
from app.models.run import Run
from app.services import alerts as alerts_service
from app.services import notifications as notif_service

router = APIRouter(prefix="/runs", tags=["alerts"])


class AlertIn(BaseModel):
    level: str = Field(pattern=r"^(error|warn|info)$")
    title: str = Field(min_length=1, max_length=200)
    body: str = Field(default="")
    source: str = Field(default="training_script", max_length=64)


class AlertOut(BaseModel):
    id: int
    run_id: str
    level: str
    title: str
    body: str
    source: str
    created_at: datetime


async def _get_run_or_404(run_id: str, session: AsyncSession) -> Run:
    run = await session.get(Run, run_id)
    if run is None:
        raise HTTPException(status.HTTP_404_NOT_FOUND, f"Run '{run_id}' not found")
    return run


@router.post(
    "/{run_id}/alerts",
    response_model=AlertOut,
    status_code=status.HTTP_201_CREATED,
)
async def post_alert(
    run_id: str,
    body: AlertIn,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> AlertOut:
    run = await _get_run_or_404(run_id, session)
    require_owner(run.owner_agent, agent)
    try:
        alert = await alerts_service.record_alert(
            session,
            run_id=run_id,
            level=body.level,
            title=body.title,
            body=body.body,
            source=body.source,
        )
    except alerts_service.AlertValidationError as exc:
        raise HTTPException(status.HTTP_422_UNPROCESSABLE_ENTITY, str(exc)) from exc
    if body.level == "error":
        notif_service.notify_alert_error(
            agent=run.owner_agent,
            run_id=run_id,
            run_name=run.name,
            title=body.title,
        )
    return AlertOut(**alerts_service.alert_to_dict(alert))


@router.get("/{run_id}/alerts", response_model=list[AlertOut])
async def get_alerts(
    run_id: str,
    since: datetime | None = Query(default=None),
    limit: int = Query(default=200, ge=1, le=1000),
    _agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> list[AlertOut]:
    await _get_run_or_404(run_id, session)
    rows = await alerts_service.list_alerts(
        session, run_id=run_id, since=since, limit=limit
    )
    return [AlertOut(**alerts_service.alert_to_dict(r)) for r in rows]


def register_routes() -> APIRouter:
    """Convenience for callers wiring under /v1."""
    return router


__all__ = ["router", "AlertIn", "AlertOut"]
