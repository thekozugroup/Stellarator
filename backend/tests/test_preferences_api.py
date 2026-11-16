"""Preference sets API: create, list, items, pairs, auto-pair, delete.

Self-contained: builds a small app with only the preferences router on a temporary SQLite file,
and creates prompt sets, runs and samples directly through the ORM.
"""

from __future__ import annotations

import os
import tempfile

os.environ.setdefault("STELLARATOR_DATA_DIR", tempfile.mkdtemp(prefix="stellarator-test-"))

import httpx
import pytest
from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from sqlalchemy import event, func, select
from sqlalchemy.ext.asyncio import async_sessionmaker, create_async_engine

from stellarator.api import preferences
from stellarator.db import get_session
from stellarator.models import Base, PreferencePair, Prompt, PromptSet, Run, Sample


def _make_app() -> FastAPI:
    app = FastAPI()
    app.include_router(preferences.router)

    @app.exception_handler(RequestValidationError)
    async def _validation_error(_request: Request, exc: RequestValidationError):  # same shape as main.py
        fields: dict[str, str] = {}
        for err in exc.errors():
            loc = [str(p) for p in err.get("loc", []) if p not in ("body", "query", "path")]
            fields[".".join(loc) or "_"] = str(err.get("msg", "Invalid value")).removeprefix("Value error, ")
        return JSONResponse(status_code=422, content={"detail": next(iter(fields.values())), "fields": fields})

    return app


@pytest.fixture
async def env(tmp_path):
    engine = create_async_engine(f"sqlite+aiosqlite:///{tmp_path / 'prefs.db'}")

    @event.listens_for(engine.sync_engine, "connect")
    def _fk(dbapi_conn, _record):
        cur = dbapi_conn.cursor()
        cur.execute("PRAGMA foreign_keys=ON")
        cur.close()

    statements: list[str] = []

    @event.listens_for(engine.sync_engine, "before_cursor_execute")
    def _count(_conn, _cursor, statement, *_args):
        statements.append(statement)

    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)
    maker = async_sessionmaker(engine, expire_on_commit=False)

    async def _session():
        async with maker() as s:
            yield s

    app = _make_app()
    app.dependency_overrides[get_session] = _session
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://test") as client:
        yield {"client": client, "maker": maker, "statements": statements}
    await engine.dispose()


async def _prompt_set(maker, name: str, n: int) -> tuple[int, list[int]]:
    async with maker() as s:
        ps = PromptSet(name=name, source_kind="paste", source={}, prompt_count=n)
        s.add(ps)
        await s.flush()
        prompts = [
            Prompt(prompt_set_id=ps.id, idx=i, messages=[{"role": "user", "content": f"Question {i}"}], meta={})
            for i in range(n)
        ]
        s.add_all(prompts)
        await s.commit()
        return ps.id, [p.id for p in prompts]


async def _run(maker, prompt_set_id: int, name: str, model: str) -> int:
    async with maker() as s:
        run = Run(
            name=name,
            endpoint_name="Spark",
            model=model,
            prompt_set_id=prompt_set_id,
            params={},
            status="completed",
        )
        s.add(run)
        await s.commit()
        return run.id


async def _sample(
    maker,
    run_id: int,
    prompt_id: int,
    sample_idx: int = 0,
    status: str = "done",
    finish_reason: str | None = "stop",
) -> int:
    async with maker() as s:
        prompt = await s.get(Prompt, prompt_id)
        sample = Sample(
            run_id=run_id,
            prompt_id=prompt_id,
            prompt_idx=prompt.idx,
            sample_idx=sample_idx,
            status=status,
            reasoning=f"thinking {run_id}/{prompt.idx}/{sample_idx}" if status == "done" else None,
            response=f"answer {run_id}/{prompt.idx}/{sample_idx}" if status == "done" else None,
            finish_reason=finish_reason if status == "done" else None,
            completion_tokens=100 + sample_idx if status == "done" else None,
        )
        s.add(sample)
        await s.commit()
        return sample.id


