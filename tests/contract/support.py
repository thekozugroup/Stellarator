"""Helpers shared by the contract tests: polling, error assertions, imports and run builders."""

from __future__ import annotations

import io
import json
import re
import time
import unicodedata
from typing import Any, Callable

import httpx

from shapes import (
    ENDPOINT,
    PROMPT_SET,
    RUN,
    UPLOAD,
    check,
)


# --------------------------------------------------------------------------- polling


def wait_until(
    fn: Callable[[], Any], *, timeout: float = 60.0, interval: float = 0.2, what: str = "condition"
) -> Any:
    """Call `fn` until it returns something truthy; return that. Fail with `what` after `timeout`."""
    deadline = time.monotonic() + timeout
    last = None
    while True:
        last = fn()
        if last:
            return last
        if time.monotonic() > deadline:
            raise AssertionError(f"Timed out after {timeout:.0f}s waiting for {what}. Last value: {last!r}")
        time.sleep(interval)


# --------------------------------------------------------------------------- errors


def assert_error(
    resp: httpx.Response,
    status: int,
    *,
    field: str | None = None,
    fields: tuple[str, ...] | list[str] = (),
    any_field: tuple[str, ...] | list[str] = (),
    detail: str | None = None,
    contains: str | None = None,
) -> dict:
    """The response is an error body: {"detail": str} plus {"fields": {name: str}} on field errors.

    field: a key that must be in `fields` (exact name). fields: several. any_field: at least one of
    these (a body missing several required fields may report only the first). detail: exact
    sentence. contains: a substring of the sentence.
    """
    assert resp.status_code == status, f"expected {status}, got {resp.status_code}: {resp.text[:300]}"
    assert resp.headers.get("content-type", "").startswith("application/json"), resp.headers.get("content-type")
    body = resp.json()
    assert isinstance(body, dict), body
    assert set(body) <= {"detail", "fields"}, f"unexpected keys in error body: {sorted(body)}"
    assert isinstance(body.get("detail"), str) and body["detail"], f"detail must be a sentence: {body}"
    flds = body.get("fields", {})
    assert isinstance(flds, dict) and all(isinstance(k, str) and isinstance(v, str) for k, v in flds.items()), body
    wanted = ([field] if field else []) + list(fields)
    for name in wanted:
        assert name in flds, f"expected field error for {name!r}, got fields {flds}"
    if any_field:
        assert set(any_field) & set(flds), f"expected a field error for one of {list(any_field)}, got {flds}"
    if flds:
        assert body["detail"] in flds.values(), f"detail should be one of the field messages: {body}"
    if detail is not None:
        assert body["detail"] == detail, f"detail {body['detail']!r} != {detail!r}"
    if contains is not None:
        assert contains in body["detail"], f"{contains!r} not in {body['detail']!r}"
    return body


def assert_no_content(resp: httpx.Response) -> None:
    assert resp.status_code == 204, f"expected 204, got {resp.status_code}: {resp.text[:300]}"
    assert resp.content == b""


# --------------------------------------------------------------------------- misc data


def slugify(name: str) -> str:
    """The export filename slug (same rules as the Python backend)."""
    text = unicodedata.normalize("NFKD", name).encode("ascii", "ignore").decode()
    text = re.sub(r"[^a-z0-9._]+", "-", text.lower()).strip("-._")
    return text[:80] or "export"


def jsonl(rows: list[Any]) -> bytes:
    return ("\n".join(json.dumps(r, ensure_ascii=False) for r in rows) + "\n").encode()


def main_rows(n: int = 30) -> list[dict]:
    """Chat-style rows with a system message and one scalar column that becomes prompt meta."""
    rows = []
    for i in range(n):
        rows.append(
            {
                "messages": [
                    {"role": "system", "content": "You are a careful math tutor."},
                    {"role": "user", "content": f"Task tag-{i:03d}: compute {3 + i} plus {40 - i}."},
                ],
                "category": "even" if i % 2 == 0 else "odd",
            }
        )
    return rows


def lines_text(n: int, prefix: str = "Question") -> str:
    return "\n".join(f"{prefix} number {i:03d} about topic {i * 7 % 13}" for i in range(n))


# --------------------------------------------------------------------------- endpoints and uploads


def create_endpoint(api: httpx.Client, base_url: str, **kw: Any) -> dict:
    body = {"name": kw.pop("name", f"Mock {time.time_ns()}"), "base_url": base_url, **kw}
    r = api.post("/api/endpoints", json=body)
    assert r.status_code == 201, r.text
    ep = r.json()
    check(ep, ENDPOINT)
    return ep


def upload(api: httpx.Client, filename: str, content: bytes | str, mime: str = "application/octet-stream") -> dict:
    data = content.encode() if isinstance(content, str) else content
    r = api.post("/api/imports/upload", files={"file": (filename, io.BytesIO(data), mime)})
    assert r.status_code == 200, r.text
    out = r.json()
    check(out, UPLOAD)
    return out


