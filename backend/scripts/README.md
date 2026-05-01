# backend/scripts — Operational Tooling

## rotate_secret.py — Automated STELLARATOR_SECRET Rotation

Re-encrypts all Fernet-protected columns (`codex_tokens`, `openai_tokens`,
`integration_keys`) from the current secret to a new one, in a single
database transaction.

### Quick reference

```bash
# 1. Generate a new secret
NEW=$(openssl rand -hex 32)

# 2. Dry-run — inspect what would change (exits 1)
STELLARATOR_SECRET=$OLD python -m scripts.rotate_secret --new-secret "$NEW"

# 3. Commit the rotation (exits 0 on success, 2 on failure)
STELLARATOR_SECRET=$OLD python -m scripts.rotate_secret --new-secret "$NEW" --commit

# 4. Update environment and restart the application
export STELLARATOR_SECRET=$NEW
export STELLARATOR_SECRET_PREVIOUS=$OLD   # allows decryption of any missed rows
# restart backend / supervisor

# 5. Once all services have restarted and traffic looks normal, remove the
#    previous secret from the environment on the next deploy.
```

Or use the `$STELLARATOR_SECRET_NEW` environment variable instead of `--new-secret`:

```bash
STELLARATOR_SECRET=$OLD STELLARATOR_SECRET_NEW=$NEW \
    python -m scripts.rotate_secret --commit
```

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Committed and validated successfully |
| 1 | Dry-run preview — no changes written |
| 2 | Error (decryption failure, DB error, bad args) |

### Idempotency

Running the script twice with the same `--new-secret` on an already-rotated
database is a no-op.  Each ciphertext is attempted under the old key first; if
that fails (because it was already re-encrypted under the new key) the script
verifies it decrypts under the new key and skips the row.

### Columns rotated

| Table | Columns |
|-------|---------|
| `codex_tokens` | `access_token`, `refresh_token` |
| `openai_tokens` | `access_token`, `refresh_token` |
| `integration_keys` | `ciphertext` |

### Security notes

- Raw secret values are never printed; only their lengths are logged.
- The script uses a synchronous SQLAlchemy connection so it can run outside
  the async FastAPI application context.
- Post-commit validation decrypts one random row per table under the new key
  to confirm correctness before reporting success.