async def _two_runs(env, n: int = 4):
    """Prompt set of n prompts; run A and run B each answered every prompt once."""
    maker = env["maker"]
    ps_id, prompt_ids = await _prompt_set(maker, "Math word problems", n)
    run_a = await _run(maker, ps_id, "Qwen3 on math", "Qwen/Qwen3-32B")
    run_b = await _run(maker, ps_id, "Llama on math", "meta-llama/Llama-3.1-8B-Instruct")
    a = {pid: await _sample(maker, run_a, pid) for pid in prompt_ids}
    b = {pid: await _sample(maker, run_b, pid) for pid in prompt_ids}
    return ps_id, prompt_ids, run_a, run_b, a, b


async def _create(client, name: str, run_ids: list[int]) -> dict:
    res = await client.post("/api/preference-sets", json={"name": name, "run_ids": run_ids})
    assert res.status_code == 201, res.text
    return res.json()


# --------------------------------------------------------------------------- create and list


async def test_create_returns_summary(env):
    client = env["client"]
    ps_id, prompt_ids, run_a, run_b, *_ = await _two_runs(env, 3)
    out = await _create(client, "  Qwen vs Llama  ", [run_a, run_b, run_a])
    assert out["name"] == "Qwen vs Llama"
    assert out["prompt_set_id"] == ps_id
    assert out["prompt_set_name"] == "Math word problems"
    assert out["run_ids"] == [run_a, run_b]
    assert out["runs"] == [
        {"id": run_a, "name": "Qwen3 on math", "model": "Qwen/Qwen3-32B"},
        {"id": run_b, "name": "Llama on math", "model": "meta-llama/Llama-3.1-8B-Instruct"},
    ]
    assert out["prompt_count"] == 3
    assert out["decided"] == 0

    got = await client.get(f"/api/preference-sets/{out['id']}")
    assert got.status_code == 200
    assert got.json() == out


async def test_create_rejects_runs_on_different_prompt_sets(env):
    client, maker = env["client"], env["maker"]
    ps1, _ = await _prompt_set(maker, "One", 1)
    ps2, _ = await _prompt_set(maker, "Two", 1)
    r1 = await _run(maker, ps1, "r1", "m1")
    r2 = await _run(maker, ps2, "r2", "m2")
    res = await client.post("/api/preference-sets", json={"name": "Mixed", "run_ids": [r1, r2]})
    assert res.status_code == 422
    assert res.json()["detail"] == "All runs must use the same prompt set."


async def test_create_rejects_missing_run_and_bad_name(env):
    client, maker = env["client"], env["maker"]
    ps, _ = await _prompt_set(maker, "One", 1)
    r1 = await _run(maker, ps, "r1", "m1")

    res = await client.post("/api/preference-sets", json={"name": "Gone", "run_ids": [r1, 9999]})
    assert res.status_code == 422
    assert "no longer exists" in res.json()["detail"]

    res = await client.post("/api/preference-sets", json={"name": "   ", "run_ids": [r1]})
    assert res.status_code == 422
    assert res.json()["fields"] == {"name": "Enter a name for the preference set."}

    res = await client.post("/api/preference-sets", json={"name": "", "run_ids": [r1]})
    assert res.status_code == 422
    assert "name" in res.json()["fields"]

    res = await client.post("/api/preference-sets", json={"name": "No runs", "run_ids": []})
    assert res.status_code == 422
    assert "run_ids" in res.json()["fields"]


async def test_prompt_count_needs_two_finished_candidates(env):
    client, maker = env["client"], env["maker"]
    ps, prompt_ids = await _prompt_set(maker, "Set", 5)
    run = await _run(maker, ps, "Sampled twice", "m")
    other = await _run(maker, ps, "Other", "m2")
    p0, p1, p2, p3, p4 = prompt_ids
    # p0: two done samples from one run -> counts
    await _sample(maker, run, p0, 0)
    await _sample(maker, run, p0, 1)
    # p1: one done, one failed -> does not count
    await _sample(maker, run, p1, 0)
    await _sample(maker, run, p1, 1, status="failed")
    # p2: one done in each run, one truncated -> counts (truncated is still done)
    await _sample(maker, run, p2, 0, finish_reason="length")
    await _sample(maker, other, p2, 0)
    # p3: pending and running -> does not count
    await _sample(maker, run, p3, 0, status="pending")
    await _sample(maker, run, p3, 1, status="running")
    # p4: one done in a run that is NOT in the set -> only one candidate inside the set
    await _sample(maker, run, p4, 0)
    await _sample(maker, other, p4, 0)

    only_run = await _create(client, "Single run", [run])
    assert only_run["prompt_count"] == 1  # p0
    both = await _create(client, "Both runs", [run, other])
    assert both["prompt_count"] == 3  # p0, p2, p4


