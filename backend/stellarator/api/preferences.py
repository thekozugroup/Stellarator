"""Preference sets: pick a chosen and a rejected response per prompt, by hand or by rule.

A preference set groups one or more runs over the same prompt set. For each prompt, the candidates
are every finished (status "done") sample from those runs. Only prompts with at least two
candidates can be paired; those are the set's prompts.
"""

from __future__ import annotations

from typing import Literal

from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.exceptions import RequestValidationError
from sqlalchemy import and_, delete, func, select
from sqlalchemy.dialects.sqlite import insert as sqlite_insert
from sqlalchemy.ext.asyncio import AsyncSession

from ..db import get_session
from ..models import (
    SAMPLE_DONE,
    PreferencePair,
    PreferenceSet,
    Prompt,
    PromptSet,
    Run,
    Sample,
    utcnow,
)
from ..schemas import (
    AutoPairIn,
    AutoPairOut,
    Candidate,
    Page,
    PairIn,
    PreferenceItem,
    PreferenceSetCreate,
    PreferenceSetOut,
)

router = APIRouter(prefix="/api", tags=["preferences"])

ItemFilter = Literal["all", "undecided", "decided"]


# --------------------------------------------------------------------------- helpers


async def _get_set(session: AsyncSession, set_id: int) -> PreferenceSet:
    ps = await session.get(PreferenceSet, set_id)
    if ps is None:
        raise HTTPException(404, "Preference set not found. It may have been deleted.")
    return ps


async def _runs_of(session: AsyncSession, ps: PreferenceSet) -> list[Run]:
    """The set's runs that still exist, in the order they were added."""
    ids = [int(i) for i in ps.run_ids or []]
    if not ids:
        return []
    rows = (await session.execute(select(Run).where(Run.id.in_(ids)))).scalars().all()
    by_id = {r.id: r for r in rows}
    return [by_id[i] for i in ids if i in by_id]


def _eligible_prompts(run_ids: list[int]):
    """Subquery of prompt ids with at least two finished samples across the given runs."""
    return (
        select(Sample.prompt_id.label("prompt_id"))
        .where(Sample.run_id.in_(run_ids), Sample.status == SAMPLE_DONE)
        .group_by(Sample.prompt_id)
        .having(func.count() >= 2)
        .subquery()
    )


async def _prompt_count(session: AsyncSession, run_ids: list[int]) -> int:
    if not run_ids:
        return 0
    eligible = _eligible_prompts(run_ids)
    return (await session.execute(select(func.count()).select_from(eligible))).scalar_one()


async def _decided_counts(session: AsyncSession, set_ids: list[int]) -> dict[int, int]:
    if not set_ids:
        return {}
    rows = await session.execute(
        select(PreferencePair.preference_set_id, func.count())
        .where(PreferencePair.preference_set_id.in_(set_ids))
        .group_by(PreferencePair.preference_set_id)
    )
    return {sid: n for sid, n in rows.all()}


async def _to_out(
    session: AsyncSession,
    ps: PreferenceSet,
    decided: int | None = None,
    prompt_set_name: str | None = None,
) -> PreferenceSetOut:
    runs = await _runs_of(session, ps)
    run_ids = [r.id for r in runs]
    if prompt_set_name is None:
        prompt_set = await session.get(PromptSet, ps.prompt_set_id)
        prompt_set_name = prompt_set.name if prompt_set else ""
    if decided is None:
        decided = (await _decided_counts(session, [ps.id])).get(ps.id, 0)
    return PreferenceSetOut(
        id=ps.id,
        name=ps.name,
        prompt_set_id=ps.prompt_set_id,
        prompt_set_name=prompt_set_name,
        run_ids=run_ids,
        runs=[{"id": r.id, "name": r.name, "model": r.model} for r in runs],
        prompt_count=await _prompt_count(session, run_ids),
        decided=decided,
        created_at=ps.created_at,
    )


