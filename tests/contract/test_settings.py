"""GET /api/health, GET and PUT /api/settings."""

from __future__ import annotations

from pathlib import Path

import pytest

from shapes import HEALTH, SETTINGS, check
from support import assert_error


def test_health(api):
    r = api.get("/api/health")
    assert r.status_code == 200
    body = r.json()
    check(body, HEALTH)
    assert body["ok"] is True
    assert body["version"]


def test_settings_shape(api, hf_token_cleared, servers):
    r = api.get("/api/settings")
    assert r.status_code == 200
    body = r.json()
    check(body, SETTINGS)
    assert body["hf_token_set"] is False
    assert body["hf_token_hint"] is None
    assert body["version"]
    if servers.data_dir is not None:
        assert Path(body["data_dir"]).resolve() == Path(servers.data_dir).resolve()


def test_settings_token_roundtrip(api, hf_token_cleared):
    r = api.put("/api/settings", json={"hf_token": "hf_abcdefghijkl"})
    assert r.status_code == 200
    body = r.json()
    check(body, SETTINGS)
    assert body["hf_token_set"] is True
    assert body["hf_token_hint"] == "…ijkl"
    assert "abcdefgh" not in r.text  # the token is never sent back

    # Persisted: a fresh GET sees it.
    assert api.get("/api/settings").json()["hf_token_hint"] == "…ijkl"

    # Omitting the field leaves it alone.
    r = api.put("/api/settings", json={})
    assert r.status_code == 200
    assert r.json()["hf_token_set"] is True

    # Whitespace is stripped.
    r = api.put("/api/settings", json={"hf_token": "  hf_zyxwvutsrq  "})
    assert r.json()["hf_token_hint"] == "…tsrq"

    # An empty string removes it.
    r = api.put("/api/settings", json={"hf_token": ""})
    assert r.status_code == 200
    assert r.json()["hf_token_set"] is False
    assert r.json()["hf_token_hint"] is None


def test_settings_short_token_hint_hides_everything(api, hf_token_cleared):
    r = api.put("/api/settings", json={"hf_token": "short"})
    assert r.json()["hf_token_set"] is True
    assert r.json()["hf_token_hint"] == "…"


def test_settings_blank_token_removes_it(api, hf_token_cleared):
    api.put("/api/settings", json={"hf_token": "hf_abcdefghijkl"})
    r = api.put("/api/settings", json={"hf_token": "   "})
    assert r.status_code == 200
    assert r.json()["hf_token_set"] is False


def test_settings_wrong_type_is_a_field_error(api):
    r = api.put("/api/settings", json={"hf_token": 123})
    assert_error(r, 422, field="hf_token")


@pytest.mark.v2
def test_settings_mode(api):
    body = api.get("/api/settings").json()
    check(body, SETTINGS.require())
    assert body["mode"] in ("docker", "desktop", "native")
