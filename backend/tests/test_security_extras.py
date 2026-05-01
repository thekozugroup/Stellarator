"""Extended security tests — agent_for_token and _validate_boot_secret."""

from __future__ import annotations

import pytest

from app.core import config as config_mod


# ---------------------------------------------------------------------------
# 1. All-empty configured tokens → always None
# ---------------------------------------------------------------------------

def test_agent_for_token_all_empty_returns_none():
    s = config_mod.Settings(
        agent_token_claude_code="",
        agent_token_openai="",
        agent_token_codex="",
    )
    assert s.agent_for_token("") is None
    assert s.agent_for_token("anything") is None
    assert s.agent_for_token("a" * 64) is None


# ---------------------------------------------------------------------------
# 2. Known-good token matches; unknown returns None
# ---------------------------------------------------------------------------

def test_agent_for_token_known_good():
    s = config_mod.Settings(
        agent_token_claude_code="valid-token-abc123",
        agent_token_openai="openai-token-xyz789",
        agent_token_codex="",
    )
    assert s.agent_for_token("valid-token-abc123") == "claude-code"
    assert s.agent_for_token("openai-token-xyz789") == "openai"
    assert s.agent_for_token("valid-token-abc123x") is None
    assert s.agent_for_token("wrong") is None


# ---------------------------------------------------------------------------
# 3. Timing-safe smoke — tokens sharing a prefix still iterate all candidates
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("probe", [
    "prefix-aaaaaa",
    "prefix-bbbbbb",
    "prefix-cccccc",
    "prefix-dddddd-extra",
])
def test_agent_for_token_timing_safe_smoke(probe):
    """No early return — all candidates must be evaluated regardless of prefix match."""
    configured = "prefix-aaaaaa"
    s = config_mod.Settings(
        agent_token_claude_code=configured,
        agent_token_openai="prefix-bbbbbb",
        agent_token_codex="prefix-cccccc",
    )
    result = s.agent_for_token(probe)
    # Verify the result is correct — not testing timing, just that iteration is complete
    if probe == "prefix-aaaaaa":
        assert result == "claude-code"
    elif probe == "prefix-bbbbbb":
        assert result == "openai"
    elif probe == "prefix-cccccc":
        assert result == "codex"
    else:
        assert result is None


# ---------------------------------------------------------------------------
# 4. _validate_boot_secret rejects weak values; accepts strong ones
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("bad_secret", [
    "",
    "change-me",
    "change-me-32-bytes",
    "x" * 31,
])
def test_validate_boot_secret_rejects_weak(bad_secret, monkeypatch):
    monkeypatch.setattr(config_mod.settings, "stellarator_secret", bad_secret)
    from app.main import _validate_boot_secret
    with pytest.raises(RuntimeError):
        _validate_boot_secret()


def test_validate_boot_secret_accepts_strong(monkeypatch):
    monkeypatch.setattr(config_mod.settings, "stellarator_secret", "x" * 32)
    from app.main import _validate_boot_secret
    # Should not raise
    _validate_boot_secret()