async def _build_items(
    session: AsyncSession,
    runs: list[Run],
    prompt_rows: list,
) -> list[PreferenceItem]:
    """Attach the candidates to a page of prompts with one query for all their samples."""
    if not prompt_rows:
        return []
    run_by_id = {r.id: r for r in runs}
    run_order = {r.id: n for n, r in enumerate(runs)}
    prompt_ids = [row.id for row in prompt_rows]
    samples = (
        await session.execute(
            select(
                Sample.id,
                Sample.run_id,
                Sample.prompt_id,
                Sample.sample_idx,
                Sample.reasoning,
                Sample.response,
                Sample.finish_reason,
                Sample.completion_tokens,
                Sample.reasoning_tokens,
            ).where(
                Sample.prompt_id.in_(prompt_ids),
                Sample.run_id.in_(list(run_by_id)),
                Sample.status == SAMPLE_DONE,
            )
        )
    ).all()
    by_prompt: dict[int, list] = {pid: [] for pid in prompt_ids}
    for s in samples:
        by_prompt[s.prompt_id].append(s)

    items: list[PreferenceItem] = []
    for row in prompt_rows:
        cands = sorted(by_prompt[row.id], key=lambda s: (run_order[s.run_id], s.sample_idx))
        items.append(
            PreferenceItem(
                prompt_id=row.id,
                prompt_idx=row.idx,
                messages=row.messages or [],
                candidates=[
                    Candidate(
                        sample_id=s.id,
                        run_id=s.run_id,
                        run_name=run_by_id[s.run_id].name,
                        model=run_by_id[s.run_id].model,
                        sample_idx=s.sample_idx,
                        reasoning=s.reasoning,
                        response=s.response,
                        finish_reason=s.finish_reason,
                        completion_tokens=s.completion_tokens,
                        reasoning_tokens=s.reasoning_tokens,
                    )
                    for s in cands
                ],
                chosen_sample_id=row.chosen_sample_id,
                rejected_sample_id=row.rejected_sample_id,
                source=row.source,
            )
        )
    return items


def _item_query(set_id: int, run_ids: list[int]):
    eligible = _eligible_prompts(run_ids)
    return (
        select(
            Prompt.id,
            Prompt.idx,
            Prompt.messages,
            PreferencePair.chosen_sample_id,
            PreferencePair.rejected_sample_id,
            PreferencePair.source,
        )
        .join(eligible, eligible.c.prompt_id == Prompt.id)
        .outerjoin(
            PreferencePair,
            and_(PreferencePair.prompt_id == Prompt.id, PreferencePair.preference_set_id == set_id),
        )
    )


# --------------------------------------------------------------------------- sets


@router.get("/preference-sets", response_model=list[PreferenceSetOut])
async def list_preference_sets(session: AsyncSession = Depends(get_session)):
    sets = (
        await session.execute(
            select(PreferenceSet).order_by(PreferenceSet.created_at.desc(), PreferenceSet.id.desc())
        )
    ).scalars().all()
    decided = await _decided_counts(session, [s.id for s in sets])
    names = {
        pid: name
        for pid, name in (
            await session.execute(
                select(PromptSet.id, PromptSet.name).where(
                    PromptSet.id.in_({s.prompt_set_id for s in sets})
                )
            )
        ).all()
    }
    return [
        await _to_out(session, s, decided.get(s.id, 0), names.get(s.prompt_set_id, ""))
        for s in sets
    ]


@router.post("/preference-sets", response_model=PreferenceSetOut, status_code=201)
async def create_preference_set(body: PreferenceSetCreate, session: AsyncSession = Depends(get_session)):
    name = body.name.strip()
    if not name:
        raise RequestValidationError(
            [{"loc": ("body", "name"), "msg": "Enter a name for the preference set.", "type": "value_error"}]
        )
    run_ids = list(dict.fromkeys(body.run_ids))  # drop repeats, keep order
    runs = (await session.execute(select(Run).where(Run.id.in_(run_ids)))).scalars().all()
    if len(runs) != len(run_ids):
        raise HTTPException(422, "One of the chosen runs no longer exists. Refresh the list and choose again.")
    if len({r.prompt_set_id for r in runs}) != 1:
        raise HTTPException(422, "All runs must use the same prompt set.")
    ps = PreferenceSet(name=name, prompt_set_id=runs[0].prompt_set_id, run_ids=run_ids)
    session.add(ps)
    await session.commit()
    await session.refresh(ps)
    return await _to_out(session, ps, decided=0)


