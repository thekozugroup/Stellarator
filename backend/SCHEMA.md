# Stellarator Canonical Schema

The Python backend (FastAPI + SQLAlchemy + Alembic) is the **sole owner** of
the SQLite schema. The Rust supervisor MUST connect to this database in
**read-and-update-rows-only** mode and MUST NOT execute any
`CREATE TABLE`, `CREATE INDEX`, `ALTER`, or `DROP` statements. Any drift
between the supervisor's expectations and the canonical schema is a bug in
the supervisor.

## Source of truth

- Models: `backend/app/models/{run.py,budget.py,chat.py}`
- Migrations: `backend/alembic/versions/*.py`
- Bootstrap: `docker/backend-entrypoint.sh` runs `alembic upgrade head`
  before launching uvicorn.

## Tables

### `runs`
Primary key `id` (text, 40). Indexed columns: `owner_agent`, `status`,
`tinker_job_id`, plus composite `(owner_agent, status)` for the dashboard
filter. Cost-tracking columns (`gpu_seconds`, `cost_usd`) are written by
both backend and supervisor.

### `run_notes`
FK `run_id -> runs.id ON DELETE CASCADE`. Indexed by `run_id`.
Append-only audit log of agent decisions.

### `run_metrics`
FK `run_id -> runs.id ON DELETE CASCADE`. Indexed by `run_id`.
**Unique constraint** `(run_id, step, name)` — the supervisor MUST treat
duplicate metric inserts as conflicts and either upsert or drop them; the
DB will reject duplicates with `IntegrityError`.

### `budgets`
Per-agent and per-run spend caps. Read by the cost service before each
run create.

### `chat_sessions`, `chat_messages`
Indexed `chat_messages(session_id, created_at)` for time-ordered fetches.

### `codex_tokens`
Per-agent OAuth credentials. Backend-only.

## SQLite pragmas

The backend installs WAL + busy_timeout=10000 on every connection
(`app/core/sqlite_pragmas.py`). The supervisor MUST set the same
`PRAGMA busy_timeout` to avoid `SQLITE_BUSY` under contention.

## Migration policy

- Add a new Alembic revision for any schema change.
- Never edit a shipped migration; add a new one.
- Index changes count as schema changes.
- The supervisor does not need to know the migration version, but it
  MUST be tolerant to new nullable columns (use explicit column lists in
  `INSERT`/`SELECT`).
