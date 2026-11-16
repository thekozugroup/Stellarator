"""Run engine against the mock server (tools/mock_server.py, 400 tokens/s per sequence)."""

from __future__ import annotations

import asyncio
import contextlib
import socket
import time

from sqlalchemy import func, select

from stellarator.db import SessionLocal
from stellarator.models import Run, RunMetric, Sample
from stellarator.services.engine import RESTART_REASON, engine_registry

# --------------------------------------------------------------------------- helpers


async def make_endpoint(client, base_url: str, name: str = "Mock") -> int:
    r = await client.post("/api/endpoints", json={"name": name, "base_url": base_url})
    assert r.status_code == 201, r.text
    return r.json()["id"]


async def start_run(client, endpoint_id: int, prompt_set_id: int, model: str, concurrency: int = 8, **params) -> dict:
    r = await client.post(
        "/api/runs",
        json={"endpoint_id": endpoint_id, "model": model, "prompt_set_id": prompt_set_id,
              "concurrency": concurrency, "params": params},
    )
    assert r.status_code == 201, r.text
    return r.json()


async def get_run(client, run_id: int) -> dict:
    r = await client.get(f"/api/runs/{run_id}")
    assert r.status_code == 200, r.text
    return r.json()


async def wait_for(client, run_id: int, pred, timeout: float = 60.0, interval: float = 0.05) -> dict:
    deadline = time.monotonic() + timeout
    while True:
        run = await get_run(client, run_id)
        if pred(run):
            return run
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out; last state: {run}")
        await asyncio.sleep(interval)


async def live(client, run_id: int) -> dict:
    r = await client.get(f"/api/runs/{run_id}/live")
    assert r.status_code == 200, r.text
    return r.json()


async def wait_live(client, run_id: int, pred, timeout: float = 20.0) -> dict:
    deadline = time.monotonic() + timeout
    while True:
        lv = await live(client, run_id)
        if pred(lv):
            return lv
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out; last live state: in_flight={lv['in_flight']} run={lv['run']}")
        await asyncio.sleep(0.02)


async def truth(run_id: int) -> dict:
    """Counters computed from the sample rows."""
    async with SessionLocal() as s:
        rows = (
            await s.execute(
                select(Sample.status, func.count(), func.sum(Sample.prompt_tokens),
                       func.sum(Sample.completion_tokens), func.sum(Sample.reasoning_tokens))
                .where(Sample.run_id == run_id)
                .group_by(Sample.status)
            )
        ).all()
        truncated = (
            await s.execute(
                select(func.count()).select_from(Sample)
                .where(Sample.run_id == run_id, Sample.status == "done", Sample.finish_reason == "length")
            )
        ).scalar_one()
    by = {st: (n, p or 0, c or 0, r or 0) for st, n, p, c, r in rows}
    done = by.get("done", (0, 0, 0, 0))
    return {
        "done": done[0],
        "failed": by.get("failed", (0,))[0],
        "pending": by.get("pending", (0,))[0],
        "running": by.get("running", (0,))[0],
        "truncated": truncated,
        "prompt_tokens": done[1],
        "completion_tokens": done[2],
        "reasoning_tokens": done[3],
    }


def assert_counters_match(run: dict, t: dict) -> None:
    for key in ("done", "failed", "truncated", "prompt_tokens", "completion_tokens", "reasoning_tokens"):
        assert run[key] == t[key], (key, run[key], t[key])


def dead_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


# --------------------------------------------------------------------------- tests


