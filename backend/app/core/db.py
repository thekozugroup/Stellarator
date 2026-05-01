from __future__ import annotations

import logging
from pathlib import Path

from sqlalchemy import inspect, text
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine
from sqlalchemy.orm import DeclarativeBase

from .config import settings
from .sqlite_pragmas import install_sqlite_pragmas

# Lazy import guard — only needed when stamping.
try:
    from sqlalchemy import create_engine as _create_engine
except ImportError:  # pragma: no cover
    _create_engine = None  # type: ignore[assignment]

# Resolve alembic.ini relative to this file so the path is correct regardless
# of the working directory at process start.
_ALEMBIC_INI = Path(__file__).resolve().parent.parent.parent / "alembic.ini"

logger = logging.getLogger(__name__)


class Base(DeclarativeBase):
    pass


engine = create_async_engine(settings.stellarator_db_url, future=True)

# Install WAL/busy_timeout/FK pragmas at module load. No-op for non-sqlite.
install_sqlite_pragmas(engine)

SessionLocal = async_sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)


async def init_db() -> None:
    """Initialize the schema.

    If alembic_version table exists, treat the DB as alembic-managed and
    no-op (migrations are run by entrypoint). Otherwise create_all and
    stamp head so future migrations apply cleanly.
    """
    # Register tables.
    from app.models import budget as _budget  # noqa: F401
    from app.models import chat as _chat  # noqa: F401
    from app.models import integration as _integration  # noqa: F401
    from app.models import oauth as _oauth  # noqa: F401
    from app.models import run as _run  # noqa: F401

    async with engine.begin() as conn:
        has_alembic = await conn.run_sync(
            lambda sync_conn: inspect(sync_conn).has_table("alembic_version")
        )
        if has_alembic:
            logger.info("alembic_version table present; skipping create_all")
            return
        await conn.run_sync(Base.metadata.create_all)

    # Best-effort stamp so subsequent `alembic upgrade head` is a no-op.
    # IMPORTANT: command.stamp() must run OUTSIDE the async engine.begin()
    # transaction because Alembic opens its own connection internally.
    try:
        from alembic import command
        from alembic.config import Config
        from sqlalchemy import create_engine as _sync_create_engine

        cfg = Config(str(_ALEMBIC_INI))
        # Derive a sync URL from the async one (strip +aiosqlite / +asyncpg etc.)
        sync_url = str(engine.url).replace("+aiosqlite", "").replace("+asyncpg", "")
        sync_engine = _sync_create_engine(sync_url)
        with sync_engine.connect() as _sc:
            command.stamp(cfg, "head")
        sync_engine.dispose()
        logger.info("Stamped alembic head after create_all")
    except Exception:  # noqa: BLE001
        # Alembic may not be configured in dev/test contexts; tolerate.
        logger.warning("Alembic stamp skipped (config unavailable at %s)", _ALEMBIC_INI, exc_info=True)


async def get_session() -> AsyncSession:
    async with SessionLocal() as session:
        yield session


__all__ = ["Base", "engine", "SessionLocal", "init_db", "get_session", "text"]
