"""Run management endpoints."""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
import uuid
from typing import Annotated

import httpx
from fastapi import APIRouter, Depends, HTTPException, Query, WebSocket, WebSocketDisconnect, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.orm import selectinload

from app.agents import preflight as preflight_module
from app.core.auth import CurrentAgent, require_owner
from app.core.config import settings
from app.core.db import get_session
from app.core.semaphores import ws_connection_slots
from app.models.run import Run, RunNote, RunStatus
from app.services import cost as cost_service
from app.services import notifications as notif_service
from app.services.reconcile import reconciliation_loop
from app.services.tinker import TinkerError, TinkerKeyMissing, tinker

from .schemas import NoteCreate, RunCreate, RunDetail, RunNoteOut, RunOut, PromoteCreate

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/runs", tags=["runs"])

SUPERVISOR_BASE = "http://supervisor:8001"

_WS_CLOSE_UNAUTHORIZED = 4401

# ---------------------------------------------------------------------------
# Reconciliation background task — wired via router lifecycle so we don't
# need to touch app/main.py. The task is cancelled on shutdown.
# ---------------------------------------------------------------------------
_reconcile_task: "asyncio.Task[None] | None" = None


@router.on_event("startup")
async def _start_reconciliation_task() -> None:
    global _reconcile_task
    import asyncio as _asyncio

    if _reconcile_task is None or _reconcile_task.done():
        _reconcile_task = _asyncio.create_task(
            reconciliation_loop(), name="stellarator-reconcile"
        )
        logger.info("Reconciliation task scheduled")


@router.on_event("shutdown")
async def _stop_reconciliation_task() -> None:
    global _reconcile_task
    import asyncio as _asyncio

    task = _reconcile_task
    if task is None or task.done():
        return
    task.cancel()
    try:
        await task
    except _asyncio.CancelledError:
        pass
    except Exception:  # noqa: BLE001
        logger.exception("Reconciliation task raised on shutdown")
    _reconcile_task = None


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


async def _get_run_or_404(run_id: str, session: AsyncSession) -> Run:
    run = await session.get(Run, run_id)
    if run is None:
        raise HTTPException(status.HTTP_404_NOT_FOUND, f"Run '{run_id}' not found")
    return run


class _SupervisorMisconfigured(Exception):
    """Raised when supervisor returns 401/403 (shared-secret mismatch)."""


async def _hand_off_to_supervisor(run_id: str, tinker_job_id: str | None) -> None:
    """POST to Rust supervisor with shared-secret auth.

    Error handling:
    - 401/403: log ERROR with misconfiguration hint, raise _SupervisorMisconfigured.
    - 5xx / network error: retry up to 3 times with backoff (0.5s, 1s, 2s).
      If all retries fail, log WARNING and return (reconcile will catch up).
    - 2xx: return immediately.
    """
    headers: dict[str, str] = {}
    if settings.supervisor_shared_secret:
        headers["X-Supervisor-Token"] = settings.supervisor_shared_secret

    _RETRY_DELAYS = (0.5, 1.0, 2.0)
    _MAX_ATTEMPTS = len(_RETRY_DELAYS) + 1  # 4 total: initial + 3 retries

    last_exc: Exception | None = None

    for attempt in range(_MAX_ATTEMPTS):
        try:
            async with httpx.AsyncClient(
                timeout=httpx.Timeout(connect=5.0, read=5.0, write=5.0, pool=5.0)
            ) as client:
                r = await client.post(
                    f"{SUPERVISOR_BASE}/supervisor/track",
                    json={"run_id": run_id, "tinker_job_id": tinker_job_id},
                    headers=headers,
                )

                if r.status_code in (401, 403):
                    logger.error(
                        "Supervisor handoff auth failure (HTTP %d) for run %s — "
                        "possible shared secret mismatch between backend and supervisor. "
                        "Check SUPERVISOR_SHARED_SECRET on both services.",
                        r.status_code,
                        run_id,
                    )
                    raise _SupervisorMisconfigured(
                        f"Supervisor returned {r.status_code} for run {run_id}"
                    )

                r.raise_for_status()
                # 2xx — success.
                return

        except _SupervisorMisconfigured:
            raise
        except (httpx.HTTPStatusError, httpx.RequestError) as exc:
            last_exc = exc
            if attempt < len(_RETRY_DELAYS):
                delay = _RETRY_DELAYS[attempt]
                logger.warning(
                    "Supervisor handoff attempt %d/%d failed for run %s: %s — retrying in %.1fs",
                    attempt + 1, _MAX_ATTEMPTS, run_id, exc, delay,
                )
                await asyncio.sleep(delay)
            else:
                logger.warning(
                    "Supervisor handoff failed after %d attempts for run %s: %s — "
                    "reconcile loop will re-track.",
                    _MAX_ATTEMPTS, run_id, exc,
                )


