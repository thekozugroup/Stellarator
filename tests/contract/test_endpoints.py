"""/api/endpoints: CRUD, test, check, unreachable servers, validation, presets (v2)."""

from __future__ import annotations

import time

import pytest

from shapes import ENDPOINT, ENDPOINT_PRESET, ENDPOINT_TEST, ListOf, check, parse_dt
from support import assert_error, assert_no_content

MOCK_MODELS = [
    "mock-flaky",
    "mock-no-tools",
    "mock-open-think",
    "mock-plain",
    "mock-reasoning",
    "mock-reasoning-content",
    "mock-think-tags",
    "mock-tools",
    "mock-tools-loop",
    "mock-truncating",
]
UNREACHABLE = "http://127.0.0.1:1/v1"
COULD_NOT_CONNECT = "Could not connect. Check the URL and that the server is up."


def test_create_endpoint_checks_the_server(api, mock_url, trash):
    r = api.post("/api/endpoints", json={"name": "  Local mock  ".strip(), "base_url": mock_url + "/"})
    assert r.status_code == 201, r.text
    ep = r.json()
    trash.endpoints.append(ep["id"])
    check(ep, ENDPOINT)
    assert ep["name"] == "Local mock"
    assert ep["base_url"] == mock_url  # the trailing slash is dropped
    assert ep["has_api_key"] is False and ep["api_key_hint"] is None
    assert ep["location"] == "local"
    assert ep["default_concurrency"] == 16
    assert ep["headers"] == {}
    assert ep["models"] == MOCK_MODELS  # sorted, case-insensitively
    assert ep["last_ok"] is True and ep["last_error"] is None
    assert ep["last_checked_at"] is not None
    assert ep["run_count"] == 0


def test_create_with_every_field(api, mock_url, trash):
    r = api.post(
        "/api/endpoints",
        json={
            "name": "Cloud-ish",
            "base_url": f"  {mock_url}/  ",
            "api_key": "sk-test-1234567890abcd",
            "location": "cloud",
            "default_concurrency": 4,
            "headers": {"HTTP-Referer": "https://example.com", "X-Title": "Contract"},
        },
    )
    assert r.status_code == 201, r.text
    ep = r.json()
    trash.endpoints.append(ep["id"])
    check(ep, ENDPOINT)
    assert ep["base_url"] == mock_url
    assert ep["has_api_key"] is True
    assert ep["api_key_hint"] == "…abcd"
    assert "sk-test-1234567890abcd" not in r.text
    assert ep["location"] == "cloud"
    assert ep["default_concurrency"] == 4
    assert ep["headers"] == {"HTTP-Referer": "https://example.com", "X-Title": "Contract"}


def test_short_api_key_hint_shows_nothing_of_the_key(api, mock_url, trash):
    r = api.post("/api/endpoints", json={"name": "Short key", "base_url": mock_url, "api_key": "abc"})
    ep = r.json()
    trash.endpoints.append(ep["id"])
    assert ep["has_api_key"] is True
    assert ep["api_key_hint"] == "…"


def test_empty_api_key_means_none(api, mock_url, trash):
    r = api.post("/api/endpoints", json={"name": "No key", "base_url": mock_url, "api_key": ""})
    ep = r.json()
    trash.endpoints.append(ep["id"])
    assert ep["has_api_key"] is False and ep["api_key_hint"] is None


def test_base_url_without_v1_and_with_chat_path_still_works(api, mock_root, mock_url, trash):
    for url in (mock_root, mock_url + "/chat/completions"):
        r = api.post("/api/endpoints", json={"name": "URL forms", "base_url": url})
        assert r.status_code == 201, r.text
        ep = r.json()
        trash.endpoints.append(ep["id"])
        assert ep["last_ok"] is True, (url, ep["last_error"])
        assert ep["models"] == MOCK_MODELS


def test_get_list_and_order(api, make_endpoint):
    first = make_endpoint(name="Order first")
    time.sleep(0.02)
    second = make_endpoint(name="Order second")

    r = api.get(f"/api/endpoints/{first['id']}")
    assert r.status_code == 200
    got = r.json()
    check(got, ENDPOINT)
    assert got == first

    r = api.get("/api/endpoints")
    assert r.status_code == 200
    items = r.json()
    check(items, ListOf(ENDPOINT))
    ids = [e["id"] for e in items]
    assert first["id"] in ids and second["id"] in ids
    assert ids.index(second["id"]) < ids.index(first["id"]), "newest first"
    assert parse_dt(second["created_at"]) >= parse_dt(first["created_at"])


def test_unknown_endpoint_is_404(api):
    for method, path in (
        ("get", "/api/endpoints/999999"),
        ("patch", "/api/endpoints/999999"),
        ("post", "/api/endpoints/999999/check"),
        ("delete", "/api/endpoints/999999"),
    ):
        r = getattr(api, method)(path, **({"json": {}} if method == "patch" else {}))
        assert_error(r, 404, detail="Endpoint not found")


def test_non_integer_id_is_a_field_error(api):
    assert_error(api.get("/api/endpoints/abc"), 422)


