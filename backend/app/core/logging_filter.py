"""Logging filter that redacts known secret values from log records.

Scans ``LogRecord.msg`` and ``LogRecord.args`` (rendered to ``str``) for any
substring matching a configured agent token, OAuth client secret, or API
key, and replaces it with ``***REDACTED***``.

Intended to also defang ``httpx`` exception ``repr``s, which embed the
``Authorization`` header value verbatim.
"""

from __future__ import annotations

import logging
import time
from typing import Iterable

REDACTED = "***REDACTED***"

_SECRETS_TTL_SECS = 5.0


class SecretRedactingFilter(logging.Filter):
    """Replace any configured secret substring with ``***REDACTED***``."""

    # 8 chars is a reasonable floor — anything shorter is far too generic
    # to safely substring-match in arbitrary log lines.
    _MIN_LEN = 8

    def __init__(self, secrets_provider) -> None:  # type: ignore[no-untyped-def]
        super().__init__()
        # ``secrets_provider`` is a zero-arg callable returning the current
        # list of secret strings. Passing a callable (rather than a static
        # list) means rotated/late-set tokens still get redacted without
        # re-wiring the filter.
        self._provider = secrets_provider
        # TTL cache state.
        self._cached_secrets: list[str] = []
        self._cache_expires_at: float = 0.0

    def _current_secrets(self) -> list[str]:
        now = time.monotonic()
        if now < self._cache_expires_at:
            return self._cached_secrets
        # TTL miss — recompute.
        try:
            raw: Iterable[str] = self._provider() or []
        except Exception:  # noqa: BLE001 - never let the filter raise
            return []
        # Sort longest-first so overlapping prefixes don't leak.
        secrets = sorted(
            {s for s in raw if isinstance(s, str) and len(s) >= self._MIN_LEN},
            key=len,
            reverse=True,
        )
        self._cached_secrets = secrets
        self._cache_expires_at = now + _SECRETS_TTL_SECS
        return secrets

    def _scrub(self, value: str, secrets: list[str]) -> str:
        for s in secrets:
            if s and s in value:
                value = value.replace(s, REDACTED)
        return value

    def filter(self, record: logging.LogRecord) -> bool:  # noqa: A003
        secrets = self._current_secrets()
        if not secrets:
            return True

        try:
            if isinstance(record.msg, str):
                record.msg = self._scrub(record.msg, secrets)
            else:
                record.msg = self._scrub(str(record.msg), secrets)

            if record.args:
                if isinstance(record.args, dict):
                    record.args = {k: self._scrub(str(v), secrets) for k, v in record.args.items()}
                elif isinstance(record.args, tuple):
                    record.args = tuple(self._scrub(str(v), secrets) for v in record.args)

            if record.exc_info and record.exc_info[1] is not None:
                # Pre-render exception text so the cached ``exc_text`` we hand
                # to formatters is already scrubbed.
                exc = record.exc_info[1]
                record.exc_text = self._scrub(repr(exc), secrets)
        except Exception:  # noqa: BLE001 - never break logging
            return True
        return True


def install_redaction_filter(secrets_provider) -> SecretRedactingFilter:  # type: ignore[no-untyped-def]
    """Attach the filter to the root logger and to ``httpx``'s logger."""
    flt = SecretRedactingFilter(secrets_provider)
    logging.getLogger().addFilter(flt)
    logging.getLogger("httpx").addFilter(flt)
    logging.getLogger("httpcore").addFilter(flt)
    logging.getLogger("uvicorn.error").addFilter(flt)
    logging.getLogger("uvicorn.access").addFilter(flt)
    return flt
