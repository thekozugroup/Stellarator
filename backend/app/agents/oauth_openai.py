"""OpenAI OAuth 2.0 authorization-code + PKCE flow ("Sign in with OpenAI").

Endpoints:
  GET  /v1/oauth/openai/start     - redirect to OpenAI authorization URL
  GET  /v1/oauth/openai/callback  - exchange code, persist tokens, redirect to /

Architecture mirrors :mod:`app.agents.oauth_codex` exactly — the only deltas
are the table written (``openai_tokens``), the env-driven endpoints, and
extraction of the ``email`` claim from the returned ``id_token`` for display.

Endpoints (defaults, override via env)
--------------------------------------
- Authorization: ``OPENAI_OAUTH_AUTH_URL`` (default
  ``https://auth.openai.com/oauth/authorize``)
- Token:         ``OPENAI_OAUTH_TOKEN_URL`` (default
  ``https://auth.openai.com/oauth/token``)

These are the same auth.openai.com endpoints that Codex uses today; if OpenAI
publishes a separate "Sign in with OpenAI" client surface, point the env vars
at it. See https://platform.openai.com/docs for the latest reference.

Hardening (identical to oauth_codex.py)
---------------------------------------
- HMAC-SHA256 signed state ``{agent, nonce, iat, exp}`` with strict expiry.
- Replay defeated via in-process LRU set of burned nonces (size 1024).
- PKCE S256, ``code_verifier`` bound to nonce so the same browser must finish.
- Initiating ``current_agent`` must equal the agent on the callback request.
- ``redirect_uri`` honours ``OPENAI_OAUTH_REDIRECT_URI``, else
  ``request.url_for``.
- Tokens encrypted at rest via :mod:`app.services.crypto`.
- ``id_token`` claims are NEVER logged; only ``email`` is persisted (plain).
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import json
import logging
import secrets
import time
from datetime import datetime, timedelta, timezone
from urllib.parse import urlencode

import httpx
from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import RedirectResponse
from sqlalchemy import select, update
from sqlalchemy.ext.asyncio import AsyncSession

from app.core.auth import CurrentAgent
from app.core.config import settings
from app.core.db import SessionLocal, get_session
from app.models.oauth import OpenAIToken
from app.models.oauth_state import OAuthState
from app.services.crypto import encrypt

try:
    from app.core.rate_limit import limiter as _limiter
    _rate_limit_available = True
except ImportError:
    _rate_limit_available = False

from app.core.rate_limit import maybe_limit
_start_limit = maybe_limit("10/minute")

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/v1/oauth/openai", tags=["oauth"])

_PROVIDER = "openai"
_STATE_TTL_SECS = 600


async def _db_remember_pending(
    nonce: str, agent: str, code_verifier: str, session: AsyncSession
) -> None:
    """Persist a pending OAuth state row. Expires in STATE_TTL_SECS."""
    now = datetime.now(tz=timezone.utc)
    row = OAuthState(
        provider=_PROVIDER,
        agent_id=agent,
        nonce=nonce,
        code_verifier=code_verifier,
        expires_at=now + timedelta(seconds=_STATE_TTL_SECS),
        used_at=None,
    )
    session.add(row)
    await session.flush()


async def _db_consume_pending(nonce: str, session: AsyncSession) -> OAuthState | None:
    """Atomically consume a pending state row (one-shot).

    Returns the row if valid and unused, or None if expired / already consumed.
    Marks used_at=now to prevent replay across workers.
    """
    now = datetime.now(tz=timezone.utc)
    # NOTE: with_for_update is a no-op on SQLite. Single-row consume is still
    # safe under SQLite WAL + busy_timeout (see core/sqlite_pragmas.py) because
    # the `used_at IS NULL` predicate combined with the subsequent UPDATE+commit
    # serializes contention. The lock is here for future Postgres deployments.
    result = await session.execute(
        select(OAuthState).where(
            OAuthState.nonce == nonce,
            OAuthState.provider == _PROVIDER,
            OAuthState.used_at.is_(None),
            OAuthState.expires_at > now,
        ).with_for_update()
    )
    row = result.scalar_one_or_none()
    if row is None:
        return None
    row.used_at = now
    await session.flush()
    return row


def _sign_state(payload: dict) -> str:
    body = json.dumps(payload, separators=(",", ":"), sort_keys=True)
    sig = hmac.new(
        settings.stellarator_secret.encode("utf-8"),
        body.encode("utf-8"),
        hashlib.sha256,
    ).hexdigest()
    data = json.dumps({"p": payload, "s": sig}, separators=(",", ":"), sort_keys=True)
    return base64.urlsafe_b64encode(data.encode("utf-8")).decode("ascii").rstrip("=")


def _verify_state(raw: str) -> dict:
    try:
        padding = "=" * (-len(raw) % 4)
        decoded = base64.urlsafe_b64decode(raw + padding).decode("utf-8")
        data = json.loads(decoded)
        payload = data["p"]
        sig = data["s"]
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(400, "Invalid OAuth state") from exc

    body = json.dumps(payload, separators=(",", ":"), sort_keys=True)
    expected = hmac.new(
        settings.stellarator_secret.encode("utf-8"),
        body.encode("utf-8"),
        hashlib.sha256,
    ).hexdigest()

    if not hmac.compare_digest(sig, expected):
        raise HTTPException(400, "OAuth state signature mismatch")

    now = int(time.time())
    iat = int(payload.get("iat", 0))
    exp = int(payload.get("exp", 0))
    if exp <= 0 or now > exp or now < iat - 5:
        raise HTTPException(400, "OAuth state expired")

    return payload


def _resolve_redirect_uri(request: Request) -> str:
    if settings.openai_oauth_redirect_uri:
        return settings.openai_oauth_redirect_uri
    return str(request.url_for("openai_oauth_callback"))


def _pkce_pair() -> tuple[str, str]:
    verifier = base64.urlsafe_b64encode(secrets.token_bytes(64)).decode("ascii").rstrip("=")
    challenge = (
        base64.urlsafe_b64encode(hashlib.sha256(verifier.encode("ascii")).digest())
        .decode("ascii")
        .rstrip("=")
    )
    return verifier, challenge


# SECURITY: _email_from_id_token decodes WITHOUT signature verification.
# Returned email is for DISPLAY ONLY — do not use for any authorization
# decision. If you ever do, switch to fetching JWKS and validating the JWT
# via python-jose (see https://python-jose.readthedocs.io/).


def _email_from_id_token(id_token: str) -> str | None:
    """Best-effort extraction of the ``email`` claim from an unverified id_token.

    We do NOT validate the signature here — the token came from a trusted
    direct exchange over TLS with the OAuth server in the same request.
    Anything beyond ``email`` is intentionally discarded; never log claims.
    """
    if not id_token:
        return None
    try:
        parts = id_token.split(".")
        if len(parts) < 2:
            return None
        payload_b64 = parts[1]
        padding = "=" * (-len(payload_b64) % 4)
        decoded = base64.urlsafe_b64decode(payload_b64 + padding).decode("utf-8")
        claims = json.loads(decoded)
        email = claims.get("email")
        if not isinstance(email, str) or "@" not in email:
            return None
        return email
    except Exception:  # noqa: BLE001
        return None


@router.get("/start", name="openai_oauth_start")
@_start_limit
async def oauth_start(
    request: Request,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
):
    """Redirect the agent's browser to the OpenAI authorization URL."""
    if not settings.openai_oauth_client_id:
        raise HTTPException(503, "OpenAI OAuth not configured")

    nonce = secrets.token_hex(16)
    iat = int(time.time())
    verifier, challenge = _pkce_pair()
    await _db_remember_pending(nonce, agent, verifier, session)
    await session.commit()

    state = _sign_state(
        {"agent": agent, "nonce": nonce, "iat": iat, "exp": iat + _STATE_TTL_SECS}
    )

    redirect_uri = _resolve_redirect_uri(request)
    params = {
        "response_type": "code",
        "client_id": settings.openai_oauth_client_id,
        "redirect_uri": redirect_uri,
        "scope": settings.openai_oauth_scopes,
        "state": state,
        "code_challenge": challenge,
        "code_challenge_method": "S256",
    }
    return RedirectResponse(f"{settings.openai_oauth_auth_url}?{urlencode(params)}")


