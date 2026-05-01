"""Symmetric encryption helpers for at-rest secrets.

Derives a Fernet key from ``stellarator_secret`` via HKDF-SHA256 so that
operators only manage one master secret. Used to encrypt OAuth tokens
before they hit the database.

Key Rotation Procedure
----------------------
1. Set ``STELLARATOR_SECRET_PREVIOUS`` to the current (old) value of
   ``STELLARATOR_SECRET`` (comma-separated if there are multiple previous
   generations).
2. Set ``STELLARATOR_SECRET`` to the new secret.
3. Restart the application. MultiFernet will now encrypt with the new key
   but can still decrypt ciphertexts produced under any previous key.
4. Run a re-encryption migration script that calls ``re_encrypt()`` on every
   stored ciphertext and writes the result back. Once all rows are migrated,
   the old key can be removed from ``STELLARATOR_SECRET_PREVIOUS``.

The envelope format is ``v1:<fernet-token>``. Decryption rejects any token
whose version prefix is not ``v1`` so future format changes are detectable.
"""

from __future__ import annotations

import base64
import os
from functools import lru_cache

from cryptography.fernet import Fernet, MultiFernet, InvalidToken
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

from app.core.config import settings

_HKDF_INFO = b"stellarator/fernet/v1"
_HKDF_SALT = b"stellarator-token-salt-v1"
_VERSION_PREFIX = "v1:"


def _derive_fernet(secret: str) -> Fernet:
    """Derive a Fernet instance from *secret* via HKDF-SHA256."""
    raw = HKDF(
        algorithm=hashes.SHA256(),
        length=32,
        salt=_HKDF_SALT,
        info=_HKDF_INFO,
    ).derive(secret.encode("utf-8"))
    return Fernet(base64.urlsafe_b64encode(raw))


@lru_cache(maxsize=1)
def _multi_fernet() -> MultiFernet:
    """Build a MultiFernet from the primary + any previous secrets.

    Primary secret comes from ``settings.stellarator_secret``.
    Previous secrets (for rotation) come from ``STELLARATOR_SECRET_PREVIOUS``
    (env var), comma-separated, each run through the same HKDF.
    """
    primary_secret = settings.stellarator_secret
    if not primary_secret:
        raise RuntimeError("STELLARATOR_SECRET must be set to derive an encryption key")

    primary = _derive_fernet(primary_secret)

    # Read previous secrets directly from the env so callers can override
    # without reloading pydantic settings.
    previous_raw = os.environ.get("STELLARATOR_SECRET_PREVIOUS", "")
    secondaries: list[Fernet] = []
    for part in previous_raw.split(","):
        part = part.strip()
        if part:
            secondaries.append(_derive_fernet(part))

    return MultiFernet([primary, *secondaries])


def encrypt(plaintext: str) -> str:
    """Encrypt a UTF-8 string and return a versioned ``v1:<token>`` envelope.

    Empty string in → empty string out (passthrough sentinel).
    """
    if plaintext is None:
        raise TypeError("plaintext must be a str")
    if plaintext == "":
        return ""
    token = _multi_fernet().encrypt(plaintext.encode("utf-8")).decode("ascii")
    return f"{_VERSION_PREFIX}{token}"


def decrypt(ciphertext: str) -> str:
    """Decrypt a token produced by :func:`encrypt`. Empty in → empty out.

    Raises :class:`ValueError` for tampered or unknown-version ciphertexts.
    """
    if ciphertext is None:
        raise TypeError("ciphertext must be a str")
    if ciphertext == "":
        return ""
    if not ciphertext.startswith(_VERSION_PREFIX):
        raise ValueError(f"Unknown ciphertext version; expected prefix '{_VERSION_PREFIX}'")
    token = ciphertext[len(_VERSION_PREFIX):]
    try:
        return _multi_fernet().decrypt(token.encode("ascii")).decode("utf-8")
    except InvalidToken as exc:
        raise ValueError("Invalid or tampered ciphertext") from exc


def re_encrypt(ciphertext: str) -> str:
    """Decrypt *ciphertext* (may be under any configured key) and re-encrypt
    under the current primary key.

    This is the building block for migration scripts run after key rotation.
    It does not write anything to the database — callers must persist the
    returned value themselves.

    Empty string passthrough mirrors :func:`encrypt`/:func:`decrypt`.
    """
    if ciphertext == "":
        return ""
    plaintext = decrypt(ciphertext)
    return encrypt(plaintext)


def reset_cache() -> None:
    """Drop the cached MultiFernet instance.

    Call this after rotating ``STELLARATOR_SECRET`` / ``STELLARATOR_SECRET_PREVIOUS``
    at runtime so subsequent encrypt/decrypt calls pick up the new keys without
    requiring a process restart.  Also used by tests to isolate key state.

    Example admin hook::

        from app.services.crypto import reset_cache
        reset_cache()
    """
    _multi_fernet.cache_clear()


def reset_cache_for_tests() -> None:
    """Alias kept for backwards compatibility with existing test code."""
    reset_cache()