async def test_list_is_newest_first_with_decided_counts(env):
    client = env["client"]
    _, prompt_ids, run_a, run_b, a, b = await _two_runs(env, 2)
    first = await _create(client, "First", [run_a, run_b])
    second = await _create(client, "Second", [run_a, run_b])
    res = await client.put(
        f"/api/preference-sets/{first['id']}/pairs/{prompt_ids[0]}",
        json={"chosen_sample_id": a[prompt_ids[0]], "rejected_sample_id": b[prompt_ids[0]]},
    )
    assert res.status_code == 200
    listed = (await client.get("/api/preference-sets")).json()
    assert [s["id"] for s in listed] == [second["id"], first["id"]]
    assert {s["name"]: s["decided"] for s in listed} == {"First": 1, "Second": 0}
    assert all(s["prompt_count"] == 2 for s in listed)
    assert all(s["prompt_set_name"] == "Math word problems" for s in listed)


async def test_unknown_set_is_404(env):
    client = env["client"]
    assert (await client.get("/api/preference-sets/42")).status_code == 404
    assert (await client.get("/api/preference-sets/42/items")).status_code == 404
    assert (await client.delete("/api/preference-sets/42")).status_code == 404
    res = await client.put("/api/preference-sets/42/pairs/1", json={"chosen_sample_id": 1, "rejected_sample_id": 2})
    assert res.status_code == 404


# --------------------------------------------------------------------------- items


async def test_items_pages_in_prompt_order_with_candidates(env):
    client, maker = env["client"], env["maker"]
    ps, prompt_ids = await _prompt_set(maker, "Set", 6)
    run_a = await _run(maker, ps, "Two samples", "model-a")
    run_b = await _run(maker, ps, "One sample", "model-b")
    # Insert out of order to prove sorting: run B first, then run A's samples 1 then 0.
    for pid in reversed(prompt_ids):
        await _sample(maker, run_b, pid, 0)
        await _sample(maker, run_a, pid, 1, finish_reason="length")
        await _sample(maker, run_a, pid, 0)
    pset = await _create(client, "Review", [run_a, run_b])

    res = await client.get(f"/api/preference-sets/{pset['id']}/items", params={"offset": 2, "limit": 3})
    assert res.status_code == 200
    page = res.json()
    assert page["total"] == 6 and page["offset"] == 2 and page["limit"] == 3
    assert [it["prompt_idx"] for it in page["items"]] == [2, 3, 4]
    item = page["items"][0]
    assert item["prompt_id"] == prompt_ids[2]
    assert item["messages"] == [{"role": "user", "content": "Question 2"}]
    assert item["chosen_sample_id"] is None and item["rejected_sample_id"] is None and item["source"] is None
    cands = item["candidates"]
    assert [(c["run_id"], c["sample_idx"]) for c in cands] == [(run_a, 0), (run_a, 1), (run_b, 0)]
    assert cands[0]["run_name"] == "Two samples" and cands[0]["model"] == "model-a"
    assert cands[1]["finish_reason"] == "length"
    assert cands[2]["reasoning"] == f"thinking {run_b}/2/0"
    assert cands[2]["response"] == f"answer {run_b}/2/0"
    assert cands[0]["completion_tokens"] == 100 and cands[1]["completion_tokens"] == 101


