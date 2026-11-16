"""Runs: create, watch, steer (pause, resume, concurrency, retry failed) and browse samples."""

from __future__ import annotations

import math
from datetime import datetime, timezone
from typing import Literal

from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.exceptions import RequestValidationError
from sqlalchemy import and_, delete, func, insert, or_, select, text
from sqlalchemy.ext.asyncio import AsyncSession

from ..db import get_session
from ..models import (
    RUN_RUNNING,
    SAMPLE_DONE,
    SAMPLE_PENDING,
    Endpoint,
    Prompt,
    PromptSet,
    Run,
    RunMetric,
    Sample,
    utcnow,
)
from ..schemas import (
    MetricPoint,
    Page,
    RunCreate,
    RunLive,
    RunOut,
    RunParams,
    RunPatch,
    SampleOut,
    SampleSummary,
)
from ..services.engine import EngineConflict, engine_registry, preview
from ..services.generation import last_user_text, prepare_messages

router = APIRouter(prefix="/api", tags=["runs"])

INSERT_BATCH = 5000
MAX_METRIC_POINTS = 240


def _utc(dt: datetime | None) -> datetime | None:
    """SQLite drops the timezone; every timestamp we store is UTC."""
    if dt is None:
        return None
    return dt if dt.tzinfo is not None else dt.replace(tzinfo=timezone.utc)


def run_out(run: Run, prompt_set_name: str | None) -> RunOut:
    in_flight = engine_registry.in_flight(run.id)
    return RunOut(
        id=run.id,
        name=run.name,
        endpoint_id=run.endpoint_id,
        endpoint_name=run.endpoint_name,
        model=run.model,
        prompt_set_id=run.prompt_set_id,
        prompt_set_name=prompt_set_name or "",
        params=RunParams.model_validate(run.params or {}),
        concurrency=run.concurrency,
        status=run.status,  # type: ignore[arg-type]
        status_reason=run.status_reason,
        total=run.total,
        done=run.done,
        failed=run.failed,
        truncated=run.truncated,
        pending=max(0, run.total - run.done - run.failed - in_flight),
        prompt_tokens=run.prompt_tokens,
        completion_tokens=run.completion_tokens,
        reasoning_tokens=run.reasoning_tokens,
        active_seconds=round(run.active_seconds + engine_registry.unsaved_active_seconds(run.id), 3),
        created_at=_utc(run.created_at),  # type: ignore[arg-type]
        started_at=_utc(run.started_at),
        finished_at=_utc(run.finished_at),
    )


async def _load(session: AsyncSession, run_id: int) -> tuple[Run, str | None]:
    row = (
        await session.execute(
            select(Run, PromptSet.name)
            .outerjoin(PromptSet, PromptSet.id == Run.prompt_set_id)
            .where(Run.id == run_id)
            .execution_options(populate_existing=True)
        )
    ).first()
    if row is None:
        raise HTTPException(404, "Run not found")
    return row[0], row[1]


async def _out(session: AsyncSession, run_id: int) -> RunOut:
    run, name = await _load(session, run_id)
    out = run_out(run, name)
    await session.rollback()  # release the connection; nothing to write
    return out


async def _require(session: AsyncSession, run_id: int) -> None:
    exists = (await session.execute(select(Run.id).where(Run.id == run_id))).scalar_one_or_none()
    await session.rollback()  # do not hold a connection while the engine works
    if exists is None:
        raise HTTPException(404, "Run not found")


def _field_error(field: str, message: str) -> RequestValidationError:
    return RequestValidationError([{"loc": ("body", field), "msg": message, "type": "value_error"}])


# --------------------------------------------------------------------------- runs


@router.get("/runs", response_model=list[RunOut])
async def list_runs(session: AsyncSession = Depends(get_session)):
    rows = (
        await session.execute(
            select(Run, PromptSet.name)
            .outerjoin(PromptSet, PromptSet.id == Run.prompt_set_id)
            .order_by(Run.created_at.desc(), Run.id.desc())
        )
    ).all()
    return [run_out(run, name) for run, name in rows]


