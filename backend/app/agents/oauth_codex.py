"""Codex OAuth 2.0 authorization-code + PKCE flow.

Endpoints:
  GET  /v1/oauth/codex/start     - redirect to Codex authorization URL
  GET  /v1/oauth/codex/callback  - exchange code, persist tokens, redirect to /

Hardening:
  - State payload {agent, nonce, iat, exp} HMAC-SHA256 signed with
    ``stellarator_secret``. Replay rejected via in-process LRU set of seen
    nonces (size 1024). Expiration enforced strictly.
  - PKCE (S256) - ``code_verifier`` is generated per /start and bound to the
    state nonce so the same browser session that started the flow must
    complete it.
  - The ``current_agent`` calling /start is recorded; /callback refuses to
    proceed unless the same agent is authenticated.
  - ``redirect_uri`` is read from ``CODEX_OAUTH_REDIRECT_URI`` env, falling
    back to a runtime-resolved URL via ``request.url_for``.
  - Token values are never logged.
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import json
import secrets
import time
from datetime import datetime, timedelta, timezone
from urllib.parse import urlencode

import httpx
from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import RedirectResponse
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.core.auth import CurrentAgent
from app.core.config import settings
from app.core.db import get_session
from app.models.chat import CodexToken
from app.models.oauth_state import OAuthState
from app.services.crypto import encrypt

try:
    from app.core.rate_limit import limiter as _limiter
    _rate_limit_available = True
except ImportError:
    _rate_limit_available = False

from app.core.rate_limit import maybe_limit
_start_limit = maybe_limit("10/minute")

router = APIRouter(prefix="/v1/oauth/codex", tags=["oauth"])

_CODEX_AUTH_URL = "https://auth.openai.com/oauth/authorize"
_CODEX_TOKEN_URL = "https://auth.openai.com/oauth/token"
_SCOPE = "openid profile email offline_access"
_PROVIDER = "codex"
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
    if settings.codex_oauth_redirect_uri:
        return settings.codex_oauth_redirect_uri
    return str(request.url_for("oauth_callback"))


def _pkce_pair() -> tuple[str, str]:
    verifier = base64.urlsafe_b64encode(secrets.token_bytes(64)).decode("ascii").rstrip("=")
    challenge = (
        base64.urlsafe_b64encode(hashlib.sha256(verifier.encode("ascii")).digest())
        .decode("ascii")
        .rstrip("=")
    )
    return verifier, challenge


@router.get("/start", name="oauth_start")
@_start_limit
async def oauth_start(
    request: Request,
    agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
):
    """Redirect the agent's browser to the Codex authorization URL."""
    if not settings.codex_oauth_client_id:
        raise HTTPException(503, "Codex OAuth not configured")

    nonce = secrets.token_hex(16)
    iat = int(time.time())
    verifier, challenge = _pkce_pair()
    await _db_remember_pending(nonce, agent, verifier, session)
    await session.commit()

    state = _sign_state({"agent": agent, "nonce": nonce, "iat": iat, "exp": iat + _STATE_TTL_SECS})

    redirect_uri = _resolve_redirect_uri(request)
    params = {
        "response_type": "code",
        "client_id": settings.codex_oauth_client_id,
        "redirect_uri": redirect_uri,
        "scope": _SCOPE,
        "state": state,
        "code_challenge": challenge,
        "code_challenge_method": "S256",
    }
    return RedirectResponse(f"{_CODEX_AUTH_URL}?{urlencode(params)}")


@router.get("/callback", name="oauth_callback")
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
            _CODEX_TOKEN_URL,
            data={
                "grant_type": "authorization_code",
                "code": code,
                "redirect_uri": redirect_uri,
                "client_id": settings.codex_oauth_client_id,
                "client_secret": settings.codex_oauth_client_secret,
                "code_verifier": pending_row.code_verifier,
            },
            headers={"Content-Type": "application/x-www-form-urlencoded"},
        )
        if r.status_code >= 400:
            raise HTTPException(502, "Codex token exchange failed")
        token_data = r.json()

    now = datetime.now(tz=timezone.utc)
    expires_at = (
        now + timedelta(seconds=int(token_data["expires_in"]))
        if token_data.get("expires_in")
        else None
    )

    result = await session.execute(
        select(CodexToken).where(CodexToken.agent_id == agent)
    )
    row = result.scalar_one_or_none()

    if row is None:
        row = CodexToken(agent_id=agent)
        session.add(row)

    # Encrypt at rest. Never log these values.
    row.access_token = encrypt(token_data["access_token"])
    row.refresh_token = encrypt(token_data.get("refresh_token", ""))
    row.expires_at = expires_at
    row.updated_at = now

    await session.commit()
    return RedirectResponse("/?oauth=codex_connected")