async def test_run_of_60_at_concurrency_8(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(30)
    run = await start_run(client, ep, ps, "mock-reasoning-content", concurrency=8, samples_per_prompt=2, seed=7)
    assert run["total"] == 60 and run["pending"] <= 60

    lv = await wait_live(client, run["id"], lambda x: x["in_flight"] > 0)
    assert lv["in_flight"] <= 8
    seq = lv["sequences"][0]
    assert seq["prompt_preview"].startswith("Question ")
    assert seq["phase"] in ("waiting", "thinking", "answering") and seq["attempt"] == 1

    run = await wait_for(client, run["id"], lambda x: x["status"] == "completed")
    assert (run["done"], run["failed"], run["pending"]) == (60, 0, 0)
    assert run["completion_tokens"] > 0 and run["reasoning_tokens"] > 0 and run["active_seconds"] > 0
    assert_counters_match(run, await truth(run["id"]))

    page = (await client.get(f"/api/runs/{run['id']}/samples?limit=5")).json()
    assert page["total"] == 60
    assert [(i["prompt_idx"], i["sample_idx"]) for i in page["items"]] == [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)]
    first = page["items"][0]
    assert first["status"] == "done" and first["has_reasoning"] and first["response_preview"].startswith("Here is")

    sample = (await client.get(f"/api/samples/{first['id']}")).json()
    assert sample["reasoning"].startswith("The question asks")
    assert sample["response"].startswith("Here is the answer")
    assert sample["finish_reason"] == "stop" and sample["attempts"] == 1
    assert sample["ttft_ms"] is not None and sample["latency_ms"] >= sample["ttft_ms"]
    assert sample["messages"] == [{"role": "user", "content": "Question 0: what is 0 + 0?"}]

    lv = await live(client, run["id"])
    assert lv["in_flight"] == 0 and lv["sequences"] == [] and lv["eta_seconds"] is None
    assert len(lv["metrics"]) >= 2 and lv["metrics"][-1]["done"] == 60