def test_patch_changes_only_what_is_sent(api, make_endpoint):
    ep = make_endpoint(name="Patch me", api_key="sk-secret-value-9999", headers={"X-A": "1"})
    r = api.patch(f"/api/endpoints/{ep['id']}", json={"name": "Renamed"})
    assert r.status_code == 200, r.text
    out = r.json()
    check(out, ENDPOINT)
    assert out["name"] == "Renamed"
    assert out["has_api_key"] is True and out["api_key_hint"] == "…9999"  # key untouched
    assert out["headers"] == {"X-A": "1"}
    assert out["id"] == ep["id"] and out["created_at"] == ep["created_at"]

    r = api.patch(
        f"/api/endpoints/{ep['id']}",
        json={"location": "cloud", "default_concurrency": 8, "headers": {"X-B": "2"}},
    )
    out = r.json()
    assert out["location"] == "cloud" and out["default_concurrency"] == 8
    assert out["headers"] == {"X-B": "2"}  # replaced, not merged
    assert out["name"] == "Renamed"

    # An empty patch is fine.
    assert api.patch(f"/api/endpoints/{ep['id']}", json={}).status_code == 200


def test_patch_api_key_set_and_remove(api, make_endpoint):
    ep = make_endpoint(name="Key patch")
    out = api.patch(f"/api/endpoints/{ep['id']}", json={"api_key": "sk-new-key-abcdefgh1234"}).json()
    assert out["has_api_key"] is True and out["api_key_hint"] == "…1234"
    out = api.patch(f"/api/endpoints/{ep['id']}", json={"api_key": ""}).json()  # "" removes it
    assert out["has_api_key"] is False and out["api_key_hint"] is None
    out = api.patch(f"/api/endpoints/{ep['id']}", json={"api_key": "sk-again-abcdefgh5678"}).json()
    out = api.patch(f"/api/endpoints/{ep['id']}", json={"api_key": None}).json()  # null leaves it
    assert out["has_api_key"] is True and out["api_key_hint"] == "…5678"


def test_patch_base_url_rechecks_the_server(api, make_endpoint, mock_url):
    ep = make_endpoint(name="Move me")
    out = api.patch(f"/api/endpoints/{ep['id']}", json={"base_url": UNREACHABLE + "/"}).json()
    assert out["base_url"] == UNREACHABLE
    assert out["last_ok"] is False
    assert out["last_error"] == COULD_NOT_CONNECT
    out = api.patch(f"/api/endpoints/{ep['id']}", json={"base_url": mock_url}).json()
    assert out["last_ok"] is True and out["last_error"] is None
    assert out["models"] == MOCK_MODELS


def test_create_validation_errors(api):
    r = api.post("/api/endpoints", json={})
    assert_error(r, 422, any_field=("name", "base_url"))

    r = api.post("/api/endpoints", json={"name": "x", "base_url": "localhost:8000"})
    assert_error(r, 422, field="base_url", detail="Base URL must start with http:// or https://")

    r = api.post("/api/endpoints", json={"name": "", "base_url": "http://x"})
    assert_error(r, 422, field="name")

    r = api.post("/api/endpoints", json={"name": "n" * 201, "base_url": "http://x"})
    assert_error(r, 422, field="name")

    for bad in (0, 4097):
        r = api.post("/api/endpoints", json={"name": "x", "base_url": "http://x", "default_concurrency": bad})
        assert_error(r, 422, field="default_concurrency")

    r = api.post("/api/endpoints", json={"name": "x", "base_url": "http://x", "location": "moon"})
    assert_error(r, 422, field="location")

    r = api.post("/api/endpoints", json={"name": "x", "base_url": "http://x", "headers": {"A": 1}})
    assert_error(r, 422)


def test_patch_validation_errors(api, make_endpoint):
    ep = make_endpoint(name="Validate patch")
    url = f"/api/endpoints/{ep['id']}"
    assert_error(api.patch(url, json={"base_url": "ftp://nope"}), 422, field="base_url")
    assert_error(api.patch(url, json={"name": ""}), 422, field="name")
    assert_error(api.patch(url, json={"default_concurrency": 0}), 422, field="default_concurrency")
    assert_error(api.patch(url, json={"location": "elsewhere"}), 422, field="location")
    # Nothing changed.
    assert api.get(url).json()["name"] == "Validate patch"


def test_malformed_json_body_is_422(api):
    r = api.post("/api/endpoints", content=b"{not json", headers={"content-type": "application/json"})
    assert_error(r, 422)


def test_unreachable_server_is_recorded_not_refused(api, trash):
    r = api.post("/api/endpoints", json={"name": "Nobody home", "base_url": UNREACHABLE})
    assert r.status_code == 201, r.text
    ep = r.json()
    trash.endpoints.append(ep["id"])
    check(ep, ENDPOINT)
    assert ep["last_ok"] is False
    assert ep["last_error"] == COULD_NOT_CONNECT
    assert ep["last_checked_at"] is not None
    assert ep["models"] == []


