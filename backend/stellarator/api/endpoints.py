from __future__ import annotations

from datetime import datetime, timezone

import httpx
from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from ..db import get_session
from ..models import Endpoint, Run
from ..schemas import EndpointIn, EndpointOut, EndpointPatch, EndpointTestIn, EndpointTestOut
from ..services.openai_compat import check_key, describe_http_error, list_models
from ..services.settings import key_hint

router = APIRouter(prefix="/api/endpoints", tags=["endpoints"])


def to_out(ep: Endpoint, run_count: int = 0) -> EndpointOut:
    return EndpointOut(
        id=ep.id,
        name=ep.name,
        base_url=ep.base_url,
        has_api_key=bool(ep.api_key),
        api_key_hint=key_hint(ep.api_key),
        location=ep.location,
        default_concurrency=ep.default_concurrency,
        headers=ep.headers or {},
        models=ep.models or [],
        last_checked_at=ep.last_checked_at,
        last_ok=ep.last_ok,
        last_error=ep.last_error,
        created_at=ep.created_at,
        run_count=run_count,
    )


async def _get(session: AsyncSession, endpoint_id: int) -> Endpoint:
    ep = await session.get(Endpoint, endpoint_id)
    if ep is None:
        raise HTTPException(404, "Endpoint not found")
    return ep


async def _run_counts(session: AsyncSession) -> dict[int, int]:
    rows = await session.execute(
        select(Run.endpoint_id, func.count()).where(Run.endpoint_id.is_not(None)).group_by(Run.endpoint_id)
    )
    return {eid: n for eid, n in rows.all()}


async def _check(ep: Endpoint) -> None:
    try:
        models, _ = await list_models(ep.base_url, ep.api_key, ep.headers)
        ep.models = models
        key_error = await check_key(ep.base_url, ep.api_key, ep.headers)
        ep.last_ok = key_error is None
        ep.last_error = key_error
    except (httpx.HTTPError, ValueError) as exc:
        ep.last_ok = False
        ep.last_error = describe_http_error(exc)
    ep.last_checked_at = datetime.now(timezone.utc)


@router.get("", response_model=list[EndpointOut])
async def list_endpoints(session: AsyncSession = Depends(get_session)):
    eps = (await session.execute(select(Endpoint).order_by(Endpoint.created_at.desc()))).scalars().all()
    counts = await _run_counts(session)
    return [to_out(ep, counts.get(ep.id, 0)) for ep in eps]


@router.post("", response_model=EndpointOut, status_code=201)
async def create_endpoint(body: EndpointIn, session: AsyncSession = Depends(get_session)):
    ep = Endpoint(**body.model_dump())
    ep.api_key = body.api_key or None
    await _check(ep)
    session.add(ep)
    await session.commit()
    return to_out(ep)


@router.post("/test", response_model=EndpointTestOut)
async def test_endpoint(body: EndpointTestIn, session: AsyncSession = Depends(get_session)):
    api_key = body.api_key
    if not api_key and body.endpoint_id is not None:
        saved = await session.get(Endpoint, body.endpoint_id)
        api_key = saved.api_key if saved else None
    try:
        models, latency = await list_models(body.base_url, api_key, body.headers)
        key_error = await check_key(body.base_url, api_key, body.headers)
        return EndpointTestOut(ok=key_error is None, models=models, latency_ms=latency, error=key_error)
    except (httpx.HTTPError, ValueError) as exc:
        return EndpointTestOut(ok=False, models=[], latency_ms=None, error=describe_http_error(exc))


@router.get("/{endpoint_id}", response_model=EndpointOut)
async def get_endpoint(endpoint_id: int, session: AsyncSession = Depends(get_session)):
    ep = await _get(session, endpoint_id)
    return to_out(ep, (await _run_counts(session)).get(ep.id, 0))


@router.patch("/{endpoint_id}", response_model=EndpointOut)
async def update_endpoint(
    endpoint_id: int, body: EndpointPatch, session: AsyncSession = Depends(get_session)
):
    ep = await _get(session, endpoint_id)
    data = body.model_dump(exclude_unset=True)
    if "api_key" in data:
        key = data.pop("api_key")
        if key is not None:
            ep.api_key = key or None
    for field, value in data.items():
        if value is not None:
            setattr(ep, field, value)
    await _check(ep)
    await session.commit()
    return to_out(ep, (await _run_counts(session)).get(ep.id, 0))


@router.post("/{endpoint_id}/check", response_model=EndpointOut)
async def check_endpoint(endpoint_id: int, session: AsyncSession = Depends(get_session)):
    ep = await _get(session, endpoint_id)
    await _check(ep)
    await session.commit()
    return to_out(ep, (await _run_counts(session)).get(ep.id, 0))


@router.delete("/{endpoint_id}", status_code=204)
async def delete_endpoint(endpoint_id: int, session: AsyncSession = Depends(get_session)):
    from ..services.engine import engine_registry

    ep = await _get(session, endpoint_id)
    active = (
        await session.execute(
            select(func.count()).select_from(Run).where(
                Run.endpoint_id == endpoint_id, Run.status.in_(("running", "pausing"))
            )
        )
    ).scalar_one()
    if active or any(r.endpoint_id == endpoint_id for r in engine_registry.active_runs()):
        raise HTTPException(409, "A run is using this endpoint. Pause it before deleting the endpoint.")
    await session.delete(ep)
    await session.commit()