# --------------------------------------------------------------------------- imports


def source_paste(text: str) -> dict:
    return {"kind": "paste", "text": text}


def source_file(upload_id: str, fmt: str = "auto") -> dict:
    return {"kind": "file", "upload_id": upload_id, "format": fmt}


def wait_prompt_set_ready(api: httpx.Client, set_id: int, timeout: float = 90.0) -> dict:
    """Poll GET /api/prompt-sets/{id} until the import is no longer running.

    The Python backend has no status field and the set is complete when it first exists, so a
    response without `status` counts as ready.
    """

    def poll() -> dict | None:
        r = api.get(f"/api/prompt-sets/{set_id}")
        assert r.status_code == 200, r.text
        ps = r.json()
        return ps if ps.get("status", "ready") != "importing" else None

    return wait_until(poll, timeout=timeout, interval=0.2, what=f"prompt set {set_id} to finish importing")


def import_prompts(
    api: httpx.Client, source: dict, name: str, mapping: dict | None = None, timeout: float = 90.0
) -> dict:
    """Commit an import on either backend and return the finished prompt set.

    Python answers 201 with the finished set; the v2 backend answers 202 and finishes in the
    background. Both are accepted here; tests of the exact status code are marked v1 or v2.
    """
    body: dict[str, Any] = {"source": source, "name": name}
    if mapping is not None:
        body["mapping"] = mapping
    r = api.post("/api/imports", json=body)
    assert r.status_code in (201, 202), f"import failed: {r.status_code} {r.text[:300]}"
    ps = r.json()
    check(ps, PROMPT_SET)
    if r.status_code == 202 or ps.get("status") == "importing":
        ps = wait_prompt_set_ready(api, ps["id"], timeout)
    assert ps.get("status", "ready") == "ready", f"import did not finish: {ps}"
    check(ps, PROMPT_SET)
    return ps


def import_outcome(api: httpx.Client, body: dict, timeout: float = 60.0) -> tuple[int, dict]:
    """Commit an import that is expected to fail, on either backend.

    Returns (422, error body) when the request is refused up front, or (200, prompt set) with
    status "failed" when the v2 backend fails it in the background.
    """
    r = api.post("/api/imports", json=body)
    if r.status_code == 422:
        return 422, r.json()
    assert r.status_code in (201, 202), f"unexpected {r.status_code}: {r.text[:300]}"
    ps = wait_prompt_set_ready(api, r.json()["id"], timeout)
    return 200, ps


def all_prompts(api: httpx.Client, set_id: int) -> list[dict]:
    out: list[dict] = []
    offset = 0
    while True:
        r = api.get(f"/api/prompt-sets/{set_id}/prompts", params={"offset": offset, "limit": 500})
        assert r.status_code == 200, r.text
        page = r.json()
        out.extend(page["items"])
        offset += len(page["items"])
        if not page["items"] or offset >= page["total"]:
            return out


# --------------------------------------------------------------------------- runs

TERMINAL = ("completed",)


def create_run(
    api: httpx.Client,
    endpoint_id: int,
    prompt_set_id: int,
    model: str,
    *,
    name: str | None = None,
    concurrency: int | None = None,
    **params: Any,
) -> dict:
    body: dict[str, Any] = {"endpoint_id": endpoint_id, "prompt_set_id": prompt_set_id, "model": model}
    if name is not None:
        body["name"] = name
    if concurrency is not None:
        body["concurrency"] = concurrency
    if params:
        body["params"] = params
    r = api.post("/api/runs", json=body)
    assert r.status_code == 201, r.text
    run = r.json()
    check(run, RUN)
    return run


def get_run(api: httpx.Client, run_id: int) -> dict:
    r = api.get(f"/api/runs/{run_id}")
    assert r.status_code == 200, r.text
    run = r.json()
    check(run, RUN)
    return run


def wait_run(api: httpx.Client, run_id: int, status: str = "completed", timeout: float = 90.0) -> dict:
    def poll() -> dict | None:
        run = get_run(api, run_id)
        return run if run["status"] == status else None

    return wait_until(poll, timeout=timeout, interval=0.2, what=f"run {run_id} to be {status}")


def list_samples(api: httpx.Client, run_id: int, **params: Any) -> list[dict]:
    """Every sample summary of a run that matches the filters."""
    out: list[dict] = []
    offset = 0
    while True:
        r = api.get(f"/api/runs/{run_id}/samples", params={**params, "offset": offset, "limit": 500})
        assert r.status_code == 200, r.text
        page = r.json()
        out.extend(page["items"])
        offset += len(page["items"])
        if not page["items"] or offset >= page["total"]:
            return out