@router.get("/preference-sets/{set_id}", response_model=PreferenceSetOut)
async def get_preference_set(set_id: int, session: AsyncSession = Depends(get_session)):
    return await _to_out(session, await _get_set(session, set_id))


@router.delete("/preference-sets/{set_id}", status_code=204)
async def delete_preference_set(set_id: int, session: AsyncSession = Depends(get_session)):
    ps = await _get_set(session, set_id)
    # The foreign key cascades too; deleting explicitly keeps this correct without the pragma.
    await session.execute(delete(PreferencePair).where(PreferencePair.preference_set_id == ps.id))
    await session.delete(ps)
    await session.commit()


# --------------------------------------------------------------------------- items


@router.get("/preference-sets/{set_id}/items", response_model=Page[PreferenceItem])
async def list_items(
    set_id: int,
    offset: int = Query(0, ge=0),
    limit: int = Query(20, ge=1, le=200),
    filter: ItemFilter = Query("all"),
    session: AsyncSession = Depends(get_session),
):
    ps = await _get_set(session, set_id)
    runs = await _runs_of(session, ps)
    if not runs:
        return Page[PreferenceItem](items=[], total=0, offset=offset, limit=limit)
    query = _item_query(ps.id, [r.id for r in runs])
    if filter == "undecided":
        query = query.where(PreferencePair.id.is_(None))
    elif filter == "decided":
        query = query.where(PreferencePair.id.is_not(None))
    total = (await session.execute(select(func.count()).select_from(query.subquery()))).scalar_one()
    rows = (await session.execute(query.order_by(Prompt.idx, Prompt.id).offset(offset).limit(limit))).all()
    items = await _build_items(session, runs, rows)
    return Page[PreferenceItem](items=items, total=total, offset=offset, limit=limit)


async def _one_item(session: AsyncSession, ps: PreferenceSet, runs: list[Run], prompt_id: int) -> PreferenceItem:
    rows = (await session.execute(_item_query(ps.id, [r.id for r in runs]).where(Prompt.id == prompt_id))).all()
    if not rows:
        raise HTTPException(404, "This prompt has fewer than two finished responses in the set's runs.")
    return (await _build_items(session, runs, rows))[0]


# --------------------------------------------------------------------------- pairs


@router.put("/preference-sets/{set_id}/pairs/{prompt_id}", response_model=PreferenceItem)
async def put_pair(set_id: int, prompt_id: int, body: PairIn, session: AsyncSession = Depends(get_session)):
    ps = await _get_set(session, set_id)
    prompt = await session.get(Prompt, prompt_id)
    if prompt is None or prompt.prompt_set_id != ps.prompt_set_id:
        raise HTTPException(404, "Prompt not found in this preference set's prompt set.")
    if body.chosen_sample_id == body.rejected_sample_id:
        raise HTTPException(422, "Chosen and rejected must be two different responses.")
    runs = await _runs_of(session, ps)
    run_ids = {r.id for r in runs}
    samples = {
        s.id: s
        for s in (
            await session.execute(
                select(Sample).where(Sample.id.in_([body.chosen_sample_id, body.rejected_sample_id]))
            )
        ).scalars()
    }
    for label, sid in (("chosen", body.chosen_sample_id), ("rejected", body.rejected_sample_id)):
        s = samples.get(sid)
        if s is None:
            raise HTTPException(422, f"The {label} response does not exist. Reload the prompt and pick again.")
        if s.run_id not in run_ids:
            raise HTTPException(422, f"The {label} response comes from a run that is not in this preference set.")
        if s.prompt_id != prompt_id:
            raise HTTPException(422, f"The {label} response answers a different prompt. Reload and pick again.")
        if s.status != SAMPLE_DONE:
            raise HTTPException(422, f"The {label} response has not finished, so it cannot be paired.")

    pair = (
        await session.execute(
            select(PreferencePair).where(
                PreferencePair.preference_set_id == ps.id, PreferencePair.prompt_id == prompt_id
            )
        )
    ).scalar_one_or_none()
    if pair is None:
        session.add(
            PreferencePair(
                preference_set_id=ps.id,
                prompt_id=prompt_id,
                chosen_sample_id=body.chosen_sample_id,
                rejected_sample_id=body.rejected_sample_id,
                source="manual",
            )
        )
    else:
        pair.chosen_sample_id = body.chosen_sample_id
        pair.rejected_sample_id = body.rejected_sample_id
        pair.source = "manual"
        pair.updated_at = utcnow()
    await session.commit()
    return await _one_item(session, ps, runs, prompt_id)


