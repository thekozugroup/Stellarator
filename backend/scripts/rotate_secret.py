"""Automated STELLARATOR_SECRET rotation script.

Decrypts every Fernet-encrypted column in codex_tokens, openai_tokens, and
integration_keys under the *current* secret, then re-encrypts under the *new*
secret.  All writes are wrapped in a single database transaction; the default
mode is dry-run — pass ``--commit`` to persist changes.

Usage
-----
    # Dry-run (default) — shows rows that would be touched, exits 1
    python -m scripts.rotate_secret --new-secret <64-char hex>

    # Live rotation
    python -m scripts.rotate_secret --new-secret <64-char hex> --commit

    # Supply new secret via env instead of CLI flag
    STELLARATOR_SECRET_NEW=<hex> python -m scripts.rotate_secret --commit

Exit codes
----------
0   Success (committed).
1   Dry-run preview completed (no writes).
2   Failure (exception or validation error).

See backend/scripts/README.md for the full operator runbook.
"""

from __future__ import annotations

import argparse
import os
import random
import sys
from typing import Any

# ---------------------------------------------------------------------------
# Crypto helpers
# ---------------------------------------------------------------------------

import base64

from cryptography.fernet import Fernet, InvalidToken
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

_HKDF_INFO = b"stellarator/fernet/v1"
_HKDF_SALT = b"stellarator-token-salt-v1"
_VERSION_PREFIX = "v1:"


def _derive_fernet(secret: str) -> Fernet:
    raw = HKDF(
        algorithm=hashes.SHA256(),
        length=32,
        salt=_HKDF_SALT,
        info=_HKDF_INFO,
    ).derive(secret.encode("utf-8"))
    return Fernet(base64.urlsafe_b64encode(raw))


def _decrypt(ciphertext: str, fernet: Fernet) -> str:
    """Strip the v1: prefix and decrypt.  Empty string passthrough."""
    if ciphertext == "":
        return ""
    if not ciphertext.startswith(_VERSION_PREFIX):
        raise ValueError(f"Unknown ciphertext version; expected '{_VERSION_PREFIX}' prefix")
    token = ciphertext[len(_VERSION_PREFIX):]
    try:
        return fernet.decrypt(token.encode("ascii")).decode("utf-8")
    except InvalidToken as exc:
        raise ValueError("Invalid or tampered ciphertext") from exc


def _encrypt(plaintext: str, fernet: Fernet) -> str:
    """Encrypt and prepend the v1: envelope.  Empty string passthrough."""
    if plaintext == "":
        return ""
    token = fernet.encrypt(plaintext.encode("utf-8")).decode("ascii")
    return f"{_VERSION_PREFIX}{token}"


def _re_encrypt(ciphertext: str, old: Fernet, new: Fernet) -> tuple[str, bool]:
    """Return (new_ciphertext, changed).

    Idempotent: if *ciphertext* is already encrypted under *new*, decryption
    with *old* will raise InvalidToken.  We detect that and verify it decrypts
    under *new* instead — meaning the rotation already happened, so we return
    the original ciphertext unchanged.
    """
    if ciphertext == "":
        return ciphertext, False
    try:
        plaintext = _decrypt(ciphertext, old)
    except ValueError:
        # May already be encrypted under the new key — verify.
        try:
            _decrypt(ciphertext, new)
            return ciphertext, False  # already rotated — no-op
        except ValueError:
            raise ValueError(
                "Ciphertext decrypts under neither old nor new key. "
                "Check STELLARATOR_SECRET and --new-secret."
            )
    new_ct = _encrypt(plaintext, new)
    return new_ct, True


# ---------------------------------------------------------------------------
# Database access (synchronous SQLAlchemy — rotation is a maintenance op)
# ---------------------------------------------------------------------------

def _sync_engine(db_url: str):  # type: ignore[return]
    """Create a synchronous SQLAlchemy engine, stripping async driver prefixes."""
    from sqlalchemy import create_engine

    sync_url = db_url.replace("+aiosqlite", "").replace("+asyncpg", "")
    return create_engine(sync_url)


# Table / column specification
_TABLE_SPECS: list[dict[str, Any]] = [
    {
        "table": "codex_tokens",
        "pk": "agent_id",
        "columns": ["access_token", "refresh_token"],
    },
    {
        "table": "openai_tokens",
        "pk": "agent_id",
        "columns": ["access_token", "refresh_token"],
    },
    {
        "table": "integration_keys",
        "pk": "id",
        "columns": ["ciphertext"],
    },
]


