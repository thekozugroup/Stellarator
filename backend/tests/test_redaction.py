"""Tests for app/core/logging_filter.py — dynamic secret redaction."""

from __future__ import annotations

import logging

import pytest

from app.core.logging_filter import REDACTED, SecretRedactingFilter


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_record(msg: str, args=None) -> logging.LogRecord:
    return logging.LogRecord(
        name="test",
        level=logging.INFO,
        pathname=__file__,
        lineno=1,
        msg=msg,
        args=args,
        exc_info=None,
    )


# ---------------------------------------------------------------------------
# 1. Known token in log line gets replaced with ***REDACTED***
# ---------------------------------------------------------------------------

def test_redacts_known_token_in_msg():
    secret = "supersecret-token-abcdef12"
    flt = SecretRedactingFilter(lambda: [secret])

    record = _make_record(f"bearer leaked: {secret}")
    flt.filter(record)

    assert secret not in record.msg
    assert REDACTED in record.msg


def test_redacts_known_token_in_args():
    secret = "another-secret-value-xyz99"
    flt = SecretRedactingFilter(lambda: [secret])

    record = _make_record("value: %s", args=(secret,))
    flt.filter(record)

    rendered = record.getMessage()
    assert secret not in rendered
    assert REDACTED in rendered


# ---------------------------------------------------------------------------
# 2. Runtime rotation — provider updates, new token gets redacted
# ---------------------------------------------------------------------------

def test_runtime_rotation_new_token_redacted():
    secrets_store: list[str] = ["initial-secret-value-abc"]

    flt = SecretRedactingFilter(lambda: list(secrets_store))

    # Initial secret gets redacted
    r1 = _make_record("token: initial-secret-value-abc")
    flt.filter(r1)
    assert "initial-secret-value-abc" not in r1.msg

    # Rotate — add a new secret at runtime
    new_secret = "rotated-secret-value-xyz99"
    secrets_store.append(new_secret)

    r2 = _make_record(f"token: {new_secret}")
    flt.filter(r2)
    assert new_secret not in r2.msg
    assert REDACTED in r2.msg


# ---------------------------------------------------------------------------
# 3. Short tokens (< 8 chars) NOT redacted
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("short_token", ["abc", "secret", "1234567", "x" * 7])
def test_short_tokens_not_redacted(short_token):
    flt = SecretRedactingFilter(lambda: [short_token])

    record = _make_record(f"value: {short_token}")
    flt.filter(record)

    # Short token should NOT be in the filter's active secrets list
    active = flt._current_secrets()
    assert short_token not in active

    # And the original record should be unchanged
    assert short_token in record.msg
    assert REDACTED not in record.msg


# ---------------------------------------------------------------------------
# 4. Empty provider returns True (filter passes through without crash)
# ---------------------------------------------------------------------------

def test_empty_provider_passthrough():
    flt = SecretRedactingFilter(lambda: [])

    record = _make_record("nothing to redact here")
    result = flt.filter(record)
    assert result is True
    assert record.msg == "nothing to redact here"


# ---------------------------------------------------------------------------
# 5. Provider raising an exception does not break logging
# ---------------------------------------------------------------------------

def test_broken_provider_does_not_crash_logging():
    def bad_provider():
        raise RuntimeError("provider exploded")

    flt = SecretRedactingFilter(bad_provider)
    record = _make_record("some log message")
    result = flt.filter(record)
    assert result is True


# ---------------------------------------------------------------------------
# 6. Longest-first ordering prevents prefix leak
# ---------------------------------------------------------------------------

def test_longest_first_prevents_prefix_leak():
    short = "secretprefix"
    long_ = "secretprefix-extended-value"
    flt = SecretRedactingFilter(lambda: [short, long_])

    record = _make_record(f"found: {long_}")
    flt.filter(record)

    # The full long secret must be redacted, not just the short prefix
    assert long_ not in record.msg
    assert REDACTED in record.msg