# ---------------------------------------------------------------------------
# POST / — create run
# ---------------------------------------------------------------------------


@router.post("/", response_model=RunOut, status_code=status.HTTP_201_CREATED)
async def create_run(
    body: RunCreate,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> RunOut:
    run_id = uuid.uuid4().hex

    # ---- Pre-flight gate (ML Intern pattern) -----------------------------
    # Sandbox runs bypass; scale runs must carry a validated preflight_json
    # AND a fresh sandbox lineage from the same agent.
    if not body.is_sandbox and preflight_module.is_scale_request(
        body.gpu_type, body.gpu_count
    ):
        try:
            pf = preflight_module.parse_preflight(body.preflight_json)
            await preflight_module.validate_sandbox_lineage(pf, agent, session)
            # Cross-check the body's sandbox_run_id matches preflight's.
            if body.sandbox_run_id and body.sandbox_run_id != pf.sandbox_run_id:
                raise preflight_module.PreflightError(
                    "sandbox_id_mismatch",
                    "body.sandbox_run_id != preflight.sandbox_run_id",
                )
        except preflight_module.PreflightError as exc:
            logger.info(
                "preflight rejected agent=%s code=%s run_id=%s",
                agent, exc.code, run_id,
            )
            raise HTTPException(status_code=412, detail=exc.to_dict()) from exc

    # Budget pre-check (BEFORE any external job creation).
    projected = cost_service.projected_total_for(body)
    is_within, info = await cost_service.check_budget(session, agent, projected)
    if not is_within:
        logger.warning(
            "Budget exceeded for agent=%s projected=%.2f info=%s run_id=%s",
            agent, projected, info, run_id,
        )
        raise HTTPException(
            status_code=402,
            detail={
                "error": "budget_exceeded",
                "run_id": run_id,
                "projected_run_cost_usd": projected,
                **(info or {}),
            },
        )

    # Call Tinker
    tinker_job_id: str | None = None
    try:
        job = await tinker.create_job(
            base_model=body.base_model,
            method=body.method,
            hyperparams=body.hyperparams,
            dataset_mixture=[d.model_dump() for d in body.dataset_mixture],
            gpu_type=body.gpu_type,
            gpu_count=body.gpu_count,
            agent=agent,
            session=session,
        )
        tinker_job_id = job.get("id") or job.get("job_id")
    except TinkerKeyMissing as exc:
        logger.warning("Tinker key missing for agent=%s: %s", agent, exc)
        raise HTTPException(
            status_code=412,
            detail={
                "error": "tinker_key_missing",
                "hint": "Set Tinker API key in /settings or TINKER_API_KEY env var",
            },
        ) from exc
    except TinkerError as exc:
        logger.warning("Tinker create_job failed: %s", exc)
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail={"error": "tinker_unavailable", "message": str(exc)},
        ) from exc

    run = Run(
        id=run_id,
        owner_agent=agent,
        name=body.name,
        base_model=body.base_model,
        method=body.method,
        hyperparams=body.hyperparams,
        dataset_mixture=[d.model_dump() for d in body.dataset_mixture],
        gpu_type=body.gpu_type,
        gpu_count=body.gpu_count,
        user_goal=body.user_goal,
        user_context=body.user_context,
        agent_plan=body.agent_plan,
        citations=body.citations,
        tinker_job_id=tinker_job_id,
        status=RunStatus.queued.value,
        is_sandbox=body.is_sandbox,
        preflight_json=body.preflight_json,
        parent_run_id=body.sandbox_run_id,
    )
    session.add(run)
    await session.commit()
    await session.refresh(run)

    # Supervisor handoff — may roll back run on auth misconfiguration.
    try:
        await _hand_off_to_supervisor(run_id, tinker_job_id)
    except _SupervisorMisconfigured as exc:
        # Best-effort cancel the Tinker job so it doesn't run orphaned.
        if tinker_job_id:
            try:
                await tinker.cancel_job(tinker_job_id, agent=agent, session=session)
            except Exception as cancel_exc:  # noqa: BLE001
                logger.warning(
                    "Tinker cancel_job failed during rollback for run %s job %s: %s",
                    run_id, tinker_job_id, cancel_exc,
                )
        # Roll back the persisted run so the caller is not left with an
        # orphaned queued row that will never progress.
        await session.delete(run)
        await session.commit()
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail={"error": "supervisor_misconfigured"},
        ) from exc

    return RunOut.model_validate(run)


