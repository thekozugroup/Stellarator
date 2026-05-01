"""Alembic environment for Stellarator (sync engine; offline + online)."""

from __future__ import annotations

import os
from logging.config import fileConfig

from alembic import context
from sqlalchemy import engine_from_config, pool

from app.core.db import Base
# Import models so metadata is populated before autogenerate.
from app.models import budget as _budget  # noqa: F401
from app.models import chat as _chat  # noqa: F401
from app.models import run as _run  # noqa: F401
from app.models import oauth_state as _oauth_state  # noqa: F401

config = context.config

if config.config_file_name is not None:
    fileConfig(config.config_file_name)

target_metadata = Base.metadata


def _resolve_url() -> str:
    # Prefer env var, then alembic.ini, then sane SQLite default.
    raw = os.getenv("STELLARATOR_DB_URL") or config.get_main_option("sqlalchemy.url") or ""
    # Alembic uses sync drivers; strip async driver suffixes.
    return (
        raw.replace("+aiosqlite", "")
           .replace("+asyncpg", "+psycopg")
           .replace("+asyncmy", "+pymysql")
    )


def run_migrations_offline() -> None:
    context.configure(
        url=_resolve_url(),
        target_metadata=target_metadata,
        literal_binds=True,
        dialect_opts={"paramstyle": "named"},
        render_as_batch=True,
    )
    with context.begin_transaction():
        context.run_migrations()


def run_migrations_online() -> None:
    section = config.get_section(config.config_ini_section) or {}
    section["sqlalchemy.url"] = _resolve_url()
    connectable = engine_from_config(
        section, prefix="sqlalchemy.", poolclass=pool.NullPool
    )
    with connectable.connect() as connection:
        context.configure(
            connection=connection,
            target_metadata=target_metadata,
            render_as_batch=True,
        )
        with context.begin_transaction():
            context.run_migrations()


if context.is_offline_mode():
    run_migrations_offline()
else:
    run_migrations_online()
