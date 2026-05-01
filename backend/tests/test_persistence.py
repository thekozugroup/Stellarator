"""Persistence-layer tests: pragmas, concurrency, budget pre-check,
unique-index enforcement, and reconciliation re-tracking.
"""

from __future__ import annotations

import asyncio
import os
from typing import AsyncIterator
from unittest.mock import AsyncMock, patch

import pytest
import pytest_asyncio
from sqlalchemy import text
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine

from app.core.db import Base
from app.core.sqlite_pragmas import install_sqlite_pragmas
from app.models.budget import Budget
from app.models.run import Run, RunMetric, RunStatus
from app.services import cost as cost_service
from app.services import reconcile as reconcile_mod


# ---------------------------------------------------------------------------
# Shared file-backed SQLite engine so multiple sessions hit the same DB
# (the ":memory:" db isn't shared across connections).
# ---------------------------------------------------------------------------


@pytest_asyncio.fixture
async def file_engine(tmp_path) -> AsyncIterator:
    db_path = tmp_path / "persist.db"
    url = f"sqlite+aiosqlite:///{db_path}"
    engine = create_async_engine(url, future=True)
    install_sqlite_pragmas(engine)
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)
    try:
        yield engine
    finally:
        await engine.dispose()


@pytest_asyncio.fixture
async def session_factory(file_engine):
    return async_sessionmaker(file_engine, class_=AsyncSession, expire_on_commit=False)


# ---------------------------------------------------------------------------
# 1. WAL mode is set after connect
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_wal_mode_is_enabled(file_engine):
    async with file_engine.connect() as conn:
        result = await conn.execute(text("PRAGMA journal_mode"))
        mode = result.scalar()
    assert isinstance(mode, str) and mode.lower() == "wal"


# ---------------------------------------------------------------------------
# 2. busy_timeout is 10000
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_busy_timeout_is_set(file_engine):
    async with file_engine.connect() as conn:
        result = await conn.execute(text("PRAGMA busy_timeout"))
        timeout = result.scalar()
    assert timeout == 10000


# ---------------------------------------------------------------------------
# 3. Concurrent inserts from two sessions don't deadlock
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_concurrent_inserts_do_not_deadlock(session_factory):
    async def _insert(idx: int) -> None:
        async with session_factory() as s:
            run = Run(
                id=f"run-{idx:03d}",
                owner_agent="agent-x",
                name=f"r{idx}",
                base_model="m",
                method="sft",
                hyperparams={},
                dataset_mixture=[],
                citations=[],
                status=RunStatus.queued.value,
            )
            s.add(run)
            await s.commit()

    await asyncio.wait_for(
        asyncio.gather(*[_insert(i) for i in range(8)]),
        timeout=15.0,
    )

    async with session_factory() as s:
        result = await s.execute(text("SELECT COUNT(*) FROM runs"))
        assert result.scalar() == 8


# ---------------------------------------------------------------------------
# 4. Budget pre-check returns 402 when projected exceeds monthly_limit
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_budget_precheck_blocks_when_exceeded(session_factory):
    async with session_factory() as s:
        s.add(
            Budget(
                scope="agent",
                scope_id="poor-agent",
                monthly_limit_usd=1.00,
                alert_threshold_pct=80.0,
            )
        )
        await s.commit()

    # 1 H100 * default price * 100h = a lot more than $1
    payload = {
        "gpu_type": "H100",
        "gpu_count": 1,
        "hyperparams": {"max_steps": 360_000, "estimated_seconds_per_step": 1.0},
    }
    projected = cost_service.projected_total_for(payload)
    assert projected > 1.00

    async with session_factory() as s:
        ok, info = await cost_service.check_budget(s, "poor-agent", projected)
    assert ok is False
    assert info is not None
    assert info["budget"] == pytest.approx(1.00)
    assert info["projected"] >= projected


@pytest.mark.asyncio
async def test_budget_precheck_passes_under_limit(session_factory):
    async with session_factory() as s:
        s.add(
            Budget(
                scope="agent",
                scope_id="rich-agent",
                monthly_limit_usd=1_000_000.0,
            )
        )
        await s.commit()

    payload = {
        "gpu_type": "H100",
        "gpu_count": 1,
        "hyperparams": {"max_steps": 100, "estimated_seconds_per_step": 1.0},
    }
    projected = cost_service.projected_total_for(payload)

    async with session_factory() as s:
        ok, info = await cost_service.check_budget(s, "rich-agent", projected)
    assert ok is True
    assert info is None


# ---------------------------------------------------------------------------
# 5. Reconciliation loop re-tracks an orphan running job
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_reconcile_retracks_orphan_running_job(session_factory, monkeypatch):
    # Seed a running run that the supervisor doesn't know about.
    async with session_factory() as s:
        s.add(
            Run(
                id="orphan-1",
                owner_agent="a",
                name="r",
                base_model="m",
                method="sft",
                hyperparams={},
                dataset_mixture=[],
                citations=[],
                status=RunStatus.running.value,
                tinker_job_id="tj-orphan",
            )
        )
        await s.commit()

    # Point reconcile at our test DB.
    monkeypatch.setattr(reconcile_mod, "SessionLocal", session_factory)

    # Mock tinker.get_job to keep status as "running".
    fake_get_job = AsyncMock(return_value={"status": "running"})
    monkeypatch.setattr(reconcile_mod.tinker, "get_job", fake_get_job)

    track_calls: list[dict] = []
    status_calls: list[None] = []

    class _FakeResp:
        def __init__(self, payload):
            self._payload = payload

        def raise_for_status(self):
            return None

        def json(self):
            return self._payload

    class _FakeClient:
        def __init__(self, *_a, **_kw):
            pass

        async def __aenter__(self):
            return self

        async def __aexit__(self, *exc):
            return False

        async def post(self, url, json=None, headers=None):
            if url.endswith("/supervisor/status"):
                status_calls.append(None)
                return _FakeResp({"runs": []})
            if url.endswith("/supervisor/track"):
                track_calls.append(json or {})
                return _FakeResp({"ok": True})
            raise AssertionError(f"unexpected url {url}")

    monkeypatch.setattr(reconcile_mod.httpx, "AsyncClient", _FakeClient)

    await reconcile_mod._reconcile_once()

    assert fake_get_job.await_count == 1
    assert len(status_calls) == 1
    assert len(track_calls) == 1
    assert track_calls[0]["run_id"] == "orphan-1"
    assert track_calls[0]["tinker_job_id"] == "tj-orphan"


# ---------------------------------------------------------------------------
# 6. Unique index prevents duplicate (run_id, step, name) — enforced via
#    Alembic migration; in tests we install it explicitly so the in-test
#    create_all matches production behavior.
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_unique_index_blocks_duplicate_metric(file_engine, session_factory):
    async with file_engine.begin() as conn:
        await conn.execute(
            text(
                "CREATE UNIQUE INDEX IF NOT EXISTS uq_run_metrics_run_step_name "
                "ON run_metrics (run_id, step, name)"
            )
        )

    async with session_factory() as s:
        s.add(
            Run(
                id="run-uq",
                owner_agent="a",
                name="r",
                base_model="m",
                method="sft",
                hyperparams={},
                dataset_mixture=[],
                citations=[],
                status=RunStatus.running.value,
            )
        )
        s.add(RunMetric(run_id="run-uq", step=1, name="loss", value=0.5))
        await s.commit()

    async with session_factory() as s:
        s.add(RunMetric(run_id="run-uq", step=1, name="loss", value=0.6))
        with pytest.raises(IntegrityError):
            await s.commit()