async def test_items_filter_and_query_count(env):
    client, statements = env["client"], env["statements"]
    _, prompt_ids, run_a, run_b, a, b = await _two_runs(env, 30)
    pset = await _create(client, "Filters", [run_a, run_b])
    url = f"/api/preference-sets/{pset['id']}"
    for pid in prompt_ids[:3]:
        res = await client.put(f"{url}/pairs/{pid}", json={"chosen_sample_id": a[pid], "rejected_sample_id": b[pid]})
        assert res.status_code == 200

    decided = (await client.get(f"{url}/items", params={"filter": "decided"})).json()
    assert decided["total"] == 3
    assert [it["prompt_idx"] for it in decided["items"]] == [0, 1, 2]
    assert decided["items"][0]["chosen_sample_id"] == a[prompt_ids[0]]
    assert decided["items"][0]["rejected_sample_id"] == b[prompt_ids[0]]
    assert decided["items"][0]["source"] == "manual"

    undecided = (await client.get(f"{url}/items", params={"filter": "undecided", "limit": 1})).json()
    assert undecided["total"] == 27
    assert undecided["items"][0]["prompt_idx"] == 3

    everything = (await client.get(f"{url}/items", params={"filter": "all", "limit": 200})).json()
    assert everything["total"] == 30 and len(everything["items"]) == 30

    bad = await client.get(f"{url}/items", params={"filter": "maybe"})
    assert bad.status_code == 422

    # No N+1: a page of 25 prompts costs the same number of statements as a page of 1.
    statements.clear()
    await client.get(f"{url}/items", params={"limit": 1})
    one = len(statements)
    statements.clear()
    await client.get(f"{url}/items", params={"limit": 25})
    assert len(statements) == one


# --------------------------------------------------------------------------- pairs


async def test_put_pair_upserts_one_pair_per_prompt(env):
    client, maker = env["client"], env["maker"]
    _, prompt_ids, run_a, run_b, a, b = await _two_runs(env, 2)
    pset = await _create(client, "Upsert", [run_a, run_b])
    pid = prompt_ids[0]
    url = f"/api/preference-sets/{pset['id']}/pairs/{pid}"

    res = await client.put(url, json={"chosen_sample_id": a[pid], "rejected_sample_id": b[pid]})
    assert res.status_code == 200
    item = res.json()
    assert item["prompt_id"] == pid
    assert item["chosen_sample_id"] == a[pid] and item["rejected_sample_id"] == b[pid]
    assert item["source"] == "manual"
    assert len(item["candidates"]) == 2

    res = await client.put(url, json={"chosen_sample_id": b[pid], "rejected_sample_id": a[pid]})
    assert res.status_code == 200
    assert res.json()["chosen_sample_id"] == b[pid]

    async with maker() as s:
        pairs = (await s.execute(select(PreferencePair))).scalars().all()
    assert len(pairs) == 1 and pairs[0].chosen_sample_id == b[pid]
    assert (await client.get(f"/api/preference-sets/{pset['id']}")).json()["decided"] == 1


async def test_put_pair_validation(env):
    client, maker = env["client"], env["maker"]
    ps_id, prompt_ids, run_a, run_b, a, b = await _two_runs(env, 2)
    p0, p1 = prompt_ids
    outsider = await _run(maker, ps_id, "Not in set", "m3")
    outsider_sample = await _sample(maker, outsider, p0)
    failed = await _sample(maker, run_a, p0, 1, status="failed")
    other_ps, other_prompts = await _prompt_set(maker, "Other set", 1)
    pset = await _create(client, "Validation", [run_a, run_b])
    url = f"/api/preference-sets/{pset['id']}/pairs"

    async def put(prompt_id, chosen, rejected):
        return await client.put(
            f"{url}/{prompt_id}", json={"chosen_sample_id": chosen, "rejected_sample_id": rejected}
        )

    res = await put(p0, a[p0], a[p0])
    assert res.status_code == 422 and res.json()["detail"] == "Chosen and rejected must be two different responses."

    res = await put(p0, failed, b[p0])
    assert res.status_code == 422 and "has not finished" in res.json()["detail"]

    res = await put(p0, a[p0], outsider_sample)
    assert res.status_code == 422 and "not in this preference set" in res.json()["detail"]

    res = await put(p0, a[p1], b[p0])
    assert res.status_code == 422 and "different prompt" in res.json()["detail"]

    res = await put(p0, a[p0], 999999)
    assert res.status_code == 422 and "does not exist" in res.json()["detail"]

    res = await put(other_prompts[0], a[p0], b[p0])
    assert res.status_code == 404

    res = await client.put(f"{url}/{p0}", json={"chosen_sample_id": a[p0]})
    assert res.status_code == 422 and "rejected_sample_id" in res.json()["fields"]

    # Nothing was written by the failed attempts.
    assert (await client.get(f"/api/preference-sets/{pset['id']}")).json()["decided"] == 0


