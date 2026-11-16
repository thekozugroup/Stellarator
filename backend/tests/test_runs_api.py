"""Runs API: validation, listing, patching, samples browsing, and a 100k-sample run."""

from __future__ import annotations

import asyncio
import socket
import time

from stellarator.services.engine import engine_registry


async def make_endpoint(client, base_url: str, name: str = "Mock") -> int:
    r = await client.post("/api/endpoints", json={"name": name, "base_url": base_url})
    assert r.status_code == 201, r.text
    return r.json()["id"]


def dead_url() -> str:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return f"http://127.0.0.1:{s.getsockname()[1]}/v1"


async def wait_for(client, run_id: int, pred, timeout: float = 60.0, interval: float = 0.05) -> dict:
    deadline = time.monotonic() + timeout
    while True:
        r = await client.get(f"/api/runs/{run_id}")
        assert r.status_code == 200, r.text
        run = r.json()
        if pred(run):
            return run
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out waiting; last state: {run}")
        await asyncio.sleep(interval)


async def test_create_validation(client, make_prompt_set):
    ep = await make_endpoint(client, dead_url(), name="Nowhere")
    ps = await make_prompt_set(3)
    empty = await make_prompt_set(0, name="Empty")

    r = await client.post("/api/runs", json={"endpoint_id": 999999, "model": "m", "prompt_set_id": ps})
    assert r.status_code == 422 and "endpoint" in r.json()["fields"]["endpoint_id"]
    r = await client.post("/api/runs", json={"endpoint_id": ep, "model": "m", "prompt_set_id": 999999})
    assert r.status_code == 422 and "prompt set" in r.json()["fields"]["prompt_set_id"]
    r = await client.post("/api/runs", json={"endpoint_id": ep, "model": "m", "prompt_set_id": empty})
    assert r.status_code == 422 and "no prompts" in r.json()["fields"]["prompt_set_id"]
    r = await client.post("/api/runs", json={"endpoint_id": ep, "model": "m", "prompt_set_id": ps,
                                             "params": {"user_template": "no slot"}})
    assert r.status_code == 422 and "params.user_template" in r.json()["fields"]
    r = await client.post("/api/runs", json={"endpoint_id": ep, "model": "m", "prompt_set_id": ps, "concurrency": 0})
    assert r.status_code == 422 and "concurrency" in r.json()["fields"]


async def test_not_found(client):
    for method, path in [
        ("get", "/api/runs/999999"),
        ("patch", "/api/runs/999999"),
        ("delete", "/api/runs/999999"),
        ("post", "/api/runs/999999/pause"),
        ("post", "/api/runs/999999/resume"),
        ("post", "/api/runs/999999/retry-failed"),
        ("get", "/api/runs/999999/live"),
        ("get", "/api/runs/999999/samples"),
        ("get", "/api/samples/999999"),
    ]:
        kwargs = {"json": {"name": "x"}} if method == "patch" else {}
        r = await getattr(client, method)(path, **kwargs)
        assert r.status_code == 404, (method, path, r.status_code)


