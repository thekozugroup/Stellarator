# Stellarator Security Model

## Identity Model

Every API request must carry a bearer token in the `Authorization: Bearer <token>` header.
Token resolution is per-request: the backend iterates the set of configured `AGENT_TOKEN_*`
environment variables and compares each against the supplied token using
`hmac.compare_digest` (constant-time) to prevent timing-oracle attacks. The matching token's
environment-variable name (e.g. `AGENT_TOKEN_SWEEP_BOT`) becomes the canonical agent identity
for the lifetime of that request. No session is issued; there is no token refresh.

---

## Trust Boundaries

| Boundary | Transport / Auth mechanism |
|---|---|
| Browser → Backend | TLS (terminator in front of uvicorn); `Authorization: Bearer <agent-token>` |
| Backend ↔ Rust supervisor | Internal Docker network; `X-Supervisor-Token` header (shared secret `SUPERVISOR_SHARED_SECRET`) |
| Backend → Tinker API | HTTPS; per-agent Tinker bearer token stored Fernet-encrypted in SQLite |
| Backend → OpenAI API | HTTPS; per-agent OpenAI key (user-supplied at runtime, stored in `sessionStorage` in browser, never in DB) |
| Backend → OpenRouter API | HTTPS; per-agent OpenRouter bearer stored Fernet-encrypted in SQLite |
| Backend ↔ SQLite | Filesystem ACL (`0600` on the DB file, owned by the process UID) |
| OAuth tokens (Codex / OpenAI) | Fernet-at-rest via `app.services.crypto`; Fernet key derived from `STELLARATOR_SECRET` via HKDF-SHA256 |

---

## Attack Surface Enumeration

### Token Theft

| Vector | Mitigation |
|---|---|
| Browser XSS stealing agent token | Agent tokens are not stored in the browser; the OpenAI key is stored in `sessionStorage` (inaccessible cross-origin, cleared on tab close) |
| Env-var leak via logs | `app.core.logging_filter.RedactionFilter` scrubs all configured secrets from log records before emission |
| Token in URL parameters | Auth is header-only; the codebase has no code path that appends bearer tokens to query strings; enforced by code review |
| Log aggregator leak | Redaction filter applies before the log record leaves the Python process; downstream aggregators never receive raw secrets |

### OAuth State Lifecycle

PKCE + nonce: the `oauth_state` table stores `code_verifier`, a random `nonce`, a `created_at`
timestamp, a `used_at` nullable timestamp, and the initiating `agent_id`.

- **Nonce burn**: `used_at` is set atomically on first consumption; subsequent requests for the
  same `state` see `used_at IS NOT NULL` and receive HTTP 400.
- **TTL**: states older than 10 minutes are rejected regardless of `used_at`.
- **Agent cross-check**: the `agent_id` stored in the state must match the token on the callback
  request; mismatches return HTTP 403.

### SSE Auth Replay

SSE streaming endpoints require a one-shot `stream_token` query parameter. The token is
single-use (`used_at` written on first connection) with a 10-minute TTL. Replay attempts after
first use are rejected with HTTP 401.

### Rate Limiting

`slowapi` enforces 10 requests / minute per IP on:
- `POST /v1/integrations/keys/{kind}/test`
- `POST /oauth/*/start`

A Redis backend can be plugged in by setting `SLOWAPI_STORAGE_URI=redis://…`; the default
in-process store is suitable for single-process deployments.

---

## Out of Scope

- **`id_token` signature validation**: the Google/Codex ID token is decoded for display (email
  extraction) only; we do not validate the JWT signature. The system does not grant access based
  on id_token claims.
- **Runtime secret rotation without restart**: `STELLARATOR_SECRET` is read once at boot;
  changing it requires a rolling restart (see `docs/KEY_ROTATION.md`).
- **Multi-tenant DB isolation**: all agents share a single SQLite database with row-level
  ownership checks; there is no schema-level or file-level tenant separation.

---

## STRIDE Threat Table

| Asset | S — Spoofing | T — Tampering | R — Repudiation | I — Info Disclosure | D — Denial of Service | E — Elevation of Privilege |
|---|---|---|---|---|---|---|
| **Tinker API keys** | Constant-time token compare prevents impersonation | Keys stored Fernet-encrypted; DB file ACL 0600 | Request log includes `agent_id` | Redaction filter; no key in URL | Rate limit on /test endpoint | Agent can only access their own key row (ownership check) |
| **Codex OAuth tokens** | PKCE + state nonce prevents CSRF injection | Fernet at rest; TLS in transit | OAuth callback logs `agent_id` + `state` | Tokens never logged; redaction filter | TTL + one-shot use limits replay storm | Agent cross-check on callback; token scoped to agent row |
| **OpenAI session key** | Not persisted server-side; XSS mitigation via sessionStorage | Not stored; validated at use | Requests logged with `agent_id` | sessionStorage scope; never transmitted to DB | No persistent state to attack | Key is ephemeral; cannot be elevated |
| **Run ownership** | Bearer auth required; agent_id stamped at run creation | Run mutations check `owner_agent_id == request.agent_id` | Run events written with timestamp + agent | Run data visible only to owning agent | Semaphore limits concurrent runs per agent | No escalation path; run CRUD is flat |
| **Chat sessions** | Bearer auth required | Chat messages stored with `agent_id`; mutation requires same agent | Messages timestamped in DB | Messages accessible only to session owner | WS semaphore limits concurrent connections | No privilege concept in chat layer |