def test_check_updates_the_status(api, make_endpoint):
    ep = make_endpoint(name="Check me")
    time.sleep(0.05)
    r = api.post(f"/api/endpoints/{ep['id']}/check")
    assert r.status_code == 200, r.text
    out = r.json()
    check(out, ENDPOINT)
    assert out["last_ok"] is True and out["models"] == MOCK_MODELS
    assert parse_dt(out["last_checked_at"]) > parse_dt(ep["last_checked_at"])


def test_check_of_a_dead_endpoint(api, make_endpoint):
    ep = make_endpoint(UNREACHABLE, name="Dead")
    out = api.post(f"/api/endpoints/{ep['id']}/check").json()
    assert out["last_ok"] is False
    assert out["last_error"] == COULD_NOT_CONNECT


def test_endpoint_test_ok(api, mock_url):
    r = api.post("/api/endpoints/test", json={"base_url": mock_url + "/"})
    assert r.status_code == 200, r.text
    out = r.json()
    check(out, ENDPOINT_TEST)
    assert out["ok"] is True and out["error"] is None
    assert out["models"] == MOCK_MODELS
    assert out["latency_ms"] >= 0


def test_endpoint_test_unreachable(api):
    r = api.post("/api/endpoints/test", json={"base_url": UNREACHABLE})
    assert r.status_code == 200
    out = r.json()
    check(out, ENDPOINT_TEST)
    assert out == {"ok": False, "models": [], "latency_ms": None, "error": COULD_NOT_CONNECT}


def test_endpoint_test_wrong_path_says_404(api, mock_root):
    r = api.post("/api/endpoints/test", json={"base_url": mock_root + "/nope"})
    out = r.json()
    check(out, ENDPOINT_TEST)
    assert out["ok"] is False
    assert out["error"] == "Nothing found at this URL (404). Does the base URL end in /v1?"


def test_endpoint_test_with_saved_key_and_unknown_id(api, make_endpoint, mock_url):
    ep = make_endpoint(name="Has key", api_key="sk-saved-key-0000")
    for endpoint_id in (ep["id"], 999999):  # an unknown id is ignored, not an error
        r = api.post(
            "/api/endpoints/test",
            json={"base_url": mock_url, "endpoint_id": endpoint_id, "headers": {"X-Test": "1"}},
        )
        assert r.status_code == 200
        check(r.json(), ENDPOINT_TEST)
        assert r.json()["ok"] is True


def test_endpoint_test_validation(api):
    assert_error(api.post("/api/endpoints/test", json={}), 422, field="base_url")
    # A URL that is not http(s) is refused as a field error, or reported as a failed test (the Python
    # backend does the latter; its message is httpx's, so only the shape is checked).
    r = api.post("/api/endpoints/test", json={"base_url": "ftp://x"})
    if r.status_code == 422:
        assert_error(r, 422, field="base_url")
    else:
        assert r.status_code == 200
        out = r.json()
        check(out, ENDPOINT_TEST)
        assert out["ok"] is False and out["error"]


def test_delete_endpoint(api, mock_url):
    ep = api.post("/api/endpoints", json={"name": "Delete me", "base_url": mock_url}).json()
    assert_no_content(api.delete(f"/api/endpoints/{ep['id']}"))
    assert_error(api.get(f"/api/endpoints/{ep['id']}"), 404, detail="Endpoint not found")
    assert_error(api.delete(f"/api/endpoints/{ep['id']}"), 404)
    assert ep["id"] not in [e["id"] for e in api.get("/api/endpoints").json()]


def test_run_count_on_the_world_endpoint(api, world):
    ep = api.get(f"/api/endpoints/{world.endpoint['id']}").json()
    assert ep["run_count"] == world.runs_on_endpoint
    listed = next(e for e in api.get("/api/endpoints").json() if e["id"] == ep["id"])
    assert listed["run_count"] == world.runs_on_endpoint


# --------------------------------------------------------------------------- v2


@pytest.mark.v2
def test_endpoint_presets(api):
    r = api.get("/api/endpoint-presets")
    assert r.status_code == 200
    presets = r.json()
    check(presets, ListOf(ENDPOINT_PRESET))
    by_id = {p["id"]: p for p in presets}
    assert len(by_id) == len(presets), "ids are unique"
    assert {"openrouter", "lmstudio", "ollama", "vllm", "sglang", "llamacpp"} <= set(by_id)

    openrouter = by_id["openrouter"]
    assert openrouter["base_url"] == "https://openrouter.ai/api/v1"
    assert openrouter["location"] == "cloud" and openrouter["needs_api_key"] is True
    assert openrouter["api_key_url"].startswith("https://")

    ports = {"lmstudio": 1234, "ollama": 11434, "vllm": 8000, "sglang": 30000, "llamacpp": 8080}
    for pid, port in ports.items():
        p = by_id[pid]
        assert p["location"] == "local" and p["needs_api_key"] is False
        assert p["base_url"].endswith(f":{port}/v1"), p["base_url"]
        assert p["base_url"].split("://")[1].split(":")[0] in ("localhost", "host.docker.internal")
    for p in presets:
        assert p["default_concurrency"] >= 1
        assert p["base_url"].startswith(("http://", "https://")) and not p["base_url"].endswith("/")
        if p["needs_api_key"]:
            assert p["api_key_url"]
