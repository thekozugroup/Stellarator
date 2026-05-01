"""Tests for app/services/crypto.py — MultiFernet encryption helpers."""

from __future__ import annotations

import os

import pytest
from cryptography.fernet import InvalidToken

from app.services import crypto as crypto_mod
from app.services.crypto import decrypt, encrypt, re_encrypt, reset_cache_for_tests


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _reset(monkeypatch, primary: str, previous: str = "") -> None:
    """Set secrets and clear the MultiFernet cache."""
    monkeypatch.setattr("app.services.crypto.settings.stellarator_secret", primary)
    monkeypatch.setenv("STELLARATOR_SECRET_PREVIOUS", previous)
    reset_cache_for_tests()


# ---------------------------------------------------------------------------
# 1. Round-trip correctness
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("plaintext", [
    "hello world",
    "Unicode: é中文\U0001f600",
    "",
    "a" * (1024 * 1024),  # 1 MB binary-ish
])
def test_roundtrip(monkeypatch, plaintext):
    _reset(monkeypatch, "a" * 32)
    assert decrypt(encrypt(plaintext)) == plaintext


# ---------------------------------------------------------------------------
# 2. Ciphertext varies between calls (Fernet timestamp+iv)
# ---------------------------------------------------------------------------

def test_ciphertext_varies(monkeypatch):
    _reset(monkeypatch, "b" * 32)
    ct1 = encrypt("same plaintext")
    ct2 = encrypt("same plaintext")
    assert ct1 != ct2


# ---------------------------------------------------------------------------
# 3. Envelope version prefix
# ---------------------------------------------------------------------------

def test_envelope_version_prefix(monkeypatch):
    _reset(monkeypatch, "c" * 32)
    ct = encrypt("versioned")
    assert ct.startswith("v1:")


# ---------------------------------------------------------------------------
# 4. MultiFernet key rotation — decrypt with old key after rotation
# ---------------------------------------------------------------------------

def test_rotation_decrypt_with_previous_key(monkeypatch):
    old_secret = "o" * 32
    new_secret = "n" * 32

    # Encrypt under old key
    _reset(monkeypatch, old_secret)
    ciphertext = encrypt("rotate-me")

    # Rotate: new primary, old in PREVIOUS
    _reset(monkeypatch, new_secret, previous=old_secret)
    assert decrypt(ciphertext) == "rotate-me"


# ---------------------------------------------------------------------------
# 5. re_encrypt produces ciphertext decryptable under new primary alone
# ---------------------------------------------------------------------------

def test_re_encrypt_under_new_primary(monkeypatch):
    old_secret = "p" * 32
    new_secret = "q" * 32

    _reset(monkeypatch, old_secret)
    old_ct = encrypt("migrate-me")

    # Rotate
    _reset(monkeypatch, new_secret, previous=old_secret)
    new_ct = re_encrypt(old_ct)

    # Remove old key — only new primary
    _reset(monkeypatch, new_secret, previous="")
    assert decrypt(new_ct) == "migrate-me"


# ---------------------------------------------------------------------------
# 6. Tampered ciphertext raises ValueError
# ---------------------------------------------------------------------------

def test_tampered_ciphertext_raises(monkeypatch):
    _reset(monkeypatch, "t" * 32)
    ct = encrypt("sensitive")
    # Corrupt the Fernet token portion
    prefix, token = ct.split(":", 1)
    tampered = prefix + ":" + token[:-4] + "XXXX"
    with pytest.raises(ValueError, match="Invalid or tampered"):
        decrypt(tampered)


# ---------------------------------------------------------------------------
# 7. Unknown version prefix rejected
# ---------------------------------------------------------------------------

def test_unknown_version_prefix_rejected(monkeypatch):
    _reset(monkeypatch, "u" * 32)
    with pytest.raises(ValueError, match="Unknown ciphertext version"):
        decrypt("v99:sometokenhere")


# ---------------------------------------------------------------------------
# 8. TypeError on non-str input
# ---------------------------------------------------------------------------

def test_encrypt_none_raises():
    with pytest.raises(TypeError):
        encrypt(None)  # type: ignore[arg-type]


def test_decrypt_none_raises():
    with pytest.raises(TypeError):
        decrypt(None)  # type: ignore[arg-type]