@router.get("/callback", name="openai_oauth_callback")
async def oauth_callback(
    request: Request,
    code: str,
    state: str,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
):
    """Exchange authorization code for tokens and persist them encrypted."""
    payload = _verify_state(state)
    nonce = payload.get("nonce", "")
    state_agent = payload.get("agent", "")

    pending_row = await _db_consume_pending(nonce, session)
    if pending_row is None:
        raise HTTPException(400, "OAuth state already used or unknown")
    if pending_row.agent_id != state_agent or state_agent != agent:
        raise HTTPException(403, "OAuth callback agent does not match initiating agent")

    redirect_uri = _resolve_redirect_uri(request)

    async with httpx.AsyncClient(timeout=30.0) as client:
        r = await client.post(
            settings.openai_oauth_token_url,
            data={
                "grant_type": "authorization_code",
                "code": code,
                "redirect_uri": redirect_uri,
                "client_id": settings.openai_oauth_client_id,
                "client_secret": settings.openai_oauth_client_secret,
                "code_verifier": pending_row.code_verifier,
            },
            headers={"Content-Type": "application/x-www-form-urlencoded"},
        )
        if r.status_code >= 400:
            # Never include the response body — could leak tokens or PII.
            raise HTTPException(502, "OpenAI token exchange failed")
        token_data = r.json()

    now = datetime.now(tz=timezone.utc)
    expires_at = (
        now + timedelta(seconds=int(token_data["expires_in"]))
        if token_data.get("expires_in")
        else None
    )

    email = _email_from_id_token(token_data.get("id_token", ""))

    result = await session.execute(
        select(OpenAIToken).where(OpenAIToken.agent_id == agent)
    )
    row = result.scalar_one_or_none()

    if row is None:
        row = OpenAIToken(agent_id=agent, created_at=now)
        session.add(row)

    row.access_token = encrypt(token_data["access_token"])
    row.refresh_token = encrypt(token_data.get("refresh_token", ""))
    row.expires_at = expires_at
    row.email = email
    row.updated_at = now

    await session.commit()
    return RedirectResponse("/?oauth=openai_connected")