@router.post("/runs", response_model=RunOut, status_code=201)
async def create_run(body: RunCreate, session: AsyncSession = Depends(get_session)):
    endpoint = await session.get(Endpoint, body.endpoint_id)
    if endpoint is None:
        raise _field_error("endpoint_id", "This endpoint does not exist. Pick another endpoint.")
    prompt_set = await session.get(PromptSet, body.prompt_set_id)
    if prompt_set is None:
        raise _field_error("prompt_set_id", "This prompt set does not exist. Pick another prompt set.")
    prompts = (
        await session.execute(
            select(Prompt.id, Prompt.idx)
            .where(Prompt.prompt_set_id == prompt_set.id)
            .order_by(Prompt.idx, Prompt.id)
        )
    ).all()
    if not prompts:
        raise _field_error("prompt_set_id", "This prompt set has no prompts. Import prompts into it first.")

    per_prompt = body.params.samples_per_prompt
    name = (body.name or "").strip() or f"{body.model} on {prompt_set.name}"
    now = utcnow()
    run = Run(
        name=name[:300],
        endpoint_id=endpoint.id,
        endpoint_name=endpoint.name,
        model=body.model,
        prompt_set_id=prompt_set.id,
        params=body.params.model_dump(),
        concurrency=body.concurrency,
        status=RUN_RUNNING,
        total=len(prompts) * per_prompt,
        created_at=now,
        started_at=now,
    )
    session.add(run)
    await session.flush()

    # Insert in (prompt_idx, sample_idx) order, so sample ids follow prompt order. The engine relies
    # on this to take pending samples in prompt order straight from the (run_id, status) index.
    table = Sample.__table__
    batch: list[dict] = []
    for prompt_id, idx in prompts:
        for k in range(per_prompt):
            batch.append(
                {
                    "run_id": run.id,
                    "prompt_id": prompt_id,
                    "prompt_idx": idx,
                    "sample_idx": k,
                    "status": SAMPLE_PENDING,
                    "attempts": 0,
                }
            )
            if len(batch) >= INSERT_BATCH:
                await session.execute(insert(table), batch)
                batch = []
    if batch:
        await session.execute(insert(table), batch)
    await session.commit()
    run_id = run.id

    try:
        await engine_registry.start(run_id)
    except EngineConflict as exc:  # the endpoint vanished between the check and the start
        raise HTTPException(409, str(exc)) from None
    return await _out(session, run_id)


@router.get("/runs/{run_id}", response_model=RunOut)
async def get_run(run_id: int, session: AsyncSession = Depends(get_session)):
    return await _out(session, run_id)


@router.patch("/runs/{run_id}", response_model=RunOut)
async def update_run(run_id: int, body: RunPatch, session: AsyncSession = Depends(get_session)):
    run, _ = await _load(session, run_id)
    if body.name is not None:
        run.name = body.name.strip() or run.name
    if body.concurrency is not None:
        run.concurrency = body.concurrency
    await session.commit()
    if body.concurrency is not None:
        engine_registry.set_concurrency(run_id, body.concurrency)
    return await _out(session, run_id)


@router.delete("/runs/{run_id}", status_code=204)
async def delete_run(run_id: int, session: AsyncSession = Depends(get_session)):
    await _require(session, run_id)
    await engine_registry.stop(run_id)
    # Samples and metrics go with it (ON DELETE CASCADE).
    await session.execute(delete(Run).where(Run.id == run_id))
    await session.commit()


@router.post("/runs/{run_id}/pause", response_model=RunOut)
async def pause_run(run_id: int, session: AsyncSession = Depends(get_session)):
    await _require(session, run_id)
    try:
        await engine_registry.pause(run_id)
    except EngineConflict as exc:
        raise HTTPException(409, str(exc)) from None
    return await _out(session, run_id)


@router.post("/runs/{run_id}/resume", response_model=RunOut)
async def resume_run(run_id: int, session: AsyncSession = Depends(get_session)):
    await _require(session, run_id)
    try:
        await engine_registry.resume(run_id)
    except EngineConflict as exc:
        raise HTTPException(409, str(exc)) from None
    return await _out(session, run_id)


@router.post("/runs/{run_id}/retry-failed", response_model=RunOut)
async def retry_failed(run_id: int, session: AsyncSession = Depends(get_session)):
    await _require(session, run_id)
    try:
        await engine_registry.retry_failed(run_id)
    except EngineConflict as exc:
        raise HTTPException(409, str(exc)) from None
    return await _out(session, run_id)