async def test_every_reasoning_shape(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(6)
    models = ["mock-reasoning", "mock-think-tags", "mock-open-think", "mock-plain"]
    runs = [await start_run(client, ep, ps, m, concurrency=6) for m in models]
    for model, run in zip(models, runs):
        run = await wait_for(client, run["id"], lambda x: x["status"] == "completed")
        assert run["done"] == 6, (model, run)
        items = (await client.get(f"/api/runs/{run['id']}/samples")).json()["items"]
        for item in items:
            s = (await client.get(f"/api/samples/{item['id']}")).json()
            assert s["response"].startswith("Here is the answer"), (model, s["response"][:80])
            if model == "mock-plain":
                assert s["reasoning"] is None
            else:
                assert s["reasoning"].startswith("The question asks"), (model, s["reasoning"])
                assert "think>" not in s["reasoning"] and "think>" not in s["response"]


async def test_non_streaming_run(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(8)
    run = await start_run(client, ep, ps, "mock-reasoning", concurrency=8, stream=False,
                          system_prompt="Be brief.", user_template="Solve: {prompt}")
    run = await wait_for(client, run["id"], lambda x: x["status"] == "completed")
    assert run["done"] == 8 and run["completion_tokens"] > 0
    item = (await client.get(f"/api/runs/{run['id']}/samples?limit=1")).json()["items"][0]
    s = (await client.get(f"/api/samples/{item['id']}")).json()
    assert s["messages"] == [
        {"role": "system", "content": "Be brief."},
        {"role": "user", "content": "Solve: Question 0: what is 0 + 0?"},
    ]
    assert s["reasoning"] and s["response"].startswith("Here is the answer")
    assert s["ttft_ms"] is None and s["latency_ms"] > 0  # no first-token time without streaming


async def test_flaky_server_retries(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(60)
    run = await start_run(client, ep, ps, "mock-flaky", concurrency=16, max_attempts=3, pause_after_failures=0)
    rid = run["id"]
    # Pause while some samples wait to retry: they go back to pending and must not be lost.
    await wait_live(client, rid, lambda x: any(s["attempt"] > 1 for s in x["sequences"]), timeout=30)
    await client.post(f"/api/runs/{rid}/pause")
    await wait_for(client, rid, lambda x: x["status"] == "paused", timeout=20)
    assert (await truth(rid))["running"] == 0
    await client.post(f"/api/runs/{rid}/resume")
    run = await wait_for(client, rid, lambda x: x["status"] == "completed", timeout=90)
    t = await truth(run["id"])
    assert t["pending"] == 0 and t["running"] == 0
    assert run["done"] + run["failed"] == 60 and run["pending"] == 0
    assert_counters_match(run, t)
    async with SessionLocal() as s:
        retried = (
            await s.execute(select(func.count()).select_from(Sample).where(Sample.run_id == run["id"], Sample.attempts > 1))
        ).scalar_one()
    assert retried > 0  # 20% of requests fail, so some samples needed a second attempt
    failed = (await client.get(f"/api/runs/{run['id']}/samples?status=failed")).json()
    for item in failed["items"]:
        assert item["error"].startswith("The server answered")


async def test_retry_failed(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(60)
    run = await start_run(client, ep, ps, "mock-flaky", concurrency=12, max_attempts=1, pause_after_failures=0)
    run = await wait_for(client, run["id"], lambda x: x["status"] == "completed", timeout=60)
    assert run["failed"] > 0 and run["done"] + run["failed"] == 60
    r = await client.post(f"/api/runs/{run['id']}/retry-failed")
    assert r.status_code == 200, r.text
    after = r.json()
    assert after["status"] == "running" and after["failed"] == 0 and after["finished_at"] is None
    run = await wait_for(client, run["id"], lambda x: x["status"] == "completed", timeout=60)
    assert run["done"] + run["failed"] == 60
    assert_counters_match(run, await truth(run["id"]))
    # Nothing failed: retrying again changes nothing.
    if run["failed"] == 0:
        r = await client.post(f"/api/runs/{run['id']}/retry-failed")
        assert r.status_code == 200 and r.json()["status"] == "completed"


async def test_truncating_counts_truncated(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(16)
    run = await start_run(client, ep, ps, "mock-truncating", concurrency=8)
    run = await wait_for(client, run["id"], lambda x: x["status"] == "completed")
    assert run["done"] == 16 and run["truncated"] == 16
    assert_counters_match(run, await truth(run["id"]))
    page = (await client.get(f"/api/runs/{run['id']}/samples?status=truncated")).json()
    assert page["total"] == 16 and all(i["finish_reason"] == "length" for i in page["items"])
    s = (await client.get(f"/api/samples/{page['items'][0]['id']}")).json()
    assert s["response"] == "" and s["reasoning"]  # cut off while thinking


async def test_concurrency_changes_live(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(400)
    run = await start_run(client, ep, ps, "mock-reasoning", concurrency=3)
    rid = run["id"]
    await wait_live(client, rid, lambda x: x["in_flight"] == 3)
    r = await client.patch(f"/api/runs/{rid}", json={"concurrency": 24})
    assert r.status_code == 200 and r.json()["concurrency"] == 24
    lv = await wait_live(client, rid, lambda x: x["in_flight"] >= 20, timeout=5)
    assert lv["in_flight"] <= 24 and len(lv["sequences"]) == lv["in_flight"]
    started = [s["elapsed_ms"] for s in lv["sequences"]]
    assert started == sorted(started, reverse=True)  # oldest first
    r = await client.patch(f"/api/runs/{rid}", json={"concurrency": 2})
    await wait_live(client, rid, lambda x: x["in_flight"] <= 2, timeout=10)
    # It stays at the lower setting.
    for _ in range(10):
        assert (await live(client, rid))["in_flight"] <= 2
        await asyncio.sleep(0.05)
    r = await client.delete(f"/api/runs/{rid}")
    assert r.status_code == 204
    assert engine_registry.get(rid) is None
    assert (await client.get(f"/api/runs/{rid}")).status_code == 404
    async with SessionLocal() as s:
        left = (await s.execute(select(func.count()).select_from(Sample).where(Sample.run_id == rid))).scalar_one()
        metrics = (await s.execute(select(func.count()).select_from(RunMetric).where(RunMetric.run_id == rid))).scalar_one()
    assert left == 0 and metrics == 0


async def test_pause_then_resume(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(80)
    run = await start_run(client, ep, ps, "mock-reasoning-content", concurrency=16)
    rid = run["id"]
    await wait_for(client, rid, lambda x: x["done"] >= 8)
    r = await client.post(f"/api/runs/{rid}/pause")
    assert r.status_code == 200 and r.json()["status"] in ("pausing", "paused")
    run = await wait_for(client, rid, lambda x: x["status"] == "paused", timeout=20)
    assert run["status_reason"] is None
    t = await truth(rid)
    assert t["running"] == 0 and t["pending"] > 0 and run["pending"] == t["pending"]
    assert_counters_match(run, t)
    await asyncio.sleep(0.5)
    assert (await get_run(client, rid))["done"] == run["done"]  # nothing moves while paused
    assert (await client.post(f"/api/runs/{rid}/pause")).json()["status"] == "paused"  # idempotent

    r = await client.post(f"/api/runs/{rid}/resume")
    assert r.status_code == 200 and r.json()["status"] == "running"
    run = await wait_for(client, rid, lambda x: x["status"] == "completed")
    assert run["done"] == 80 and run["pending"] == 0
    assert_counters_match(run, await truth(rid))
    r = await client.post(f"/api/runs/{rid}/resume")
    assert r.status_code == 409 and "finished" in r.json()["detail"]
    r = await client.post(f"/api/runs/{rid}/pause")
    assert r.status_code == 409


async def test_pause_while_pausing_can_resume(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(40)
    run = await start_run(client, ep, ps, "mock-reasoning", concurrency=8)
    rid = run["id"]
    await wait_live(client, rid, lambda x: x["in_flight"] >= 4)
    assert (await client.post(f"/api/runs/{rid}/pause")).json()["status"] == "pausing"
    r = await client.post(f"/api/runs/{rid}/resume")  # changed our mind before it settled
    assert r.json()["status"] == "running"
    run = await wait_for(client, rid, lambda x: x["status"] == "completed")
    assert run["done"] == 40


async def test_recover_after_crash(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(120)
    run = await start_run(client, ep, ps, "mock-reasoning", concurrency=16)
    rid = run["id"]
    await wait_for(client, rid, lambda x: x["done"] >= 10)

    # Crash: the writer dies with unsaved work and the run task vanishes without cleanup.
    writer = engine_registry._writer
    writer.task.cancel()
    with contextlib.suppress(asyncio.CancelledError):
        await writer.task
    task = engine_registry.get(rid)
    task.main_task.cancel()
    with contextlib.suppress(asyncio.CancelledError):
        await task.main_task
    engine_registry._tasks.clear()
    engine_registry._writer = None
    async with SessionLocal() as s:  # make sure some rows are left mid-flight
        ids = (await s.execute(select(Sample.id).where(Sample.run_id == rid, Sample.status == "pending").limit(3))).scalars().all()
        await s.execute(Sample.__table__.update().where(Sample.id.in_(ids)).values(status="running"))
        await s.commit()
    before = await truth(rid)
    assert before["running"] >= 3
    assert (await get_run(client, rid))["status"] == "running"

    await engine_registry.recover()
    run = await get_run(client, rid)
    assert run["status"] == "paused" and run["status_reason"] == RESTART_REASON
    t = await truth(rid)
    assert t["running"] == 0 and t["pending"] == 120 - t["done"] - t["failed"]
    assert_counters_match(run, t)

    r = await client.post(f"/api/runs/{rid}/resume")
    assert r.status_code == 200 and r.json()["status_reason"] is None
    run = await wait_for(client, rid, lambda x: x["status"] == "completed")
    assert run["done"] == 120
    assert_counters_match(run, await truth(rid))


async def test_shutdown_returns_in_flight_to_pending(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(60)
    run = await start_run(client, ep, ps, "mock-reasoning", concurrency=8)
    rid = run["id"]
    await wait_live(client, rid, lambda x: x["in_flight"] == 8)
    await engine_registry.shutdown()
    t = await truth(rid)
    assert t["running"] == 0 and t["done"] + t["pending"] == 60
    async with SessionLocal() as s:
        row = await s.get(Run, rid)
        assert row.status == "running" and row.active_seconds > 0
    await engine_registry.recover()
    run = await get_run(client, rid)
    assert run["status"] == "paused" and run["status_reason"] == RESTART_REASON
    await client.delete(f"/api/runs/{rid}")


async def test_circuit_breaker_and_deleted_endpoint(client, make_prompt_set):
    ep = await make_endpoint(client, f"http://127.0.0.1:{dead_port()}/v1", name="Dead")
    ps = await make_prompt_set(50)
    run = await start_run(client, ep, ps, "any-model", concurrency=2, max_attempts=1, pause_after_failures=3)
    rid = run["id"]
    run = await wait_for(client, rid, lambda x: x["status"] == "paused", timeout=20)
    assert run["status_reason"].startswith("Paused after 3 failures in a row. Last error: Could not connect.")
    assert 3 <= run["failed"] <= 4 and run["done"] == 0
    assert_counters_match(run, await truth(rid))

    r = await client.post(f"/api/runs/{rid}/resume")
    assert r.status_code == 200 and r.json()["status_reason"] is None
    run = await wait_for(client, rid, lambda x: x["status"] == "paused", timeout=20)
    assert run["status_reason"].startswith("Paused after 3 failures in a row.")

    # Once its endpoint is gone the run cannot continue.
    assert (await client.delete(f"/api/endpoints/{ep}")).status_code == 204
    r = await client.post(f"/api/runs/{rid}/resume")
    assert r.status_code == 409 and "endpoint" in r.json()["detail"]
    r = await client.post(f"/api/runs/{rid}/retry-failed")
    assert r.status_code == 409
    run = await get_run(client, rid)
    assert run["endpoint_id"] is None and run["endpoint_name"] == "Dead" and run["status"] == "paused"


async def test_load_2000_samples_at_256(client, mock_server, make_prompt_set):
    ep = await make_endpoint(client, mock_server)
    ps = await make_prompt_set(1000)
    t0 = time.monotonic()
    run = await start_run(client, ep, ps, "mock-reasoning-content", concurrency=256, samples_per_prompt=2)
    peak = 0
    while True:
        lv = await live(client, run["id"])
        peak = max(peak, lv["in_flight"])
        if lv["run"]["status"] == "completed":
            break
        assert time.monotonic() - t0 < 120, lv["run"]
        await asyncio.sleep(0.2)
    run = await get_run(client, run["id"])
    t = await truth(run["id"])
    assert run["done"] == 2000 and run["failed"] == 0 and t["pending"] == 0 and t["running"] == 0
    assert_counters_match(run, t)
    assert peak > 128
    print(f"\n2,000 samples at 256 seqs: {time.monotonic() - t0:.1f} s, peak in flight {peak}")


async def test_pause_does_not_wait_for_retry_backoff(client, make_prompt_set):
    ep = await make_endpoint(client, f"http://127.0.0.1:{dead_port()}/v1", name="Down")
    ps = await make_prompt_set(20)
    run = await start_run(client, ep, ps, "any-model", concurrency=4, max_attempts=10, pause_after_failures=0)
    rid = run["id"]
    # Every request fails to connect, so all four sit in backoff between attempts.
    lv = await wait_live(client, rid, lambda x: x["in_flight"] == 4 and all(s["attempt"] >= 2 for s in x["sequences"]))
    assert all(s["phase"] == "waiting" for s in lv["sequences"])
    t0 = time.monotonic()
    await client.post(f"/api/runs/{rid}/pause")
    run = await wait_for(client, rid, lambda x: x["status"] == "paused", timeout=5)
    assert time.monotonic() - t0 < 1.5
    assert run["failed"] == 0 and run["pending"] == 20  # waiting samples went back to pending
    t = await truth(rid)
    assert t["running"] == 0 and t["pending"] == 20
    await client.delete(f"/api/runs/{rid}")
