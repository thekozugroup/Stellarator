"""Alert ingestion + read service.

Training scripts call ``trackio.alert(title, text, level)`` which POSTs to
``/v1/runs/{id}/alerts``. The orchestrator agent reads the alert stream on
each loop iteration and routes:

  * ERROR -> re-research / re-plan
  * WARN  -> tweak hyperparams (lr, batch size, etc.)
  * INFO  -> milestone, no action
"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.models.alert import ALERT_LEVELS, RunAlert


class AlertValidationError(ValueError):
    """Raised when an alert payload is malformed."""


async def record_alert(
    session: AsyncSession,
    *,
    run_id: str,
    level: str,
    title: str,
    body: str = "",
    source: str = "training_script",
) -> RunAlert:
    if level not in ALERT_LEVELS:
        raise AlertValidationError(f"level must be one of {ALERT_LEVELS}, got {level!r}")
    if not title.strip():
        raise AlertValidationError("title is required")
    alert = RunAlert(
        run_id=run_id,
        level=level,
        title=title[:200],
        body=body,
        source=source[:64],
        created_at=datetime.utcnow(),
    )
    session.add(alert)
    await session.commit()
    await session.refresh(alert)
    return alert


async def list_alerts(
    session: AsyncSession,
    *,
    run_id: str,
    since: datetime | None = None,
    limit: int = 200,
) -> list[RunAlert]:
    stmt = select(RunAlert).where(RunAlert.run_id == run_id)
    if since is not None:
        stmt = stmt.where(RunAlert.created_at > since)
    stmt = stmt.order_by(RunAlert.created_at.asc(), RunAlert.id.asc()).limit(limit)
    result = await session.execute(stmt)
    return list(result.scalars().all())


def alert_to_dict(alert: RunAlert) -> dict[str, Any]:
    return {
        "id": alert.id,
        "run_id": alert.run_id,
        "level": alert.level,
        "title": alert.title,
        "body": alert.body,
        "source": alert.source,
        "created_at": alert.created_at.isoformat() if alert.created_at else None,
    }