@router.delete("/preference-sets/{set_id}/pairs/{prompt_id}", status_code=204)
async def delete_pair(set_id: int, prompt_id: int, session: AsyncSession = Depends(get_session)):
    ps = await _get_set(session, set_id)
    await session.execute(
        delete(PreferencePair).where(
            PreferencePair.preference_set_id == ps.id, PreferencePair.prompt_id == prompt_id
        )
    )
    await session.commit()


async def _first_done(session: AsyncSession, run_id: int) -> dict[int, int]:
    """prompt_id -> id of the run's first finished sample (lowest sample_idx) for that prompt."""
    rows = await session.execute(
        select(Sample.prompt_id, Sample.id)
        .where(Sample.run_id == run_id, Sample.status == SAMPLE_DONE)
        .order_by(Sample.prompt_id, Sample.sample_idx.desc())
    )
    # Later rows overwrite earlier ones, so the lowest sample_idx wins.
    return {pid: sid for pid, sid in rows.all()}


@router.post("/preference-sets/{set_id}/auto-pair", response_model=AutoPairOut)
async def auto_pair(set_id: int, body: AutoPairIn, session: AsyncSession = Depends(get_session)):
    ps = await _get_set(session, set_id)
    run_ids = {r.id for r in await _runs_of(session, ps)}
    if body.chosen_run_id == body.rejected_run_id:
        raise HTTPException(422, "Pick two different runs: one for chosen and one for rejected.")
    for label, rid in (("chosen", body.chosen_run_id), ("rejected", body.rejected_run_id)):
        if rid not in run_ids:
            raise HTTPException(422, f"The {label} run is not in this preference set.")

    chosen = await _first_done(session, body.chosen_run_id)
    rejected = await _first_done(session, body.rejected_run_id)
    existing = {
        pid: source
        for pid, source in (
            await session.execute(
                select(PreferencePair.prompt_id, PreferencePair.source).where(
                    PreferencePair.preference_set_id == ps.id
                )
            )
        ).all()
    }

    now = utcnow()
    rows: list[dict] = []
    skipped = 0
    for pid in sorted(chosen.keys() & rejected.keys()):
        if existing.get(pid) == "manual" and not body.overwrite:
            skipped += 1
            continue
        rows.append(
            {
                "preference_set_id": ps.id,
                "prompt_id": pid,
                "chosen_sample_id": chosen[pid],
                "rejected_sample_id": rejected[pid],
                "source": "rule",
                "created_at": now,
                "updated_at": now,
            }
        )

    # SQLite caps bound parameters per statement; 7 columns x 500 rows stays well under it.
    for start in range(0, len(rows), 500):
        stmt = sqlite_insert(PreferencePair).values(rows[start : start + 500])
        stmt = stmt.on_conflict_do_update(
            index_elements=["preference_set_id", "prompt_id"],
            set_={
                "chosen_sample_id": stmt.excluded.chosen_sample_id,
                "rejected_sample_id": stmt.excluded.rejected_sample_id,
                "source": stmt.excluded.source,
                "updated_at": stmt.excluded.updated_at,
            },
        )
        await session.execute(stmt)
    await session.commit()
    return AutoPairOut(created=len(rows), skipped=skipped)
