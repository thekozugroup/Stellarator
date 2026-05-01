# Stellarator — known issues / TODO

## Bring-up bugs (already fixed locally on Hephaestus, pending upstream commit)

1. **`docker/frontend.Dockerfile`**: `npm install` needs `--legacy-peer-deps` (React 19 ecosystem has a stale peer-range in `@tanstack/react-virtual@3.10.8` excluding React 19).
2. **`docker/supervisor.Dockerfile`**: `rust:1.82-bookworm` was too old; `Cargo.toml` uses `edition2024` which needs Rust ≥1.85. Bumped to `rust:1-bookworm` to track latest stable.
3. **`backend/alembic.ini`**: `script_location = backend/alembic` was wrong relative to `cd /app` in entrypoint; corrected to `script_location = alembic`.
4. **`docker/backend-entrypoint.sh`**: `alembic upgrade head` failed on multi-head migration tree; switched to `alembic upgrade heads`.
5. **`docker-compose.yml`**:
   - removed obsolete `version: '3.8'`
   - flipped supervisor↔backend `depends_on` (supervisor now waits on backend healthcheck, since backend creates the DB via alembic)
   - supervisor needs override for `STELLARATOR_DB_URL` because sqlx can't parse `+aiosqlite` driver suffix; set explicit `sqlite:////data/stellarator.db?mode=rwc` for supervisor
   - healthchecks switched from `curl` to `python3` / bash `/dev/tcp` (curl wasn't installed in either image)
6. **`.env.example`**: documented `SUPERVISOR_SHARED_SECRET` (was required by supervisor but undocumented).

## Frontend "Failed to fetch" when accessing UI from non-localhost host

**Fixed locally**: frontend used `NEXT_PUBLIC_API_URL=http://localhost:8000` as default, which resolves to the *browser's* localhost (the user's laptop), not the server. Any access from LAN / Tailscale / hostname broke.

Permanent fix applied:
- `frontend/next.config.ts`: added Next.js rewrites `/v1/* → BACKEND_INTERNAL_URL/v1/*` (default `http://backend:8000`).
- All frontend files using `process.env.NEXT_PUBLIC_API_URL || "http://localhost:8000"` changed to default to empty string (same-origin).
- `.env`: `NEXT_PUBLIC_API_URL=` (empty) → triggers same-origin URLs.
- Browser now only ever calls same origin; works from localhost / LAN / Tailscale identically.

## Codex OAuth fails 503 "Codex OAuth not configured"

`/v1/oauth/codex/start` requires `CODEX_OAUTH_CLIENT_ID` + `CODEX_OAUTH_CLIENT_SECRET` env vars. OpenAI does not currently publicly issue OAuth client credentials for `auth.openai.com/oauth/authorize` to arbitrary apps.

**However**, the `opencode` tool reportedly got Codex OAuth working — investigate how. Likely paths:
- Different OAuth surface (e.g., chatgpt.com flow, or Codex CLI's own token-passing)
- Pre-issued tokens via partner program
- Device-code flow

Until investigated, document the limitation in `docs/bootstrap.md` and provide an OpenAI-API-key fallback path so users without Codex OAuth credentials can still use the Chat tab via plain OpenAI API.

**Action**: study opencode's Codex OAuth implementation; port the working flow into `backend/app/agents/oauth_codex.py`.

## docs/bootstrap.md API endpoints don't match implementation

`docs/bootstrap.md` documents:
- `POST /v1/research` — does not exist (404)
- `POST /v1/sandbox_create` — does not exist
- `POST /v1/submit_preflight` — does not exist
- `POST /v1/runs` — wrong (real path is `/v1/runs/` with trailing slash, sandbox via `is_sandbox=True` flag in body)
- `GET /v1/read_alerts` — does not exist (real: `GET /v1/runs/{run_id}/alerts`)

Real API surface (from `/openapi.json`):
- `POST /v1/runs/` (sandbox + scale, distinguished by `is_sandbox`)
- `POST /v1/runs/preflight/validate`
- `GET /v1/runs/{id}/alerts` (also POST to record an alert)
- `POST /v1/runs/{id}/{cancel,pause,resume,promote}`
- `GET/PUT /v1/integrations/keys[/{kind}]`

Update `docs/bootstrap.md` to match the actual endpoints, or add aliases in the API to match the documented paths.

## UI confusion: agent identity vs AI provider

Settings page mixes two distinct concepts:
- **Agent tokens** (`AGENT_TOKEN_*`) — bearer tokens identifying *who is calling Stellarator's API*
- **AI provider keys** (Tinker, OpenAI, OpenRouter, Codex OAuth) — credentials Stellarator uses to *call out* to those services

New operators consistently mis-paste their agent token into the Codex/OpenAI provider fields. Add a clarifying note or split the Settings page into two tabs: "Identity" + "Connected services".

## Healthcheck for supervisor reports unhealthy despite /healthz returning 200

Bash `/dev/tcp` + grep approach has timing flakiness. Replace with a proper helper script (or bake `curl` into supervisor.Dockerfile and revert to the simple `curl -fsS` healthcheck).
