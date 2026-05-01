"""Tests: reconcile exponential backoff."""

from __future__ import annotations

import pytest

from app.services.reconcile import _backoff, _BACKOFF_CAP


def test_backoff_initial_no_failures() -> None:
    assert _backoff(30.0, 0) == 30.0


def test_backoff_grows_exponentially() -> None:
    assert _backoff(30.0, 1) == 60.0
    assert _backoff(30.0, 2) == 120.0
    assert _backoff(30.0, 3) == 240.0


def test_backoff_capped_at_300() -> None:
    assert _backoff(30.0, 4) == 300.0
    assert _backoff(30.0, 10) == 300.0
    assert _backoff(30.0, 4) <= _BACKOFF_CAP


def test_backoff_cap_constant() -> None:
    assert _BACKOFF_CAP == 300.0


@pytest.mark.asyncio
async def test_reconciliation_loop_resets_on_success(monkeypatch: pytest.MonkeyPatch) -> None:
    """Supervisor failure counter grows then resets on success."""
    import asyncio
    from app.services import reconcile as rec_mod

    call_count = 0
    slept: list[float] = []

    async def fake_reconcile_once(
        supervisor_token: str | None,
        tinker_failures: dict[str, int],
        base_interval: float,
    ) -> None:
        nonlocal call_count
        call_count += 1
        if call_count <= 2:
            raise RuntimeError("simulated supervisor failure")
        # success on 3rd call — loop will sleep base_interval next

    async def fake_sleep(secs: float) -> None:
        slept.append(secs)
        if len(slept) >= 3:
            raise asyncio.CancelledError

    monkeypatch.setattr(rec_mod, "_reconcile_once", fake_reconcile_once)
    monkeypatch.setattr(asyncio, "sleep", fake_sleep)

    # Patch settings to avoid import side-effects
    class _FakeSettings:
        supervisor_shared_secret = ""

    monkeypatch.setattr(rec_mod, "_read_interval", lambda: 30.0)

    import importlib, types

    fake_cfg_mod = types.ModuleType("app.core.config")
    fake_cfg_mod.settings = _FakeSettings()  # type: ignore[attr-defined]
    monkeypatch.setitem(__import__("sys").modules, "app.core.config", fake_cfg_mod)

    with pytest.raises(asyncio.CancelledError):
        await rec_mod.reconciliation_loop()

    # After 2 failures: sleeps should be 60, 120 (backoff). After success: 30.
    assert slept[0] == 60.0, f"Expected 60s after 1st failure, got {slept[0]}"
    assert slept[1] == 120.0, f"Expected 120s after 2nd failure, got {slept[1]}"
    assert slept[2] == 30.0, f"Expected base interval after success, got {slept[2]}"