async def test_delete_pair(env):
    client = env["client"]
    _, prompt_ids, run_a, run_b, a, b = await _two_runs(env, 2)
    pset = await _create(client, "Clear", [run_a, run_b])
    pid = prompt_ids[1]
    url = f"/api/preference-sets/{pset['id']}/pairs/{pid}"
    await client.put(url, json={"chosen_sample_id": a[pid], "rejected_sample_id": b[pid]})
    assert (await client.get(f"/api/preference-sets/{pset['id']}")).json()["decided"] == 1

    assert (await client.delete(url)).status_code == 204
    assert (await client.get(f"/api/preference-sets/{pset['id']}")).json()["decided"] == 0
    # Clearing again is harmless.
    assert (await client.delete(url)).status_code == 204
    items = (await client.get(f"/api/preference-sets/{pset['id']}/items", params={"filter": "undecided"})).json()
    assert items["total"] == 2


# --------------------------------------------------------------------------- auto-pair


async def test_auto_pair_prefers_one_run_and_keeps_manual_pairs(env):
    client, maker = env["client"], env["maker"]
    ps_id, prompt_ids = await _prompt_set(maker, "Auto", 5)
    run_a = await _run(maker, ps_id, "Strong", "big")
    run_b = await _run(maker, ps_id, "Weak", "small")
    p0, p1, p2, p3, p4 = prompt_ids
    first_a = {}
    for pid in (p0, p1, p2, p3):
        # Run A's sample 0 failed on p0, so its first finished sample is sample 1.
        if pid == p0:
            await _sample(maker, run_a, pid, 0, status="failed")
            first_a[pid] = await _sample(maker, run_a, pid, 1)
        else:
            first_a[pid] = await _sample(maker, run_a, pid, 0)
            await _sample(maker, run_a, pid, 1)
    first_b = {pid: await _sample(maker, run_b, pid, 0) for pid in (p0, p1, p2)}
    # p3: run B has nothing; p4: neither run answered.
    pset = await _create(client, "Auto pairs", [run_a, run_b])
    base = f"/api/preference-sets/{pset['id']}"

    # A hand-made pair on p1 (reversed preference) must survive.
    res = await client.put(
        f"{base}/pairs/{p1}", json={"chosen_sample_id": first_b[p1], "rejected_sample_id": first_a[p1]}
    )
    assert res.status_code == 200

    res = await client.post(f"{base}/auto-pair", json={"chosen_run_id": run_a, "rejected_run_id": run_b})
    assert res.status_code == 200
    assert res.json() == {"created": 2, "skipped": 1}

    async with maker() as s:
        pairs = {
            p.prompt_id: p
            for p in (
                await s.execute(select(PreferencePair).where(PreferencePair.preference_set_id == pset["id"]))
            ).scalars()
        }
    assert set(pairs) == {p0, p1, p2}
    assert pairs[p0].chosen_sample_id == first_a[p0] and pairs[p0].rejected_sample_id == first_b[p0]
    assert pairs[p0].source == "rule"
    assert pairs[p1].chosen_sample_id == first_b[p1] and pairs[p1].source == "manual"

    # Running it again rewrites rule pairs and still keeps the manual one.
    res = await client.post(f"{base}/auto-pair", json={"chosen_run_id": run_a, "rejected_run_id": run_b})
    assert res.json() == {"created": 2, "skipped": 1}

    # Overwrite replaces the manual pair too, and flips the rule pairs to the new direction.
    res = await client.post(
        f"{base}/auto-pair", json={"chosen_run_id": run_b, "rejected_run_id": run_a, "overwrite": True}
    )
    assert res.json() == {"created": 3, "skipped": 0}
    async with maker() as s:
        rows = (
            await s.execute(select(PreferencePair).where(PreferencePair.preference_set_id == pset["id"]))
        ).scalars().all()
    assert len(rows) == 3
    assert all(r.source == "rule" for r in rows)
    assert {r.prompt_id: r.chosen_sample_id for r in rows} == {pid: first_b[pid] for pid in (p0, p1, p2)}
    assert (await client.get(base)).json()["decided"] == 3


