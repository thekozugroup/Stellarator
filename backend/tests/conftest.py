"""Shared pytest fixtures for Stellarator tests."""

from __future__ import annotations

import pytest
import pytest_asyncio
from httpx import ASGITransport, AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine

from app.core.db import Base, get_session
from app.main import app

TEST_DB_URL = "sqlite+aiosqlite:///:memory:"


@pytest_asyncio.fixture(scope="function")
async def session():
    engine = create_async_engine(TEST_DB_URL, future=True)
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)

    session_factory = async_sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)
    async with session_factory() as s:
        yield s

    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.drop_all)
    await engine.dispose()


@pytest_asyncio.fixture(scope="function")
async def client(session: AsyncSession):
    """AsyncClient with DB overridden to in-memory SQLite."""

    async def _override_session():
        yield session

    app.dependency_overrides[get_session] = _override_session
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as ac:
        yield ac
    app.dependency_overrides.clear()


# Agent tokens configured in settings; we monkey-patch settings for tests.
CLAUDE_CODE_TOKEN = "test-token-claude-code"
OPENAI_TOKEN = "test-token-openai"


@pytest.fixture(autouse=True)
def patch_settings(monkeypatch):
    from app.core import config

    monkeypatch.setattr(config.settings, "agent_token_claude_code", CLAUDE_CODE_TOKEN)
    monkeypatch.setattr(config.settings, "agent_token_openai", OPENAI_TOKEN)
    # Disable Tinker by patching create_job to return a fake id
    import app.services.tinker as tinker_mod

    async def fake_create_job(**kw):
        return {"id": "fake-tinker-job-id"}

    async def fake_cancel_job(job_id):
        return {}

    async def fake_pause_job(job_id):
        return {}

    async def fake_resume_job(job_id):
        return {}

    monkeypatch.setattr(tinker_mod.tinker, "create_job", fake_create_job)
    monkeypatch.setattr(tinker_mod.tinker, "cancel_job", fake_cancel_job)
    monkeypatch.setattr(tinker_mod.tinker, "pause_job", fake_pause_job)
    monkeypatch.setattr(tinker_mod.tinker, "resume_job", fake_resume_job)
