"""Stellarator FastAPI application."""

from __future__ import annotations

import logging
from contextlib import asynccontextmanager
from typing import AsyncIterator

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from app.core.config import settings, warn_if_no_agent_tokens
from app.core.db import init_db
from app.core.logging_filter import install_redaction_filter
from app.api.alerts import router as alerts_router
from app.api.notifications import router as notifications_router
from app.api.runs import router as runs_router
from app.api.agents import router as agents_router
from app.api.chat import router as chat_router
from app.agents.oauth_codex import router as oauth_codex_router
from app.agents.oauth_openai import router as oauth_openai_router

# Try to import cost router, skip gracefully if concurrent edits conflict
try:
    from app.api.cost import router as cost_router
    _cost_router_available = True
except (ImportError, SyntaxError):
    _cost_router_available = False

# Try to import integrations router, skip gracefully if concurrent edits conflict
try:
    from app.api.integrations import router as integrations_router
    _integrations_router_available = True
except (ImportError, SyntaxError):
    _integrations_router_available = False

# Distributed tracing via OpenTelemetry — optional; skipped gracefully if not installed.
try:
    from app.core.tracing import init_tracing as _init_tracing
    _init_tracing()
except ImportError:
    pass

# Rate limiting via SlowAPI — optional; skipped gracefully if not installed.
try:
    from slowapi import _rate_limit_exceeded_handler
    from slowapi.errors import RateLimitExceeded
    from slowapi.middleware import SlowAPIMiddleware
    from app.core.rate_limit import limiter as _limiter
    _slowapi_available = True
except ImportError:
    _slowapi_available = False


# ---------------------------------------------------------------------------
# Logging — install secret-redaction filter BEFORE any module-level loggers
# emit records that could contain bearer tokens.
# ---------------------------------------------------------------------------
logging.basicConfig(level=logging.INFO)
install_redaction_filter(settings.configured_secrets)


_FORBIDDEN_SECRETS = {"", "change-me", "change-me-32-bytes"}


def _validate_boot_secret() -> None:
    secret = settings.stellarator_secret
    if secret in _FORBIDDEN_SECRETS:
        raise RuntimeError(
            "STELLARATOR_SECRET is unset or set to a default placeholder; refusing to boot"
        )
    if len(secret.encode("utf-8")) < 32:
        raise RuntimeError("STELLARATOR_SECRET must be at least 32 bytes long")


def _cors_origins() -> list[str]:
    raw = (settings.stellarator_cors_origins or "").strip()
    if raw == "*":
        logging.getLogger(__name__).warning(
            "STELLARATOR_CORS_ORIGINS=* — wide-open CORS is unsafe outside local dev"
        )
        return ["*"]
    if not raw:
        return ["http://localhost:3000"]
    return [o.strip() for o in raw.split(",") if o.strip()]


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    _validate_boot_secret()
    warn_if_no_agent_tokens()
    await init_db()
    yield


app = FastAPI(
    title="Stellarator",
    description="Fine-tuning run management API",
    version="0.1.0",
    docs_url="/docs",
    redoc_url="/redoc",
    lifespan=lifespan,
)

if _slowapi_available:
    app.state.limiter = _limiter
    app.add_exception_handler(RateLimitExceeded, _rate_limit_exceeded_handler)
    app.add_middleware(SlowAPIMiddleware)

# ---------------------------------------------------------------------------
# CORS — env-gated; defaults to localhost:3000.
# ---------------------------------------------------------------------------
app.add_middleware(
    CORSMiddleware,
    allow_origins=_cors_origins(),
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# ---------------------------------------------------------------------------
# Routers
# ---------------------------------------------------------------------------
app.include_router(runs_router, prefix="/v1")
app.include_router(alerts_router, prefix="/v1")
app.include_router(notifications_router, prefix="/v1")
app.include_router(agents_router, prefix="/v1")
app.include_router(chat_router)
app.include_router(oauth_codex_router)
# DEPRECATED: OpenAI OAuth merged into Codex sign-in
# app.include_router(oauth_openai_router)
if _cost_router_available:
    app.include_router(cost_router)
if _integrations_router_available:
    app.include_router(integrations_router, prefix="/v1")


# ---------------------------------------------------------------------------
# Health check
# ---------------------------------------------------------------------------
@app.get("/healthz", tags=["ops"])
async def healthz() -> dict[str, str]:
    return {"status": "ok"}
