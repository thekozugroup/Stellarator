#!/bin/sh
# Entrypoint: run alembic migrations to head, then exec the API server.
set -eu

cd /app

# Ensure the data dir exists for SQLite.
mkdir -p /data

echo "[entrypoint] Running alembic upgrade head"
alembic upgrade head

echo "[entrypoint] Starting uvicorn"
exec uvicorn app.main:app --host 0.0.0.0 --port 8000 "$@"