# ---------------------------------------------------------------------------
# GET / — list runs
# ---------------------------------------------------------------------------


@router.get("/", response_model=list[RunOut])
async def list_runs(
    owner: str | None = Query(default=None),
    run_status: str | None = Query(default=None, alias="status"),
    limit: int = Query(default=50, ge=1, le=500),
    _agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> list[RunOut]:
    stmt = select(Run)
    if owner:
        stmt = stmt.where(Run.owner_agent == owner)
    if run_status:
        valid = {s.value for s in RunStatus}
        if run_status not in valid:
            raise HTTPException(status.HTTP_422_UNPROCESSABLE_ENTITY, f"status must be one of {sorted(valid)}")
        stmt = stmt.where(Run.status == run_status)
    stmt = stmt.order_by(Run.created_at.desc()).limit(limit)
    result = await session.execute(stmt)
    runs = result.scalars().all()
    return [RunOut.model_validate(r) for r in runs]


# ---------------------------------------------------------------------------
# GET /{run_id} — detail with notes + last 200 metrics
# ---------------------------------------------------------------------------


@router.get("/{run_id}", response_model=RunDetail)
async def get_run(
    run_id: str,
    _agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> RunDetail:
    stmt = (
        select(Run)
        .where(Run.id == run_id)
        .options(
            selectinload(Run.notes),
            selectinload(Run.metrics),
        )
    )
    result = await session.execute(stmt)
    run = result.scalar_one_or_none()
    if run is None:
        raise HTTPException(status.HTTP_404_NOT_FOUND, f"Run '{run_id}' not found")

    notes = [RunNoteOut.model_validate(n) for n in run.notes]
    from app.api.schemas import RunMetricOut
    metrics = sorted(run.metrics, key=lambda m: m.id)[-200:]
    metrics_out = [RunMetricOut.model_validate(m) for m in metrics]

    detail = RunDetail.model_validate(run)
    detail.notes = notes
    detail.metrics = metrics_out
    return detail


# ---------------------------------------------------------------------------
# POST /{run_id}/cancel
# ---------------------------------------------------------------------------


@router.post("/{run_id}/cancel", response_model=RunOut)
async def cancel_run(
    run_id: str,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> RunOut:
    run = await _get_run_or_404(run_id, session)
    require_owner(run.owner_agent, agent)

    if run.tinker_job_id:
        try:
            await tinker.cancel_job(run.tinker_job_id, agent=agent, session=session)
        except TinkerError as exc:
            logger.warning("Tinker cancel_job failed for %s: %s", run_id, exc)

    run.status = RunStatus.cancelled.value
    await session.commit()
    await session.refresh(run)
    notif_service.notify_run_finished(
        agent=run.owner_agent,
        run_id=run.id,
        run_name=run.name,
        status=RunStatus.cancelled.value,
        is_sandbox=run.is_sandbox,
    )
    return RunOut.model_validate(run)


# ---------------------------------------------------------------------------
# POST /{run_id}/pause
# ---------------------------------------------------------------------------


@router.post("/{run_id}/pause", response_model=RunOut)
async def pause_run(
    run_id: str,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> RunOut:
    run = await _get_run_or_404(run_id, session)
    require_owner(run.owner_agent, agent)

    if run.tinker_job_id:
        try:
            await tinker.pause_job(run.tinker_job_id, agent=agent, session=session)
        except TinkerError as exc:
            logger.warning("Tinker pause_job failed for %s: %s", run_id, exc)

    run.status = RunStatus.paused.value
    await session.commit()
    await session.refresh(run)
    return RunOut.model_validate(run)


# ---------------------------------------------------------------------------
# POST /{run_id}/resume
# ---------------------------------------------------------------------------


@router.post("/{run_id}/resume", response_model=RunOut)
async def resume_run(
    run_id: str,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> RunOut:
    run = await _get_run_or_404(run_id, session)
    require_owner(run.owner_agent, agent)

    if run.tinker_job_id:
        try:
            await tinker.resume_job(run.tinker_job_id, agent=agent, session=session)
        except TinkerError as exc:
            logger.warning("Tinker resume_job failed for %s: %s", run_id, exc)

    run.status = RunStatus.running.value
    await session.commit()
    await session.refresh(run)
    return RunOut.model_validate(run)


# ---------------------------------------------------------------------------
# POST /{sandbox_id}/promote — promote a succeeded sandbox to production scale
# ---------------------------------------------------------------------------


@router.post("/{sandbox_id}/promote", response_model=RunOut, status_code=status.HTTP_201_CREATED)
async def promote_run(
    sandbox_id: str,
    body: "PromoteCreate",
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> RunOut:
    """Promote a succeeded sandbox run to a production-scale run.

    Builds a Preflight automatically from the sandbox's recorded metrics,
    hyperparams, dataset_mixture, and citations, then routes through the
    standard preflight-gate create flow.
    """
    from app.agents.preflight import Preflight, PreflightCitation, PreflightDataset, PreflightError

    sandbox = await _get_run_or_404(sandbox_id, session)
    require_owner(sandbox.owner_agent, agent)

    if not sandbox.is_sandbox:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail={"error": "not_a_sandbox", "message": "Source run must be a sandbox run."},
        )
    if sandbox.status != RunStatus.succeeded.value:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail={
                "error": "sandbox_not_succeeded",
                "message": f"Sandbox status is '{sandbox.status}'; must be 'succeeded'.",
            },
        )

    # Build sandbox_summary from recorded metrics
    from app.models.run import RunMetric
    from sqlalchemy import select as _select

    metrics_result = await session.execute(
        _select(RunMetric)
        .where(RunMetric.run_id == sandbox_id)
        .order_by(RunMetric.id.desc())
        .limit(50)
    )
    metrics_rows = list(metrics_result.scalars().all())
    last_loss = next(
        (m.value for m in metrics_rows if "loss" in m.name.lower()), None
    )
    last_step = max((m.step for m in metrics_rows), default=0)
    sandbox_summary = (
        f"Sandbox {sandbox_id[:8]} completed {last_step} steps"
        + (f"; final loss {last_loss:.4f}" if last_loss is not None else "")
        + "."
    )

    # Merge hyperparams
    merged_hyperparams: dict = {**(sandbox.hyperparams or {}), **body.hyperparams_overrides}

    # Build Preflight from sandbox lineage
    try:
        pf_citations = [
            PreflightCitation(
                source=c.get("source", "web"),
                id=c.get("id", "unknown"),
                title=c.get("title", "unknown"),
                note=c.get("note", ""),
            )
            for c in (sandbox.citations or [])
        ]
        pf_datasets = [
            PreflightDataset(
                name=d.get("name", ""),
                weight=float(d.get("weight", 1.0)),
                source=d.get("source", ""),
            )
            for d in (sandbox.dataset_mixture or [])
        ]
        preflight = Preflight(
            model=sandbox.base_model,
            method=sandbox.method,
            dataset_mixture=pf_datasets,
            hyperparams=merged_hyperparams,
            sandbox_run_id=sandbox_id,
            sandbox_summary=sandbox_summary,
            projected_cost_usd=0.0,
            citations=pf_citations,
        )
    except Exception as exc:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail={"error": "preflight_build_failed", "message": str(exc)},
        ) from exc

    # Validate lineage (ownership + freshness)
    try:
        await preflight_module.validate_sandbox_lineage(preflight, agent, session)
    except PreflightError as exc:
        raise HTTPException(status_code=412, detail=exc.to_dict()) from exc

    run_id = uuid.uuid4().hex
    preflight_dict = preflight.model_dump()

    # Budget check
    from app.api.schemas import RunCreate as _RunCreate, DatasetEntry as _DatasetEntry
    _dummy_body = _RunCreate(
        name=body.name,
        base_model=sandbox.base_model,
        method=sandbox.method,
        hyperparams=merged_hyperparams,
        dataset_mixture=[
            _DatasetEntry(name=d.name, weight=d.weight, source=d.source)
            for d in pf_datasets
        ],
        gpu_type=body.gpu_type,
        gpu_count=body.gpu_count,
        user_goal=body.user_goal or sandbox.user_goal,
        is_sandbox=False,
    )
    projected = cost_service.projected_total_for(_dummy_body)
    is_within, info = await cost_service.check_budget(session, agent, projected)
    if not is_within:
        raise HTTPException(
            status_code=402,
            detail={"error": "budget_exceeded", "projected_run_cost_usd": projected, **(info or {})},
        )

    # Tinker
    tinker_job_id: str | None = None
    try:
        job = await tinker.create_job(
            base_model=sandbox.base_model,
            method=sandbox.method,
            hyperparams=merged_hyperparams,
            dataset_mixture=list(sandbox.dataset_mixture or []),
            gpu_type=body.gpu_type,
            gpu_count=body.gpu_count,
            agent=agent,
            session=session,
        )
        tinker_job_id = job.get("id") or job.get("job_id")
    except TinkerKeyMissing as exc:
        raise HTTPException(
            status_code=412,
            detail={"error": "tinker_key_missing"},
        ) from exc
    except TinkerError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail={"error": "tinker_unavailable", "message": str(exc)},
        ) from exc

    run = Run(
        id=run_id,
        owner_agent=agent,
        name=body.name,
        base_model=sandbox.base_model,
        method=sandbox.method,
        hyperparams=merged_hyperparams,
        dataset_mixture=list(sandbox.dataset_mixture or []),
        gpu_type=body.gpu_type,
        gpu_count=body.gpu_count,
        user_goal=body.user_goal or sandbox.user_goal,
        user_context=sandbox.user_context,
        agent_plan=sandbox.agent_plan,
        citations=list(sandbox.citations or []),
        tinker_job_id=tinker_job_id,
        status=RunStatus.queued.value,
        is_sandbox=False,
        preflight_json=preflight_dict,
        parent_run_id=sandbox_id,
    )
    session.add(run)
    await session.commit()
    await session.refresh(run)

    try:
        await _hand_off_to_supervisor(run_id, tinker_job_id)
    except _SupervisorMisconfigured as exc:
        if tinker_job_id:
            with contextlib.suppress(Exception):
                await tinker.cancel_job(tinker_job_id, agent=agent, session=session)
        await session.delete(run)
        await session.commit()
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail={"error": "supervisor_misconfigured"},
        ) from exc

    return RunOut.model_validate(run)


# ---------------------------------------------------------------------------
# POST /{run_id}/notes
# ---------------------------------------------------------------------------


@router.post("/{run_id}/notes", response_model=RunNoteOut, status_code=status.HTTP_201_CREATED)
async def add_note(
    run_id: str,
    body: NoteCreate,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> RunNoteOut:
    run = await _get_run_or_404(run_id, session)
    require_owner(run.owner_agent, agent)

    note = RunNote(
        run_id=run_id,
        author_agent=agent,
        kind=body.kind,
        body=body.body,
    )
    session.add(note)
    await session.commit()
    await session.refresh(note)
    return RunNoteOut.model_validate(note)


# ---------------------------------------------------------------------------
# GET /{run_id}/notes
# ---------------------------------------------------------------------------


@router.get("/{run_id}/notes", response_model=list[RunNoteOut])
async def list_notes(
    run_id: str,
    _agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> list[RunNoteOut]:
    await _get_run_or_404(run_id, session)
    stmt = select(RunNote).where(RunNote.run_id == run_id).order_by(RunNote.id)
    result = await session.execute(stmt)
    notes = result.scalars().all()
    return [RunNoteOut.model_validate(n) for n in notes]


# ---------------------------------------------------------------------------
# WebSocket /{run_id}/stream — proxy Rust supervisor WS
# ---------------------------------------------------------------------------


@router.websocket("/{run_id}/stream")
async def stream_run(
    run_id: str,
    websocket: WebSocket,
    token: str | None = Query(default=None),
) -> None:
    """Authenticated WS proxy.

    Browsers cannot set ``Authorization`` on WebSocket handshakes, so the
    token is passed as ``?token=...``. We validate via the same constant-
    time path as the HTTP auth dependency, then close with code 4401 on
    any mismatch.

    Slot enforcement uses PerAgentSemaphore (per-agent=16, global=64).
    We attempt a non-blocking acquire (0.001s timeout) so we never stall
    the handshake when capacity is full.
    """
    agent = settings.agent_for_token(token or "")
    if agent is None:
        # Per RFC 6455 we must accept before sending a custom close code.
        await websocket.accept()
        await websocket.close(code=_WS_CLOSE_UNAUTHORIZED, reason="invalid token")
        return

    # Non-blocking slot check: if we can't acquire within 1ms the server is busy.
    # The acquire is atomic: global+per-agent are both held or global is released
    # on per-agent failure inside PerAgentSemaphore.acquire().
    _slot_acquired = False
    try:
        await asyncio.wait_for(ws_connection_slots.acquire(agent), timeout=0.001)
        _slot_acquired = True
    except asyncio.TimeoutError:
        await websocket.accept()
        await websocket.close(code=1013, reason="server busy")
        return

    try:
        await websocket.accept()
        supervisor_url = f"ws://supervisor:8001/ws/runs/{run_id}"
        try:
            import websockets  # optional dep; graceful error if missing

            async with websockets.connect(supervisor_url) as upstream:
                async for message in upstream:
                    await websocket.send_text(
                        message if isinstance(message, str) else message.decode()
                    )
        except WebSocketDisconnect:
            pass
        except ImportError:
            await websocket.send_text('{"error": "websockets package not installed on server"}')
            await websocket.close()
        except Exception as exc:  # noqa: BLE001
            logger.warning("WS proxy error for run %s: %s", run_id, exc)
            try:
                await websocket.send_text(
                    json.dumps({
                        "error": "upstream connection failed",
                        "exception_type": type(exc).__name__,
                    })
                )
            except Exception:  # noqa: BLE001
                pass
            await websocket.close()
    finally:
        if _slot_acquired:
            # Shield the release so task cancellation doesn't prevent cleanup.
            async def _release() -> None:
                with contextlib.suppress(Exception):
                    ws_connection_slots.release(agent)

            with contextlib.suppress(Exception):
                await asyncio.shield(_release())


# ---------------------------------------------------------------------------
# POST /preflight/validate — pure validation (no run created)
# ---------------------------------------------------------------------------


@router.post("/preflight/validate")
async def validate_preflight(
    body: dict,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
) -> dict:
    """Validate a preflight payload without creating a run.

    Body shape: ``{"preflight_json": {...}, "planned_name": "..."}``.
    Returns ``{"ok": true, "sandbox_run_id": ...}`` on success or a 412
    with the same error envelope as ``POST /v1/runs/`` rejection.
    """
    raw = body.get("preflight_json")
    try:
        pf = preflight_module.parse_preflight(raw if isinstance(raw, dict) else None)
        await preflight_module.validate_sandbox_lineage(pf, agent, session)
    except preflight_module.PreflightError as exc:
        raise HTTPException(status_code=412, detail=exc.to_dict()) from exc
    return {
        "ok": True,
        "sandbox_run_id": pf.sandbox_run_id,
        "planned_name": body.get("planned_name", ""),
    }
