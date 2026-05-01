"""SQLite PRAGMA configuration for the async engine.

Registers a connect-time event listener that sets WAL journal mode,
NORMAL synchronous, a 10s busy_timeout, and enables FK enforcement on
every aiosqlite connection.

Only applied to SQLite engines; a no-op for other backends.
"""

from __future__ import annotations

import logging

from sqlalchemy import event
from sqlalchemy.engine import Engine
from sqlalchemy.ext.asyncio import AsyncEngine

logger = logging.getLogger(__name__)

_PRAGMAS: tuple[tuple[str, str], ...] = (
    ("journal_mode", "WAL"),
    ("synchronous", "NORMAL"),
    ("busy_timeout", "10000"),
    ("foreign_keys", "ON"),
)


def _is_sqlite(engine: AsyncEngine | Engine) -> bool:
    try:
        return engine.url.get_backend_name() == "sqlite"
    except Exception:  # pragma: no cover - defensive
        return False


def install_sqlite_pragmas(engine: AsyncEngine | Engine) -> None:
    """Attach a 'connect' listener that applies PRAGMAs to every connection.

    Safe to call repeatedly; SQLAlchemy will register duplicate listeners
    only if `once=False` (default), so callers should invoke at module-load
    time exactly once.
    """
    if not _is_sqlite(engine):
        logger.debug("Skipping SQLite pragma install: backend is not sqlite")
        return

    sync_engine = engine.sync_engine if isinstance(engine, AsyncEngine) else engine

    @event.listens_for(sync_engine, "connect")
    def _set_sqlite_pragmas(dbapi_connection, _connection_record) -> None:
        cursor = dbapi_connection.cursor()
        try:
            for name, value in _PRAGMAS:
                cursor.execute(f"PRAGMA {name}={value};")
        except Exception:
            logger.exception("Failed to set SQLite pragmas")
            raise
        finally:
            cursor.close()
