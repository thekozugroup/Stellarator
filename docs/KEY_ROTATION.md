# Key Rotation Runbook

This document covers rotation procedures for the four secret types used by Stellarator.
Each section follows the same structure: precondition → command sequence → validation → rollback.

---

## 1. `STELLARATOR_SECRET` (Fernet / HKDF root)

**What it protects**: All Fernet-encrypted fields in SQLite (OAuth tokens, integration keys).
Changing this key without re-encrypting existing rows will make those rows unreadable.

### Preconditions

- Maintenance window or rolling restart accepted.
- Database backup taken: `cp stellarator.db stellarator.db.bak`.
- New secret generated (≥ 32 bytes of entropy):
  ```bash
  python3 -c "import secrets; print(secrets.token_hex(32))"
  ```

### Command Sequence

1. Set both the old and new secrets in the environment:
   ```bash
   export STELLARATOR_SECRET_PREVIOUS="<old-secret>"
   export STELLARATOR_SECRET="<new-secret>"
   ```

2. Run the re-encryption helper (reads `STELLARATOR_SECRET_PREVIOUS`, re-encrypts all
   Fernet-protected columns under `STELLARATOR_SECRET`):
   ```bash
   cd backend
   python3 -m app.services.crypto re_encrypt
   ```
   The helper prints a count of rows migrated. Verify it matches the expected row count.

3. Restart the backend with only `STELLARATOR_SECRET` set (remove `STELLARATOR_SECRET_PREVIOUS`):
   ```bash
   docker compose restart backend
   ```

### Validation

```bash
curl -sf -H "Authorization: Bearer $AGENT_TOKEN" \
     http://localhost:8000/v1/integrations/keys/tinker \
     | jq '.configured'
# Must return true, not an error.
```

### Rollback

Stop the backend, restore the backup, set `STELLARATOR_SECRET` back to the old value, restart.

---

## 2. `SUPERVISOR_SHARED_SECRET`

**What it protects**: Backend ↔ Rust supervisor channel (`X-Supervisor-Token` header).
Both processes must hold the same value simultaneously.

### Option A — Coordinated restart (brief outage)

1. Generate a new secret:
   ```bash
   python3 -c "import secrets; print(secrets.token_hex(32))"
   ```
2. Update the secret in your secrets store / `.env` for both `backend` and `supervisor`.
3. Stop both services:
   ```bash
   docker compose stop supervisor backend
   ```
4. Start both services with the new secret:
   ```bash
   docker compose up -d supervisor backend
   ```

### Option B — Dual-secret transition (zero downtime)

The Rust supervisor can be patched to accept either old or new token for a brief window:

1. Set `SUPERVISOR_SHARED_SECRET_PREVIOUS=<old>` and `SUPERVISOR_SHARED_SECRET=<new>` in the
   supervisor's environment.
2. Deploy the supervisor (it now accepts both).
3. Restart the backend with only `SUPERVISOR_SHARED_SECRET=<new>`.
4. Remove `SUPERVISOR_SHARED_SECRET_PREVIOUS` from the supervisor and redeploy.

### Validation

```bash
curl -sf -H "X-Supervisor-Token: $SUPERVISOR_SHARED_SECRET" \
     http://supervisor:8001/healthz
# Expected: {"status":"ok"}
```

### Rollback

Set both services back to the previous secret and restart.

---

## 3. `AGENT_TOKEN_*` (per-agent bearer tokens)

**What they protect**: API authentication for each agent identity.

### Preconditions

- Identify the token being rotated (e.g. `AGENT_TOKEN_SWEEP_BOT`).
- Notify the agent operator that their token will be revoked.

### Command Sequence

1. Generate a new token:
   ```bash
   python3 -c "import secrets; print(secrets.token_hex(32))"
   ```
2. Add the new token to the backend environment alongside the old one temporarily:
   ```
   AGENT_TOKEN_SWEEP_BOT_NEW=<new-token>
   AGENT_TOKEN_SWEEP_BOT=<old-token>
   ```
3. Restart the backend. Both tokens are now valid (they have different env-var names).
4. Communicate the new token to the agent operator.
5. Once the operator confirms migration, remove `AGENT_TOKEN_SWEEP_BOT` and rename
   `AGENT_TOKEN_SWEEP_BOT_NEW` → `AGENT_TOKEN_SWEEP_BOT`, then restart again.
6. Update the MCP server `.env`:
   ```
   STELLARATOR_TOKEN=<new-token>
   ```
7. Restart MCP server processes; agents will re-authenticate on next request.

### Validation

```bash
curl -sf -H "Authorization: Bearer <new-token>" \
     http://localhost:8000/v1/whoami
# Must return the correct agent identity.

curl -sf -H "Authorization: Bearer <old-token>" \
     http://localhost:8000/v1/whoami
# Must return 401 after old token is removed.
```

### Rollback

Re-add the old `AGENT_TOKEN_*` env var and restart the backend.

---

## 4. Per-agent Tinker / OpenRouter Integration Keys

**What they protect**: Third-party API access on behalf of individual agents.
These are fully self-service — no admin action is required.

### User Self-Service Procedure

An agent operator updates their own key via the integrations API:

```bash
# Rotate a Tinker key
curl -X PUT http://localhost:8000/v1/integrations/keys/tinker \
     -H "Authorization: Bearer $AGENT_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"key": "<new-tinker-api-key>"}'

# Rotate an OpenRouter key
curl -X PUT http://localhost:8000/v1/integrations/keys/openrouter \
     -H "Authorization: Bearer $AGENT_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"key": "<new-openrouter-api-key>"}'
```

The backend Fernet-encrypts the new value and overwrites the existing row.

### Validation

```bash
curl -sf -H "Authorization: Bearer $AGENT_TOKEN" \
     http://localhost:8000/v1/integrations/keys/tinker/test
# Expected: {"ok": true}
```

### Rollback

Re-PUT the previous key value. The backend stores only the latest value; no history is retained.
