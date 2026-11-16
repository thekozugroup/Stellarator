from __future__ import annotations

from fastapi import APIRouter, Depends, HTTPException, Query
from sqlalchemy import String, cast, func, select
from sqlalchemy.ext.asyncio import AsyncSession

from ..db import get_session
from ..models import Prompt, PromptSet, Run
from ..schemas import Page, PromptOut, PromptSetOut, PromptSetPatch

router = APIRouter(prefix="/api/prompt-sets", tags=["prompt sets"])


async def _run_counts(session: AsyncSession) -> dict[int, int]:
    rows = await session.execute(select(Run.prompt_set_id, func.count()).group_by(Run.prompt_set_id))
    return {pid: n for pid, n in rows.all()}


def to_out(ps: PromptSet, run_count: int = 0) -> PromptSetOut:
    out = PromptSetOut.model_validate(ps)
    out.run_count = run_count
    return out


async def _get(session: AsyncSession, set_id: int) -> PromptSet:
    ps = await session.get(PromptSet, set_id)
    if ps is None:
        raise HTTPException(404, "Prompt set not found")
    return ps


@router.get("", response_model=list[PromptSetOut])
async def list_prompt_sets(session: AsyncSession = Depends(get_session)):
    sets = (await session.execute(select(PromptSet).order_by(PromptSet.created_at.desc()))).scalars().all()
    counts = await _run_counts(session)
    return [to_out(ps, counts.get(ps.id, 0)) for ps in sets]


@router.get("/{set_id}", response_model=PromptSetOut)
async def get_prompt_set(set_id: int, session: AsyncSession = Depends(get_session)):
    ps = await _get(session, set_id)
    return to_out(ps, (await _run_counts(session)).get(ps.id, 0))


@router.patch("/{set_id}", response_model=PromptSetOut)
async def rename_prompt_set(set_id: int, body: PromptSetPatch, session: AsyncSession = Depends(get_session)):
    ps = await _get(session, set_id)
    ps.name = body.name
    await session.commit()
    return to_out(ps, (await _run_counts(session)).get(ps.id, 0))


@router.get("/{set_id}/prompts", response_model=Page[PromptOut])
async def list_prompts(
    set_id: int,
    offset: int = Query(0, ge=0),
    limit: int = Query(50, ge=1, le=500),
    q: str | None = Query(None, description="Case-insensitive text search"),
    session: AsyncSession = Depends(get_session),
):
    await _get(session, set_id)
    where = [Prompt.prompt_set_id == set_id]
    if q:
        where.append(func.lower(cast(Prompt.messages, String)).contains(q.lower()))
    total = (await session.execute(select(func.count()).select_from(Prompt).where(*where))).scalar_one()
    rows = (
        await session.execute(select(Prompt).where(*where).order_by(Prompt.idx).offset(offset).limit(limit))
    ).scalars().all()
    return Page[PromptOut](
        items=[PromptOut.model_validate(p) for p in rows], total=total, offset=offset, limit=limit
    )


@router.delete("/{set_id}", status_code=204)
async def delete_prompt_set(set_id: int, session: AsyncSession = Depends(get_session)):
    from ..services.engine import engine_registry

    ps = await _get(session, set_id)
    if any(r.prompt_set_id == set_id for r in engine_registry.active_runs()):
        raise HTTPException(409, "A run on this prompt set is still going. Pause it first.")
    # Runs, samples and preference sets on this prompt set are deleted with it (cascade).
    await session.delete(ps)
    await session.commit()
