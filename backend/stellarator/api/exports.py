"""Dataset exports: preview, download and Hugging Face push for runs and preference sets."""

from __future__ import annotations

from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.responses import StreamingResponse
from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from ..db import get_session
from ..models import PreferenceSet, PromptSet, Run
from ..schemas import (
    ExportOptions,
    ExportPreviewOut,
    HubPushIn,
    HubPushOut,
    PrefExportOptions,
)
from ..services import exporters as ex
from ..services.settings import hf_token

router = APIRouter(prefix="/api", tags=["exports"])


class RunHubIn(BaseModel):
    options: ExportOptions = Field(default_factory=ExportOptions)
    target: HubPushIn


class PrefHubIn(BaseModel):
    options: PrefExportOptions = Field(default_factory=PrefExportOptions)
    target: HubPushIn


async def _run(session: AsyncSession, run_id: int) -> Run:
    run = await session.get(Run, run_id)
    if run is None:
        raise HTTPException(404, "Run not found")
    return run


async def _pset(session: AsyncSession, set_id: int) -> PreferenceSet:
    ps = await session.get(PreferenceSet, set_id)
    if ps is None:
        raise HTTPException(404, "Preference set not found")
    return ps


def _download(filename: str, file_type: str, chunks) -> StreamingResponse:
    return StreamingResponse(
        chunks,
        media_type=ex.MEDIA_TYPES[file_type],
        headers={"Content-Disposition": f'attachment; filename="{filename}"'},
    )


# --------------------------------------------------------------------------- runs


@router.post("/runs/{run_id}/export/preview", response_model=ExportPreviewOut)
async def preview_run_export(run_id: int, options: ExportOptions, session: AsyncSession = Depends(get_session)):
    run = await _run(session, run_id)
    return await ex.sft_preview(session, run, options)


@router.get("/runs/{run_id}/export")
async def download_run_export(
    run_id: int,
    options: Annotated[ExportOptions, Query()],
    session: AsyncSession = Depends(get_session),
):
    run = await _run(session, run_id)
    filename = ex.filename_for(run.name, options.format, options.file_type)
    chunks = ex.stream_file(options.file_type, ex.iter_sft_records(run, options), ex.sft_schema(options))
    return _download(filename, options.file_type, chunks)


@router.post("/runs/{run_id}/export/hub", response_model=HubPushOut)
async def push_run_export(run_id: int, body: RunHubIn, session: AsyncSession = Depends(get_session)):
    run = await _run(session, run_id)
    token = await hf_token()
    if not token:
        raise HTTPException(400, ex.HF_TOKEN_MISSING)
    options = body.options
    count, _ = await ex.sft_counts(session, run.id, options)
    set_name = (await session.get(PromptSet, run.prompt_set_id)).name
    return await ex.push_to_hub(
        token=token,
        target=body.target,
        records=ex.iter_sft_records(run, options),
        schema=ex.sft_schema(options),
        count=count,
        card=ex.run_card(run, set_name, count, options, body.target.split),
    )


# --------------------------------------------------------------------------- preference sets


@router.post("/preference-sets/{set_id}/export/preview", response_model=ExportPreviewOut)
async def preview_pref_export(set_id: int, options: PrefExportOptions, session: AsyncSession = Depends(get_session)):
    ps = await _pset(session, set_id)
    return await ex.pref_preview(session, ps.name, ps.id, options)


@router.get("/preference-sets/{set_id}/export")
async def download_pref_export(
    set_id: int,
    options: Annotated[PrefExportOptions, Query()],
    session: AsyncSession = Depends(get_session),
):
    ps = await _pset(session, set_id)
    filename = ex.filename_for(ps.name, options.format, options.file_type)
    chunks = ex.stream_file(options.file_type, ex.iter_pref_records(ps.id, options), ex.pref_schema(options))
    return _download(filename, options.file_type, chunks)


@router.post("/preference-sets/{set_id}/export/hub", response_model=HubPushOut)
async def push_pref_export(set_id: int, body: PrefHubIn, session: AsyncSession = Depends(get_session)):
    ps = await _pset(session, set_id)
    token = await hf_token()
    if not token:
        raise HTTPException(400, ex.HF_TOKEN_MISSING)
    options = body.options
    count = await ex.pref_count(session, ps.id, options)
    set_name = (await session.get(PromptSet, ps.prompt_set_id)).name
    runs = (await session.execute(select(Run).where(Run.id.in_(ps.run_ids or [])))).scalars().all()
    return await ex.push_to_hub(
        token=token,
        target=body.target,
        records=ex.iter_pref_records(ps.id, options),
        schema=ex.pref_schema(options),
        count=count,
        card=ex.pref_card(ps.name, set_name, list(runs), count, options, body.target.split),
    )