def _rotate(
    db_url: str,
    old_secret: str,
    new_secret: str,
    commit: bool,
    dry_run_log: list[str],
) -> dict[str, int]:
    """Perform (or simulate) the rotation.  Returns rows-touched per table."""
    from sqlalchemy import text as sa_text

    engine = _sync_engine(db_url)
    old_fernet = _derive_fernet(old_secret)
    new_fernet = _derive_fernet(new_secret)

    touched: dict[str, int] = {}

    with engine.begin() as conn:
        for spec in _TABLE_SPECS:
            table = spec["table"]
            pk = spec["pk"]
            columns = spec["columns"]

            col_list = ", ".join([pk] + columns)
            rows = conn.execute(sa_text(f"SELECT {col_list} FROM {table}")).fetchall()  # noqa: S608

            table_touched = 0
            for row in rows:
                pk_val = row[0]
                updates: dict[str, str] = {}

                for i, col in enumerate(columns):
                    original_ct = row[i + 1] or ""
                    if not original_ct:
                        continue
                    new_ct, changed = _re_encrypt(original_ct, old_fernet, new_fernet)
                    if changed:
                        updates[col] = new_ct

                if not updates:
                    continue

                table_touched += 1
                set_clause = ", ".join(f"{c} = :{c}" for c in updates)
                params = {**updates, "pk_val": pk_val}
                stmt = sa_text(
                    f"UPDATE {table} SET {set_clause} WHERE {pk} = :pk_val"  # noqa: S608
                )

                if commit:
                    conn.execute(stmt, params)
                else:
                    dry_run_log.append(
                        f"  [dry-run] would UPDATE {table} SET {', '.join(updates)} "
                        f"WHERE {pk}={pk_val!r}"
                    )

            touched[table] = table_touched

        if not commit:
            # Roll back explicitly — belt-and-suspenders in dry-run mode.
            conn.rollback()

    return touched


def _validate(db_url: str, new_secret: str) -> None:
    """Re-decrypt one random row from each table under the new key."""
    from sqlalchemy import text as sa_text

    engine = _sync_engine(db_url)
    new_fernet = _derive_fernet(new_secret)

    with engine.connect() as conn:
        for spec in _TABLE_SPECS:
            table = spec["table"]
            pk = spec["pk"]
            columns = spec["columns"]

            rows = conn.execute(sa_text(f"SELECT {pk}, {columns[0]} FROM {table}")).fetchall()  # noqa: S608
            if not rows:
                print(f"  [validate] {table}: no rows — skipping")
                continue

            sample_pk, sample_ct = random.choice(rows)  # noqa: S311 (non-crypto random OK here)
            sample_ct = sample_ct or ""
            if not sample_ct:
                print(f"  [validate] {table}: sampled row has empty {columns[0]} — skipping")
                continue

            # Will raise ValueError if decryption fails — surfaces as exit code 2.
            _decrypt(sample_ct, new_fernet)
            print(f"  [validate] {table}: OK (pk={sample_pk!r}, col={columns[0]})")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Rotate STELLARATOR_SECRET: re-encrypt all stored tokens under a new key."
    )
    parser.add_argument(
        "--new-secret",
        metavar="HEX",
        default=os.environ.get("STELLARATOR_SECRET_NEW", ""),
        help=(
            "New secret (hex string).  Defaults to $STELLARATOR_SECRET_NEW. "
            "Generate with: openssl rand -hex 32"
        ),
    )
    parser.add_argument(
        "--commit",
        action="store_true",
        default=False,
        help="Persist changes (default: dry-run, exit 1).",
    )
    parser.add_argument(
        "--db-url",
        metavar="URL",
        default=os.environ.get(
            "STELLARATOR_DB_URL",
            "sqlite+aiosqlite:////data/stellarator.db",
        ),
        help="SQLAlchemy database URL (default: $STELLARATOR_DB_URL or SQLite /data path).",
    )
    return parser.parse_args()


def main() -> None:
    args = _parse_args()

    old_secret = os.environ.get("STELLARATOR_SECRET", "")
    new_secret = args.new_secret

    # --- Validation ---------------------------------------------------------
    if not old_secret:
        print("ERROR: STELLARATOR_SECRET environment variable is not set.", file=sys.stderr)
        sys.exit(2)

    if not new_secret:
        print(
            "ERROR: Provide --new-secret <hex> or set $STELLARATOR_SECRET_NEW.",
            file=sys.stderr,
        )
        sys.exit(2)

    if old_secret == new_secret:
        print("INFO: new secret is identical to current secret — nothing to do.")
        sys.exit(0)

    # Redact secrets from stdout — never log raw secret values.
    print(f"Rotation mode   : {'COMMIT' if args.commit else 'DRY-RUN'}")
    print(f"Database        : {args.db_url}")
    print(f"Old secret      : <redacted, len={len(old_secret)}>")
    print(f"New secret      : <redacted, len={len(new_secret)}>")
    print()

    dry_run_log: list[str] = []

    try:
        touched = _rotate(
            db_url=args.db_url,
            old_secret=old_secret,
            new_secret=new_secret,
            commit=args.commit,
            dry_run_log=dry_run_log,
        )
    except Exception as exc:  # noqa: BLE001
        print(f"ERROR: rotation failed: {exc}", file=sys.stderr)
        sys.exit(2)

    # --- Report -------------------------------------------------------------
    for line in dry_run_log:
        print(line)

    print("Rows touched per table:")
    for table, count in touched.items():
        print(f"  {table}: {count}")

    total = sum(touched.values())
    print(f"  TOTAL: {total}")
    print()

    if args.commit:
        print("Validating re-encryption (sampling one row per table)…")
        try:
            _validate(args.db_url, new_secret)
        except Exception as exc:  # noqa: BLE001
            print(f"ERROR: post-commit validation failed: {exc}", file=sys.stderr)
            sys.exit(2)
        print()
        print("SUCCESS — rotation committed and validated.")
        print()
        print("Next steps:")
        print(
            "  export STELLARATOR_SECRET=<new>  STELLARATOR_SECRET_PREVIOUS=<old>  # then restart"
        )
        sys.exit(0)
    else:
        print("DRY-RUN complete — no changes written.")
        print("Re-run with --commit to persist.")
        sys.exit(1)


if __name__ == "__main__":
    main()
