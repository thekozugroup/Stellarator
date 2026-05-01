"""Background reconciliation loop.

Every STELLARATOR_RECONCILE_INTERVAL_SECS (default 30):
  1. Scan runs where status in (queued, running) and tinker_job_id IS NOT NULL.
  2. Refresh each run's status via tinker.get_job; persist deltas.
  3. Diff against the supervisor's tracked-run list; re-issue /supervisor/track
     for any running orphan jobs.
  4. POST /supervisor/untrack/{run_id} when a run reaches a terminal status
     and is still in supervisor's tracked set.

Exponential backoff:
  - Consecutive supervisor-status failures increase sleep up to 300s.
  - Consecutive tinker.get_job failures per-run increase wait up to 300s.
  Both counters reset independently on success.

Idempotent: re-tracking is safe; status writes only happen on real change.
Cancellable: the lifespan task cancels on shutdown, and we swallow
asyncio.CancelledError cleanly.

Environment variables
---------------------
STELLARATOR_RECONCILE_INTERVAL_SECS
    Base interval between reconciliation passes. Default: 30.
"""

from __future__ import annotations

import asyncio
import logging
import os
import time
from datetime import datetime
from typing import Any

import httpx
from sqlalchemy import select

from sqlalchemy import delete

from app.core.db import SessionLocal
from app.models.oauth_state import OAuthState
from app.models.run import Run, RunStatus
from app.services.tinker import TinkerError, tinker

logger = logging.getLogger(__name__)

SUPERVISOR_BASE = "http://supervisor:8001"
HTTP_TIMEOUT = 5.0

_RECONCILE_INTERVAL_SECS_DEFAULT = 30.0
_BACKOFF_CAP = 300.0

_TERMINAL = {RunStatus.succeeded.value, RunStatus.failed.value, RunStatus.cancelled.value}
_ACTIVE = (RunStatus.queued.value, RunStatus.running.value)


def _read_interval() -> float:
    raw = os.environ.get("STELLARATOR_RECONCILE_INTERVAL_SECS", "")
    try:
        val = float(raw)
        if val > 0:
            return val
    except (ValueError, TypeError):
        pass
    return _RECONCILE_INTERVAL_SECS_DEFAULT


def _backoff(base: float, failures: int) -> float:
    return min(base * (2 ** failures), _BACKOFF_CAP)


async def _list_supervisor_tracked(
    client: httpx.AsyncClient,
    supervisor_token: str | None,
) -> set[str]:
    """Return the set of run_ids currently tracked by the supervisor.

    Tolerates several response shapes: {"runs":[{"run_id":...}]} or
    {"tracked":["id1", ...]}. Returns empty set on any error.
    """
    headers: dict[str, str] = {}
    if supervisor_token:
        headers["X-Supervisor-Token"] = supervisor_token
    try:
        r = await client.post(
            f"{SUPERVISOR_BASE}/supervisor/status",
            headers=headers,
        )
        r.raise_for_status()
        data: Any = r.json()
        if isinstance(data, dict):
            if "runs" in data and isinstance(data["runs"], list):
                return {
                    str(item.get("run_id"))
                    for item in data["runs"]
                    if isinstance(item, dict) and item.get("run_id")
                }
            if "tracked" in data and isinstance(data["tracked"], list):
                return {str(x) for x in data["tracked"]}
        if isinstance(data, list):
            return {str(x) for x in data}
    except Exception as exc:  # noqa: BLE001
        logger.warning("supervisor status fetch failed: %s", exc)
        raise  # re-raise so caller can count consecutive failures
    return set()


async def _retrack(
    client: httpx.AsyncClient,
    run_id: str,
    tinker_job_id: str | None,
    supervisor_token: str | None,
) -> None:
    headers: dict[str, str] = {}
    if supervisor_token:
        headers["X-Supervisor-Token"] = supervisor_token
    try:
        r = await client.post(
            f"{SUPERVISOR_BASE}/supervisor/track",
            json={"run_id": run_id, "tinker_job_id": tinker_job_id},
            headers=headers,
        )
        r.raise_for_status()
        logger.info("Re-tracked orphan run_id=%s tinker_job_id=%s", run_id, tinker_job_id)
    except Exception as exc:  # noqa: BLE001
        logger.warning("Re-track failed for run_id=%s: %s", run_id, exc)


async def _untrack(
    client: httpx.AsyncClient,
    run_id: str,
    supervisor_token: str | None,
) -> None:
    """POST untrack for a run that has reached terminal status."""
    headers: dict[str, str] = {}
    if supervisor_token:
        headers["X-Supervisor-Token"] = supervisor_token
    try:
        r = await client.post(
            f"{SUPERVISOR_BASE}/supervisor/untrack/{run_id}",
            headers=headers,
        )
        r.raise_for_status()
        logger.info("Untracked terminal run_id=%s", run_id)
    except Exception as exc:  # noqa: BLE001
        logger.warning("Untrack failed for run_id=%s: %s", run_id, exc)


async def _refresh_run_status(run: Run) -> str | None:
    """Return the new status string if it changed, else None.

    Raises TinkerError on failure (caller tracks consecutive failures).
    """
    job = await tinker.get_job(run.tinker_job_id)  # type: ignore[arg-type]

    new_status = job.get("status")
    if not isinstance(new_status, str):
        return None
    valid = {s.value for s in RunStatus}
    if new_status not in valid:
        logger.warning("Unknown tinker status=%r for run_id=%s", new_status, run.id)
        return None
    if new_status == run.status:
        return None
    return new_status


