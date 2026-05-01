"""Tests for POST /v1/runs/{sandbox_id}/promote."""

from __future__ import annotations

import pytest
import pytest_asyncio
from httpx import ASGITransport, AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession

from app.main import app
from app.models.run import Run, RunStatus
from app.core.db import get_session

from tests.conftest import CLAUDE_CODE_TOKEN, OPENAI_TOKEN

AUTH = {"Authorization": f"Bearer {CLAUDE_CODE_TOKEN}"}
OTHER_AUTH = {"Authorization": f"Bearer {OPENAI_TOKEN}"}

_PROMOTE_BODY = {
    "name": "scale-run-1",
    "gpu_type": "H100",
    "gpu_count": 4,
    "hyperparams_overrides": {"lr": 3e-5},
    "user_goal": "scale up",
}


async def _insert_run(session: AsyncSession, **kwargs) -> Run:
    defaults = dict(
        id="sandbox001",
        owner_agent="claude-code",
        name="my sandbox",
        base_model="llama-3-8b",
        method="sft",
        hyperparams={"lr": 1e-4, "epochs": 1},
        dataset_mixture=[{"name": "ds1", "weight": 1.0, "source": "hf"}],
        gpu_type="H100",
        gpu_count=1,
        is_sandbox=True,
        status=RunStatus.succeeded.value,
        citations=[{"source": "arxiv", "id": "2301.00001", "title": "Some Paper", "note": ""}],
    )
    defaults.update(kwargs)
    run = Run(**defaults)
    session.add(run)
    await session.commit()
    await session.refresh(run)
    return run


@pytest.mark.asyncio
async def test_promote_success(session: AsyncSession, patch_settings, monkeypatch):
    """Happy path: succeeded sandbox → new production run returned."""
    import app.services.tinker as tinker_mod

    async def fake_create_job(**kw):
        return {"id": "fake-tinker-promo"}

    monkeypatch.setattr(tinker_mod.tinker, "create_job", fake_create_job)

    # Stub supervisor handoff
    import app.api.runs as runs_mod

    async def fake_handoff(run_id, tinker_job_id):
        return None

    monkeypatch.setattr(runs_mod, "_hand_off_to_supervisor", fake_handoff)

    await _insert_run(session)

    async def _override():
        yield session

    app.dependency_overrides[get_session] = _override
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as ac:
        r = await ac.post("/v1/runs/sandbox001/promote", json=_PROMOTE_BODY, headers=AUTH)
    app.dependency_overrides.clear()

    assert r.status_code == 201, r.text
    data = r.json()
    assert data["is_sandbox"] is False
    assert data["parent_run_id"] == "sandbox001"
    assert data["gpu_count"] == 4
    assert data["hyperparams"]["lr"] == pytest.approx(3e-5)


@pytest.mark.asyncio
async def test_promote_carries_citations(session: AsyncSession, patch_settings, monkeypatch):
    """Promoted run inherits citations from sandbox."""
    import app.services.tinker as tinker_mod
    import app.api.runs as runs_mod

    monkeypatch.setattr(tinker_mod.tinker, "create_job", lambda **kw: {"id": "tj"})
    monkeypatch.setattr(runs_mod, "_hand_off_to_supervisor", lambda *a: None)

    await _insert_run(session)

    async def _override():
        yield session

    app.dependency_overrides[get_session] = _override
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as ac:
        r = await ac.post("/v1/runs/sandbox001/promote", json=_PROMOTE_BODY, headers=AUTH)
    app.dependency_overrides.clear()

    assert r.status_code == 201
    data = r.json()
    assert data["citations"][0]["source"] == "arxiv"


@pytest.mark.asyncio
async def test_promote_rejects_non_sandbox(session: AsyncSession, patch_settings):
    """Promoting a non-sandbox run must return 422."""
    await _insert_run(session, is_sandbox=False)

    async def _override():
        yield session

    app.dependency_overrides[get_session] = _override
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as ac:
        r = await ac.post("/v1/runs/sandbox001/promote", json=_PROMOTE_BODY, headers=AUTH)
    app.dependency_overrides.clear()

    assert r.status_code == 422
    assert r.json()["detail"]["error"] == "not_a_sandbox"


@pytest.mark.asyncio
async def test_promote_rejects_unfinished_sandbox(session: AsyncSession, patch_settings):
    """Sandbox that hasn't succeeded must return 409."""
    await _insert_run(session, status=RunStatus.running.value)

    async def _override():
        yield session

    app.dependency_overrides[get_session] = _override
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as ac:
        r = await ac.post("/v1/runs/sandbox001/promote", json=_PROMOTE_BODY, headers=AUTH)
    app.dependency_overrides.clear()

    assert r.status_code == 409
    assert r.json()["detail"]["error"] == "sandbox_not_succeeded"


@pytest.mark.asyncio
async def test_promote_rejects_non_owner(session: AsyncSession, patch_settings):
    """A different agent must not be able to promote another agent's sandbox."""
    await _insert_run(session)  # owned by claude-code

    async def _override():
        yield session

    app.dependency_overrides[get_session] = _override
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as ac:
        r = await ac.post("/v1/runs/sandbox001/promote", json=_PROMOTE_BODY, headers=OTHER_AUTH)
    app.dependency_overrides.clear()

    assert r.status_code == 403