async def test_list_patch_and_shape(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(4, name="Four prompts")
    params = {"samples_per_prompt": 2, "temperature": 0.2, "seed": 1, "extra_body": {"top_k": 20}}
    a = (await client.post("/api/runs", json={"endpoint_id": ep, "model": "mock-plain", "prompt_set_id": ps,
                                              "params": params})).json()
    b = (await client.post("/api/runs", json={"name": "Second", "endpoint_id": ep, "model": "mock-plain",
                                              "prompt_set_id": ps, "concurrency": 2})).json()
    assert b["name"] == "Second" and a["name"] == "mock-plain on Four prompts" and b["total"] == 4
    assert a["params"]["extra_body"] == {"top_k": 20} and a["params"]["samples_per_prompt"] == 2
    assert a["endpoint_name"] == "Mock" and a["prompt_set_name"] == "Four prompts" and a["total"] == 8
    for key in ("created_at", "started_at"):
        assert a[key].endswith("Z") or a[key].endswith("+00:00")

    runs = (await client.get("/api/runs")).json()
    ids = [r["id"] for r in runs]
    assert ids.index(b["id"]) < ids.index(a["id"])  # newest first

    r = await client.patch(f"/api/runs/{a['id']}", json={"name": "Renamed", "concurrency": 5})
    assert r.status_code == 200 and r.json()["name"] == "Renamed" and r.json()["concurrency"] == 5
    task = engine_registry.get(a["id"])
    assert task is None or task.concurrency == 5
    r = await client.patch(f"/api/runs/{a['id']}", json={"concurrency": 0})
    assert r.status_code == 422

    for run in (a, b):
        done = await wait_for(client, run["id"], lambda x: x["status"] == "completed")
        assert done["done"] == run["total"] and done["finished_at"] is not None
    r = await client.patch(f"/api/runs/{b['id']}", json={"concurrency": 7})
    assert r.json()["concurrency"] == 7 and r.json()["status"] == "completed"


async def test_samples_filters_and_search(client, mock_server, make_prompt_set):
    long_prompt = "Explain " + "very " * 100 + "carefully."
    texts = ["Alpha 100% sure?", "beta_value test", "Gamma ray burst", long_prompt, "Café crème"]
    ps = await make_prompt_set(
        5, messages=lambda i: [{"role": "system", "content": "sys"}, {"role": "user", "content": texts[i]}]
    )
    ep = await make_endpoint(client, mock_server)
    run = (await client.post("/api/runs", json={"endpoint_id": ep, "model": "mock-truncating",
                                                "prompt_set_id": ps, "params": {"samples_per_prompt": 2}})).json()
    rid = run["id"]
    await wait_for(client, rid, lambda x: x["status"] == "completed")

    async def page(query: str) -> dict:
        r = await client.get(f"/api/runs/{rid}/samples{query}")
        assert r.status_code == 200, r.text
        return r.json()

    p = await page("?limit=3&offset=2")
    assert p["total"] == 10 and p["offset"] == 2 and p["limit"] == 3
    assert [(i["prompt_idx"], i["sample_idx"]) for i in p["items"]] == [(1, 0), (1, 1), (2, 0)]
    assert (await page("?status=done"))["total"] == 10
    assert (await page("?status=truncated"))["total"] == 10
    assert (await page("?status=failed"))["total"] == 0
    assert (await page("?status=pending"))["total"] == 0
    assert (await client.get(f"/api/runs/{rid}/samples?status=bogus")).status_code == 422

    assert (await page("?q=gamma"))["total"] == 2  # case-insensitive, prompt text
    assert (await page("?q=100%25"))["total"] == 2  # a literal %
    assert (await page("?q=beta_value"))["total"] == 2
    assert (await page("?q=_"))["total"] == 2  # a literal _, not a wildcard
    assert (await page("?q=caf%C3%A9"))["total"] == 2  # non-ASCII
    assert (await page("?q=sys"))["total"] == 10  # the system message is part of the prompt
    assert (await page("?q=nothing-like-this"))["total"] == 0

    item = next(i for i in (await page("?q=carefully"))["items"])
    assert len(item["prompt_preview"]) <= 160 and item["prompt_preview"].startswith("Explain very")
    assert item["finish_reason"] == "length" and item["has_reasoning"] is True
    assert item["response_preview"] == ""  # truncated while thinking

    full = (await client.get(f"/api/samples/{item['id']}")).json()
    assert full["messages"] == [{"role": "system", "content": "sys"}, {"role": "user", "content": long_prompt}]
    assert full["completed_at"].endswith("Z") or full["completed_at"].endswith("+00:00")


async def test_search_in_response(client, mock_server, make_prompt_set):
    ps = await make_prompt_set(3)
    ep = await make_endpoint(client, mock_server)
    run = (await client.post("/api/runs", json={"endpoint_id": ep, "model": "mock-plain", "prompt_set_id": ps})).json()
    await wait_for(client, run["id"], lambda x: x["status"] == "completed")
    r = await client.get(f"/api/runs/{run['id']}/samples?q=final%20answer")
    assert r.json()["total"] == 3  # "Final answer: N." is in every response


async def test_large_run_is_created_quickly(client, make_prompt_set):
    ep = await make_endpoint(client, dead_url(), name="Nowhere")
    ps = await make_prompt_set(50_000)
    t0 = time.perf_counter()
    r = await client.post("/api/runs", json={
        "endpoint_id": ep, "model": "m", "prompt_set_id": ps, "concurrency": 1,
        "params": {"samples_per_prompt": 2, "max_attempts": 1, "pause_after_failures": 0},
    })
    elapsed = time.perf_counter() - t0
    assert r.status_code == 201, r.text
    run = r.json()
    assert run["total"] == 100_000
    assert elapsed < 10, f"creating 100,000 samples took {elapsed:.1f} s"
    r = await client.post(f"/api/runs/{run['id']}/pause")
    assert r.status_code == 200
    p = (await client.get(f"/api/runs/{run['id']}/samples?offset=99990&limit=20")).json()
    assert p["total"] == 100_000 and len(p["items"]) == 10
    assert (p["items"][-1]["prompt_idx"], p["items"][-1]["sample_idx"]) == (49_999, 1)
    assert (await client.delete(f"/api/runs/{run['id']}")).status_code == 204


async def test_live_metrics_are_downsampled(client, make_prompt_set):
    from datetime import UTC, datetime, timedelta

    from stellarator.db import SessionLocal
    from stellarator.models import RunMetric

    ep = await make_endpoint(client, dead_url(), name="Nowhere")
    ps = await make_prompt_set(5)
    run = (await client.post("/api/runs", json={"endpoint_id": ep, "model": "m", "prompt_set_id": ps,
                                                "params": {"max_attempts": 1, "pause_after_failures": 1}})).json()
    await wait_for(client, run["id"], lambda x: x["status"] == "paused")
    t0 = datetime.now(UTC) + timedelta(minutes=1)  # after the points the engine wrote
    async with SessionLocal() as s:
        await s.execute(RunMetric.__table__.insert(), [
            {"run_id": run["id"], "t": t0 + timedelta(seconds=5 * i), "tokens_per_s": float(i),
             "samples_per_min": 1.0, "active": 1, "concurrency": 16, "done": i, "failed": 0}
            for i in range(1000)
        ])
        await s.commit()
    lv = (await client.get(f"/api/runs/{run['id']}/live")).json()
    assert lv["in_flight"] == 0 and lv["sequences"] == [] and lv["eta_seconds"] is None
    points = lv["metrics"]
    assert 200 <= len(points) <= 240
    times = [p["t"] for p in points]
    assert times == sorted(times)
    assert points[-1]["done"] == 999  # the latest point is always kept
    await client.delete(f"/api/runs/{run['id']}")