async def _reconcile_once(
    supervisor_token: str | None,
    tinker_failures: dict[str, int],
    next_attempt_at: dict[str, float],
    base_interval: float,
) -> None:
    async with SessionLocal() as session:
        stmt = select(Run).where(
            Run.status.in_(_ACTIVE), Run.tinker_job_id.is_not(None)
        )
        result = await session.execute(stmt)
        active_runs: list[Run] = list(result.scalars().all())

        if not active_runs:
            return

        async with httpx.AsyncClient(timeout=HTTP_TIMEOUT) as client:
            try:
                tracked = await _list_supervisor_tracked(client, supervisor_token)
            except Exception:
                # Already logged in _list_supervisor_tracked; propagate so
                # the loop can count supervisor failures.
                raise

            dirty = False
            now = time.monotonic()
            for run in active_runs:
                # Per-run tinker backoff: skip if still within cooldown window.
                if now < next_attempt_at.get(run.id, 0.0):
                    failures = tinker_failures.get(run.id, 0)
                    logger.debug(
                        "Skipping tinker refresh for run_id=%s (consecutive_failures=%d, "
                        "retry_in=%.1fs)",
                        run.id, failures,
                        next_attempt_at[run.id] - now,
                    )
                    # Still apply untrack / re-track logic based on current DB status.
                else:
                    try:
                        new_status = await _refresh_run_status(run)
                        # Success — clear backoff state.
                        tinker_failures.pop(run.id, None)
                        next_attempt_at.pop(run.id, None)
                        if new_status is not None:
                            logger.info(
                                "Run status delta run_id=%s old=%s new=%s",
                                run.id, run.status, new_status,
                            )
                            run.status = new_status
                            dirty = True
                    except TinkerError as exc:
                        failures = tinker_failures.get(run.id, 0) + 1
                        tinker_failures[run.id] = failures
                        backoff_secs = _backoff(base_interval, failures)
                        next_attempt_at[run.id] = time.monotonic() + backoff_secs
                        logger.warning(
                            "tinker.get_job failed for run_id=%s (consecutive=%d, "
                            "next_retry_in=%.1fs): %s",
                            run.id, failures, backoff_secs, exc,
                        )

                # Untrack if the run is now terminal and supervisor still tracks it.
                if run.status in _TERMINAL and run.id in tracked:
                    await _untrack(client, run.id, supervisor_token)

                # Re-track if the run is (still) running but supervisor doesn't know.
                elif run.status == RunStatus.running.value and run.id not in tracked:
                    await _retrack(client, run.id, run.tinker_job_id, supervisor_token)

            if dirty:
                await session.commit()


_OAUTH_STATE_CLEANUP_INTERVAL = 3600.0  # 1 hour between cleanup passes
_OAUTH_STATE_MAX_AGE_SECS = 3600.0     # delete rows older than 1 hour


async def _cleanup_expired_oauth_states() -> None:
    """Delete oauth_state rows that expired more than 1 hour ago."""
    from datetime import timedelta, timezone

    cutoff = datetime.now(tz=timezone.utc) - timedelta(seconds=_OAUTH_STATE_MAX_AGE_SECS)
    try:
        async with SessionLocal() as session:
            result = await session.execute(
                delete(OAuthState).where(OAuthState.expires_at < cutoff)
            )
            deleted = result.rowcount
            await session.commit()
        if deleted:
            logger.info("OAuth state cleanup: deleted %d expired rows", deleted)
    except Exception:
        logger.exception("OAuth state cleanup failed")


async def reconciliation_loop() -> None:
    """Run the reconciliation pass forever, until cancelled.

    Reads STELLARATOR_RECONCILE_INTERVAL_SECS once at startup.
    Applies exponential back-off (capped at 300s) on consecutive supervisor
    failures; resets on success.
    """
    from app.core.config import settings  # local import avoids circular at module load

    base_interval = _read_interval()
    supervisor_token: str | None = settings.supervisor_shared_secret or None

    logger.info("Reconciliation loop started (base_interval=%.1fs)", base_interval)

    supervisor_failures = 0
    # Per-run tinker failure counters — keyed by run.id string.
    tinker_failures: dict[str, int] = {}
    # Per-run next-attempt timestamps (monotonic).
    next_attempt_at: dict[str, float] = {}
    _last_oauth_cleanup = 0.0

    try:
        while True:
            sleep_secs: float
            try:
                await _reconcile_once(supervisor_token, tinker_failures, next_attempt_at, base_interval)
                supervisor_failures = 0
                sleep_secs = base_interval
            except asyncio.CancelledError:
                raise
            except Exception:
                supervisor_failures += 1
                sleep_secs = _backoff(base_interval, supervisor_failures)
                logger.exception(
                    "reconciliation pass failed (consecutive_supervisor_failures=%d, next_sleep=%.1fs)",
                    supervisor_failures, sleep_secs,
                )

            # Periodic OAuth state cleanup (once per hour, best-effort).
            now_mono = time.monotonic()
            if now_mono - _last_oauth_cleanup >= _OAUTH_STATE_CLEANUP_INTERVAL:
                await _cleanup_expired_oauth_states()
                _last_oauth_cleanup = now_mono

            await asyncio.sleep(sleep_secs)
    except asyncio.CancelledError:
        logger.info("Reconciliation loop cancelled; exiting")
        raise
