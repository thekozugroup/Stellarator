from __future__ import annotations

import hmac
import logging

from pydantic_settings import BaseSettings, SettingsConfigDict

logger = logging.getLogger(__name__)


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore")

    tinker_api_key: str = ""
    tinker_base_url: str = "https://api.tinker.thinkingmachines.ai/v1"

    stellarator_db_url: str = "sqlite+aiosqlite:////data/stellarator.db"
    stellarator_secret: str = "change-me"

    agent_token_claude_code: str = ""
    agent_token_openai: str = ""
    agent_token_codex: str = ""

    openai_api_key: str = ""
    codex_oauth_client_id: str = ""
    codex_oauth_client_secret: str = ""
    codex_oauth_redirect_uri: str = ""

    # --- OpenAI browser OAuth (Sign in with OpenAI) ----------------------
    # See app/agents/oauth_openai.py module docstring for endpoint references.
    openai_oauth_client_id: str = ""
    openai_oauth_client_secret: str = ""
    openai_oauth_auth_url: str = "https://auth.openai.com/oauth/authorize"
    openai_oauth_token_url: str = "https://auth.openai.com/oauth/token"
    openai_oauth_redirect_uri: str = ""
    openai_oauth_scopes: str = "openid email offline_access"

    supervisor_shared_secret: str = ""

    # OpenRouter etiquette headers — sent on every request so the router can
    # attribute traffic and surface your app in its analytics dashboard.
    openrouter_referer: str = "https://stellarator.dev"
    openrouter_title: str = "Stellarator"

    stellarator_cors_origins: str = "http://localhost:3000"

    cost_h100_usd_per_hr: float = 4.50
    cost_a100_usd_per_hr: float = 2.20

    # GitHub Code Search + contents (research sub-agent). Optional; unauth has
    # a low rate limit. Personal access token (classic, scope: public_repo) works.
    github_token: str = ""

    # Internal base URL for tool dispatch (research sub-agent reuses /v1).
    stellarator_internal_base_url: str = "http://localhost:8000"

    def agent_for_token(self, token: str) -> str | None:
        """Constant-time token comparison.

        Iterates every (configured_token, agent) pair and uses
        ``hmac.compare_digest`` so the comparison time does not leak which
        token (if any) matched. Empty configured tokens are skipped so an
        empty bearer never matches.
        """
        if not token:
            return None
        token_bytes = token.encode("utf-8")
        candidates: tuple[tuple[str, str], ...] = (
            (self.agent_token_claude_code, "claude-code"),
            (self.agent_token_openai, "openai"),
            (self.agent_token_codex, "codex"),
        )
        match: str | None = None
        for configured, agent in candidates:
            if not configured:
                continue
            if hmac.compare_digest(configured.encode("utf-8"), token_bytes):
                match = agent
        return match

    def configured_secrets(self) -> list[str]:
        """Return a snapshot of all secret-like values currently configured.

        Used by the log-redaction filter. Empty values are skipped.
        """
        values = [
            self.agent_token_claude_code,
            self.agent_token_openai,
            self.agent_token_codex,
            self.openai_api_key,
            self.codex_oauth_client_secret,
            self.openai_oauth_client_secret,
            self.tinker_api_key,
            self.supervisor_shared_secret,
        ]
        return [v for v in values if v]


settings = Settings()


def warn_if_no_agent_tokens() -> None:
    if not any(
        (
            settings.agent_token_claude_code,
            settings.agent_token_openai,
            settings.agent_token_codex,
        )
    ):
        logger.warning(
            "No AGENT_TOKEN_* configured; all bearer-protected endpoints will reject requests"
        )