async def test_auto_pair_validation(env):
    client, maker = env["client"], env["maker"]
    ps_id, _, run_a, run_b, *_ = await _two_runs(env, 1)
    outsider = await _run(maker, ps_id, "Outsider", "m")
    pset = await _create(client, "Rules", [run_a, run_b])
    url = f"/api/preference-sets/{pset['id']}/auto-pair"

    res = await client.post(url, json={"chosen_run_id": run_a, "rejected_run_id": run_a})
    assert res.status_code == 422 and "two different runs" in res.json()["detail"]

    res = await client.post(url, json={"chosen_run_id": outsider, "rejected_run_id": run_b})
    assert res.status_code == 422 and res.json()["detail"] == "The chosen run is not in this preference set."

    res = await client.post(url, json={"chosen_run_id": run_a, "rejected_run_id": outsider})
    assert res.status_code == 422 and res.json()["detail"] == "The rejected run is not in this preference set."


async def test_auto_pair_handles_many_prompts(env):
    """More rows than one INSERT batch."""
    client, maker = env["client"], env["maker"]
    ps_id, prompt_ids = await _prompt_set(maker, "Big", 1200)
    run_a = await _run(maker, ps_id, "A", "a")
    run_b = await _run(maker, ps_id, "B", "b")
    async with maker() as s:
        for rid in (run_a, run_b):
            s.add_all(
                Sample(run_id=rid, prompt_id=pid, prompt_idx=i, sample_idx=0, status="done", response="x")
                for i, pid in enumerate(prompt_ids)
            )
        await s.commit()
    pset = await _create(client, "Big", [run_a, run_b])
    assert pset["prompt_count"] == 1200
    res = await client.post(
        f"/api/preference-sets/{pset['id']}/auto-pair", json={"chosen_run_id": run_a, "rejected_run_id": run_b}
    )
    assert res.json() == {"created": 1200, "skipped": 0}
    items = (await client.get(f"/api/preference-sets/{pset['id']}/items", params={"filter": "undecided"})).json()
    assert items["total"] == 0


# --------------------------------------------------------------------------- delete


async def test_delete_set_removes_pairs(env):
    client, maker = env["client"], env["maker"]
    _, prompt_ids, run_a, run_b, a, b = await _two_runs(env, 3)
    keep = await _create(client, "Keep", [run_a, run_b])
    drop = await _create(client, "Drop", [run_a, run_b])
    for pset in (keep, drop):
        res = await client.post(
            f"/api/preference-sets/{pset['id']}/auto-pair", json={"chosen_run_id": run_a, "rejected_run_id": run_b}
        )
        assert res.json()["created"] == 3

    assert (await client.delete(f"/api/preference-sets/{drop['id']}")).status_code == 204
    assert (await client.get(f"/api/preference-sets/{drop['id']}")).status_code == 404
    async with maker() as s:
        left = (
            await s.execute(select(PreferencePair.preference_set_id, func.count()).group_by(PreferencePair.preference_set_id))
        ).all()
    assert left == [(keep["id"], 3)]
    assert [s["id"] for s in (await client.get("/api/preference-sets")).json()] == [keep["id"]]


async def test_deleted_run_drops_out_of_set(env):
    client, maker = env["client"], env["maker"]
    _, prompt_ids, run_a, run_b, a, b = await _two_runs(env, 2)
    pset = await _create(client, "Run deleted", [run_a, run_b])
    async with maker() as s:
        await s.delete(await s.get(Run, run_b))
        await s.commit()
    out = (await client.get(f"/api/preference-sets/{pset['id']}")).json()
    assert out["run_ids"] == [run_a]
    assert out["prompt_count"] == 0
    items = (await client.get(f"/api/preference-sets/{pset['id']}/items")).json()
    assert items == {"items": [], "total": 0, "offset": 0, "limit": 20}