@router.get("/runs/{run_id}/live", response_model=RunLive)
async def run_live(run_id: int, session: AsyncSession = Depends(get_session)):
    run, name = await _load(session, run_id)
    rows = (
        await session.execute(
            select(
                RunMetric.t,
                RunMetric.tokens_per_s,
                RunMetric.samples_per_min,
                RunMetric.active,
                RunMetric.concurrency,
                RunMetric.done,
                RunMetric.failed,
            )
            .where(RunMetric.run_id == run_id)
            .order_by(RunMetric.t, RunMetric.id)
        )
    ).all()
    out = run_out(run, name)
    await session.rollback()
    if len(rows) > MAX_METRIC_POINTS:
        stride = math.ceil(len(rows) / MAX_METRIC_POINTS)
        last = rows[-1]
        rows = rows[::stride]
        if rows[-1] is not last:
            if len(rows) >= MAX_METRIC_POINTS:
                rows[-1] = last
            else:
                rows.append(last)
    metrics = [
        MetricPoint(
            t=_utc(t),  # type: ignore[arg-type]
            tokens_per_s=tps,
            samples_per_min=spm,
            active=active,
            concurrency=conc,
            done=done,
            failed=failed,
        )
        for t, tps, spm, active, conc, done, failed in rows
    ]
    live = engine_registry.live(run_id)
    if live is None:
        return RunLive(
            run=out,
            in_flight=0,
            sequences=[],
            tokens_per_s=0.0,
            samples_per_min=0.0,
            eta_seconds=None,
            metrics=metrics,
        )
    return RunLive(run=out, metrics=metrics, **live)


# --------------------------------------------------------------------------- samples


SampleFilter = Literal["pending", "running", "done", "failed", "truncated"]


def _like_pattern(q: str) -> str:
    escaped = q.lower().replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
    return f"%{escaped}%"


@router.get("/runs/{run_id}/samples", response_model=Page[SampleSummary])
async def list_samples(
    run_id: int,
    status: SampleFilter | None = Query(None),
    offset: int = Query(0, ge=0),
    limit: int = Query(50, ge=1, le=500),
    q: str | None = Query(None, description="Case-insensitive search in the prompt and the response"),
    session: AsyncSession = Depends(get_session),
):
    exists = (await session.execute(select(Run.id).where(Run.id == run_id))).scalar_one_or_none()
    if exists is None:
        raise HTTPException(404, "Run not found")
    where = [Sample.run_id == run_id]
    if status == "truncated":
        where += [Sample.status == SAMPLE_DONE, Sample.finish_reason == "length"]
    elif status:
        where.append(Sample.status == status)
    needs_prompt = False
    if q and q.strip():
        pattern = _like_pattern(q.strip())
        in_prompt = text(
            "EXISTS (SELECT 1 FROM json_each(prompts.messages) AS m "
            "WHERE lower(json_extract(m.value, '$.content')) LIKE :q_pattern ESCAPE '\\')"
        ).bindparams(q_pattern=pattern)
        in_response = func.lower(Sample.response).like(pattern, escape="\\")
        where.append(or_(in_prompt, in_response))
        needs_prompt = True

    count_q = select(func.count()).select_from(Sample)
    if needs_prompt:
        count_q = count_q.join(Prompt, Prompt.id == Sample.prompt_id)
    total = (await session.execute(count_q.where(*where))).scalar_one()

    has_reasoning = and_(Sample.reasoning.is_not(None), func.length(Sample.reasoning) > 0)
    rows = (
        await session.execute(
            select(
                Sample.id,
                Sample.prompt_idx,
                Sample.sample_idx,
                Sample.status,
                func.substr(Sample.response, 1, 400),
                has_reasoning,
                Sample.finish_reason,
                Sample.completion_tokens,
                Sample.latency_ms,
                Sample.error,
                Prompt.messages,
            )
            .join(Prompt, Prompt.id == Sample.prompt_id)
            .where(*where)
            .order_by(Sample.prompt_idx, Sample.sample_idx, Sample.id)
            .offset(offset)
            .limit(limit)
        )
    ).all()
    items = [
        SampleSummary(
            id=sid,
            prompt_idx=pidx,
            sample_idx=sidx,
            status=st,
            prompt_preview=preview(last_user_text(messages or [])),
            response_preview=None if response is None else preview(response),
            has_reasoning=bool(reasoning),
            finish_reason=finish,
            completion_tokens=ctoks,
            latency_ms=lat,
            error=err,
        )
        for sid, pidx, sidx, st, response, reasoning, finish, ctoks, lat, err, messages in rows
    ]
    return Page[SampleSummary](items=items, total=total, offset=offset, limit=limit)


@router.get("/samples/{sample_id}", response_model=SampleOut)
async def get_sample(sample_id: int, session: AsyncSession = Depends(get_session)):
    sample = await session.get(Sample, sample_id)
    if sample is None:
        raise HTTPException(404, "Sample not found")
    prompt = await session.get(Prompt, sample.prompt_id)
    run = await session.get(Run, sample.run_id)
    params = RunParams.model_validate((run.params if run else None) or {})
    out = SampleOut.model_validate(sample)
    out.completed_at = _utc(sample.completed_at)
    out.messages = prepare_messages(prompt.messages if prompt else [], params)
    return out
