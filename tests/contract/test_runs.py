"""/api/runs and /api/samples: create, watch, steer, browse and delete."""

from __future__ import annotations

import time


from shapes import (
    RUN,
    RUN_LIVE,
    RUN_PARAMS,
    SAMPLE,
    SAMPLE_SUMMARY,
    ListOf,
    check,
    page,
    parse_dt,
)
from support import (
    assert_error,
    assert_no_content,
    get_run,
    list_samples,
    wait_run,
    wait_until,
)

DEFAULT_PARAMS = {
    "samples_per_prompt": 1,
    "temperature": 0.7,
    "top_p": None,
    "max_tokens": 8192,
    "max_tokens_field": "max_tokens",
    "seed": None,
    "system_prompt": None,
    "user_template": None,
    "extra_body": {},
    "stream": True,
    "think_open": "<think>",
    "think_close": "</think>",
    "max_attempts": 4,
    "request_timeout_s": 1800.0,
    "pause_after_failures": 10,
}


# --------------------------------------------------------------------------- the main run


def test_main_run_as_created(world):
    run = world.created["main"]
    check(run, RUN)
    assert run["name"] == "Contract Main Run"
    assert run["status"] == "running"
    assert run["status_reason"] is None
    assert run["endpoint_id"] == world.endpoint["id"] and run["endpoint_name"] == "Contract mock"
    assert run["model"] == "mock-reasoning-content"
    assert run["prompt_set_id"] == world.prompt_set["id"] and run["prompt_set_name"] == "Contract prompts"
    assert run["concurrency"] == 16
    assert run["total"] == 60
    assert run["params"] == {**DEFAULT_PARAMS, "samples_per_prompt": 2}
    assert run["done"] + run["failed"] + run["truncated"] <= 60
    assert run["finished_at"] is None
    assert run["started_at"] is not None
    assert parse_dt(run["created_at"]) <= parse_dt(run["started_at"])


def test_main_run_completed(world):
    run = world.main
    check(run, RUN)
    assert run["status"] == "completed" and run["status_reason"] is None
    assert (run["total"], run["done"], run["failed"], run["truncated"], run["pending"]) == (60, 60, 0, 0, 0)
    assert run["prompt_tokens"] > 0 and run["completion_tokens"] > 0
    assert 0 < run["reasoning_tokens"] < run["completion_tokens"]
    assert run["active_seconds"] > 0
    assert run["finished_at"] is not None
    assert parse_dt(run["finished_at"]) >= parse_dt(run["started_at"])


def test_get_run_matches_list(api, world):
    run = get_run(api, world.main["id"])
    listed = api.get("/api/runs")
    assert listed.status_code == 200
    items = listed.json()
    check(items, ListOf(RUN))
    assert next(r for r in items if r["id"] == run["id"]) == run
    ids = [r["id"] for r in items]
    # Newest first: the runs of the world were created main, think, plain, trunc, other.
    order = [world.main["id"], world.think["id"], world.plain["id"], world.trunc["id"], world.other["id"]]
    positions = [ids.index(i) for i in order]
    assert positions == sorted(positions, reverse=True)


def test_unknown_run_is_404_everywhere(api):
    for method, path in (
        ("get", "/api/runs/999999"),
        ("patch", "/api/runs/999999"),
        ("delete", "/api/runs/999999"),
        ("post", "/api/runs/999999/pause"),
        ("post", "/api/runs/999999/resume"),
        ("post", "/api/runs/999999/retry-failed"),
        ("get", "/api/runs/999999/live"),
        ("get", "/api/runs/999999/samples"),
    ):
        kwargs = {"json": {}} if method == "patch" else {}
        assert_error(getattr(api, method)(path, **kwargs), 404, detail="Run not found")
    assert_error(api.get("/api/samples/999999"), 404, detail="Sample not found")
    assert_error(api.get("/api/runs/abc"), 422)


def test_run_totals_match_the_samples(api, world):
    samples = list_samples(api, world.main["id"])
    assert len(samples) == 60
    assert sum(s["completion_tokens"] for s in samples) == world.main["completion_tokens"]
    assert all(s["status"] == "done" for s in samples)


# --------------------------------------------------------------------------- live


def test_live_of_a_finished_run(api, world):
    r = api.get(f"/api/runs/{world.main['id']}/live")
    assert r.status_code == 200
    live = r.json()
    check(live, RUN_LIVE)
    assert live["run"] == world.main
    assert live["in_flight"] == 0 and live["sequences"] == []
    assert live["tokens_per_s"] == 0 and live["samples_per_min"] == 0
    assert live["eta_seconds"] is None
    assert 1 <= len(live["metrics"]) <= 240
    ts = [parse_dt(m["t"]) for m in live["metrics"]]
    assert ts == sorted(ts)
    assert live["metrics"][-1]["done"] == 60 and live["metrics"][-1]["active"] == 0


def test_live_while_the_run_generates(world):
    snaps = world.live_snapshots
    assert snaps, "no snapshots were taken"
    for s in snaps:
        check(s, RUN_LIVE)
        assert len(s["sequences"]) <= 64
        assert 0 <= s["in_flight"] <= s["run"]["concurrency"]
        assert len(s["sequences"]) <= s["in_flight"] or s["in_flight"] == 0 or len(s["sequences"]) == 64
        assert 0 <= s["run"]["pending"] <= s["run"]["total"]
        assert len(s["metrics"]) <= 240
        for seq in s["sequences"]:
            assert len(seq["prompt_preview"]) <= 160
            assert len(seq["tail"]) <= 300
            assert seq["attempt"] >= 1 and seq["tokens"] >= 0 and seq["elapsed_ms"] >= 0
            assert 0 <= seq["prompt_idx"] < 30 and seq["sample_idx"] in (0, 1)
            assert seq["prompt_preview"].startswith("Task tag-")
    busy = [s for s in snaps if s["in_flight"] > 0]
    assert busy, "the run was never seen in flight"
    phases = {q["phase"] for s in busy for q in s["sequences"]}
    assert "thinking" in phases, phases
    assert any(s["tokens_per_s"] > 0 for s in busy)
    assert any(q["tail"] for s in busy for q in s["sequences"])
    # Sequences are listed oldest first.
    for s in busy:
        elapsed = [q["elapsed_ms"] for q in s["sequences"]]
        assert elapsed == sorted(elapsed, reverse=True), elapsed
    # Progress only moves forward.
    done = [s["run"]["done"] for s in snaps]
    assert done == sorted(done)


# --------------------------------------------------------------------------- samples


def test_samples_page(api, world):
    url = f"/api/runs/{world.main['id']}/samples"
    r = api.get(url)
    assert r.status_code == 200
    body = r.json()
    check(body, page(SAMPLE_SUMMARY))
    assert (body["total"], body["offset"], body["limit"]) == (60, 0, 50)
    assert len(body["items"]) == 50
    keys = [(s["prompt_idx"], s["sample_idx"]) for s in body["items"]]
    assert keys == [(i // 2, i % 2) for i in range(50)]
    first = body["items"][0]
    assert first["status"] == "done" and first["has_reasoning"] is True
    assert first["finish_reason"] == "stop" and first["error"] is None
    assert first["prompt_preview"] == "Task tag-000: compute 3 plus 40."
    assert first["response_preview"] and len(first["response_preview"]) <= 160
    assert first["completion_tokens"] > 0 and first["latency_ms"] >= 0

    body = api.get(url, params={"offset": 50, "limit": 50}).json()
    assert len(body["items"]) == 10 and body["offset"] == 50
    assert (body["items"][0]["prompt_idx"], body["items"][0]["sample_idx"]) == (25, 0)


def test_samples_filters(api, world):
    url = f"/api/runs/{world.main['id']}/samples"
    assert api.get(url, params={"status": "done"}).json()["total"] == 60
    for status in ("failed", "pending", "running", "truncated"):
        body = api.get(url, params={"status": status}).json()
        check(body, page(SAMPLE_SUMMARY))
        assert body["total"] == 0 and body["items"] == [], status
    assert_error(api.get(url, params={"status": "bogus"}), 422, field="status")
    assert_error(api.get(url, params={"limit": 0}), 422, field="limit")
    assert_error(api.get(url, params={"limit": 501}), 422, field="limit")
    assert_error(api.get(url, params={"offset": -1}), 422, field="offset")


def test_samples_search(api, world):
    url = f"/api/runs/{world.main['id']}/samples"
    body = api.get(url, params={"q": "tag-007"}).json()  # in the prompt
    assert body["total"] == 2
    assert [(s["prompt_idx"], s["sample_idx"]) for s in body["items"]] == [(7, 0), (7, 1)]
    assert api.get(url, params={"q": "TAG-007"}).json()["total"] == 2
    assert api.get(url, params={"q": "final answer"}).json()["total"] == 60  # in the response
    assert api.get(url, params={"q": "no such text anywhere"}).json()["total"] == 0
    assert api.get(url, params={"q": "   "}).json()["total"] == 60  # blank means no search
    # LIKE wildcards in the query are literal characters.
    assert api.get(url, params={"q": "%"}).json()["total"] == 0
    assert api.get(url, params={"q": "_"}).json()["total"] == 0
    both = api.get(url, params={"q": "tag-007", "status": "done", "limit": 1}).json()
    assert both["total"] == 2 and len(both["items"]) == 1


def test_sample_detail(api, world):
    summaries = api.get(f"/api/runs/{world.main['id']}/samples", params={"limit": 3}).json()["items"]
    s = api.get(f"/api/samples/{summaries[1]['id']}")
    assert s.status_code == 200
    sample = s.json()
    check(sample, SAMPLE)
    assert sample["run_id"] == world.main["id"]
    assert (sample["prompt_idx"], sample["sample_idx"]) == (0, 1)
    assert sample["prompt_id"] == world.prompts[0]["id"]
    assert sample["status"] == "done" and sample["finish_reason"] == "stop"
    assert sample["reasoning"].startswith("The question asks: Task tag-000")
    assert sample["response"].endswith(".") and "Final answer:" in sample["response"]
    assert "<think>" not in sample["response"] and "<think>" not in sample["reasoning"]
    assert sample["prompt_tokens"] > 0 and sample["completion_tokens"] > 0
    assert 0 < sample["reasoning_tokens"] < sample["completion_tokens"]
    assert sample["latency_ms"] >= sample["ttft_ms"] >= 0
    assert sample["attempts"] == 1 and sample["error"] is None
    assert parse_dt(sample["completed_at"]) >= parse_dt(world.main["started_at"])
    assert sample["messages"] == world.prompts[0]["messages"]
    # The summary agrees with the detail.
    assert summaries[1]["response_preview"] == " ".join(sample["response"].split())[:160]


def test_thinking_is_split_out_of_inline_tags(api, world):
    s = list_samples(api, world.think["id"])[0]
    sample = api.get(f"/api/samples/{s['id']}").json()
    assert sample["reasoning"] and "</think>" not in sample["response"]
    assert "<think>" not in sample["reasoning"]
    assert s["has_reasoning"] is True


def test_plain_model_has_no_reasoning(api, world):
    for s in list_samples(api, world.plain["id"]):
        assert s["has_reasoning"] is False
    sample = api.get(f"/api/samples/{list_samples(api, world.plain['id'])[0]['id']}").json()
    assert not sample["reasoning"] and sample["response"]
    assert world.plain["reasoning_tokens"] == 0


def test_truncated_samples(api, world):
    run = world.trunc
    assert (run["done"], run["truncated"], run["failed"]) == (30, 30, 0)
    body = api.get(f"/api/runs/{run['id']}/samples", params={"status": "truncated"}).json()
    assert body["total"] == 30
    assert all(s["status"] == "done" and s["finish_reason"] == "length" for s in body["items"])
    # Truncated samples are also "done".
    assert api.get(f"/api/runs/{run['id']}/samples", params={"status": "done"}).json()["total"] == 30
    # And the other runs have none.
    assert api.get(f"/api/runs/{world.main['id']}/samples", params={"status": "truncated"}).json()["total"] == 0


# --------------------------------------------------------------------------- create


def test_create_with_defaults(api, make_endpoint, make_prompt_set, trash):
    ep = make_endpoint(name="Defaults endpoint")
    ps = make_prompt_set(3, name="Defaults prompts")
    r = api.post("/api/runs", json={"endpoint_id": ep["id"], "model": "mock-plain", "prompt_set_id": ps["id"]})
    assert r.status_code == 201, r.text
    run = r.json()
    check(run, RUN)
    trash.runs.append(run["id"])
    assert run["name"] == "mock-plain on Defaults prompts"
    assert run["params"] == DEFAULT_PARAMS
    check(run["params"], RUN_PARAMS)
    assert run["concurrency"] == 16 and run["total"] == 3
    assert run["status"] in ("running", "completed")
    final = wait_run(api, run["id"])
    assert final["done"] == 3


def test_create_name_rules(api, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Name endpoint")
    ps = make_prompt_set(2, name="Name prompts")
    named = make_run(ep["id"], ps["id"], name="  My run  ")
    assert named["name"] == "My run"
    blank = make_run(ep["id"], ps["id"], name="   ")
    assert blank["name"] == "mock-plain on Name prompts"


def test_create_keeps_params(api, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Params endpoint")
    ps = make_prompt_set(2, name="Params prompts")
    params = {
        "samples_per_prompt": 3,
        "temperature": 0.2,
        "top_p": 0.9,
        "max_tokens": 500,
        "max_tokens_field": "max_completion_tokens",
        "seed": 7,
        "system_prompt": "Be short.",
        "user_template": "Q: {prompt}",
        "extra_body": {"chat_template_kwargs": {"enable_thinking": True}},
        "stream": False,
        "think_open": "<t>",
        "think_close": "</t>",
        "max_attempts": 2,
        "request_timeout_s": 60,
        "pause_after_failures": 0,
    }
    run = make_run(ep["id"], ps["id"], concurrency=3, **params)
    assert run["total"] == 6 and run["concurrency"] == 3
    assert run["params"] == {**params, "request_timeout_s": 60.0}
    assert get_run(api, run["id"])["params"] == run["params"]
    final = wait_run(api, run["id"])
    assert final["done"] == 6


def test_explicit_null_clears_a_defaulted_param(api, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Null endpoint")
    ps = make_prompt_set(1, name="Null prompts")
    run = make_run(ep["id"], ps["id"], temperature=None, max_tokens=None, user_template="  ", system_prompt="")
    assert run["params"]["temperature"] is None and run["params"]["max_tokens"] is None
    # Blank strings mean "none". The Python backend echoes them back unchanged, so both are accepted.
    assert run["params"]["user_template"] in (None, "  ")
    assert run["params"]["system_prompt"] in (None, "")
    assert run["total"] == 1


def test_sample_messages_apply_system_prompt_and_template(api, world, make_endpoint, make_run):
    ep = make_endpoint(name="Template endpoint")
    run = make_run(
        ep["id"], world.prompt_set["id"], "mock-plain",
        system_prompt="You are terse.", user_template="Solve: {prompt}",
        concurrency=32,
    )
    wait_run(api, run["id"])
    first = list_samples(api, run["id"])[0]
    assert first["prompt_preview"] == "Task tag-000: compute 3 plus 40."  # the stored prompt, not the template
    sample = api.get(f"/api/samples/{first['id']}").json()
    assert sample["messages"] == [
        {"role": "system", "content": "You are terse."},  # replaces the prompt's own system message
        {"role": "user", "content": "Solve: Task tag-000: compute 3 plus 40."},
    ]
    # The stored prompt is untouched.
    assert api.get(f"/api/prompt-sets/{world.prompt_set['id']}/prompts").json()["items"][0]["messages"][0] == {
        "role": "system", "content": "You are a careful math tutor.",
    }


def test_create_validation(api, world):
    base = {"endpoint_id": world.endpoint["id"], "model": "mock-plain", "prompt_set_id": world.prompt_set["id"]}

    def post(**over):
        body = {**base, **{k: v for k, v in over.items() if k != "params"}}
        if "params" in over:
            body["params"] = over["params"]
        return api.post("/api/runs", json=body)

    assert_error(api.post("/api/runs", json={}), 422, any_field=("endpoint_id", "model", "prompt_set_id"))
    assert_error(post(model=""), 422, field="model")
    assert_error(post(concurrency=0), 422, field="concurrency")
    assert_error(post(concurrency=4097), 422, field="concurrency")
    assert_error(post(name="n" * 301), 422, field="name")
    for field, value in (
        ("samples_per_prompt", 0),
        ("samples_per_prompt", 65),
        ("temperature", 2.5),
        ("temperature", -0.1),
        ("top_p", 0),
        ("top_p", 1.5),
        ("max_tokens", 0),
        ("max_tokens_field", "max_new_tokens"),
        ("max_attempts", 0),
        ("max_attempts", 21),
        ("request_timeout_s", 5),
        ("pause_after_failures", -1),
    ):
        assert_error(post(params={field: value}), 422, field=f"params.{field}")
    r = post(params={"user_template": "no placeholder here"})
    assert_error(r, 422, field="params.user_template", detail="The prompt template must contain {prompt}")


def test_create_with_missing_references(api, world):
    r = api.post("/api/runs", json={"endpoint_id": 999999, "model": "m", "prompt_set_id": world.prompt_set["id"]})
    assert_error(r, 422, field="endpoint_id", detail="This endpoint does not exist. Pick another endpoint.")
    r = api.post("/api/runs", json={"endpoint_id": world.endpoint["id"], "model": "m", "prompt_set_id": 999999})
    assert_error(r, 422, field="prompt_set_id", detail="This prompt set does not exist. Pick another prompt set.")


# --------------------------------------------------------------------------- patch


def test_patch_run(api, world, make_endpoint, make_run):
    ep = make_endpoint(name="Patch endpoint")
    run = make_run(ep["id"], world.other_prompt_set["id"], "mock-plain", name="Before")
    wait_run(api, run["id"])
    r = api.patch(f"/api/runs/{run['id']}", json={"name": "  After  ", "concurrency": 5})
    assert r.status_code == 200, r.text
    out = r.json()
    check(out, RUN)
    assert out["name"] == "After" and out["concurrency"] == 5
    assert out["status"] == "completed" and out["done"] == 3
    assert get_run(api, run["id"])["name"] == "After"
    # Each field alone, and nothing at all.
    assert api.patch(f"/api/runs/{run['id']}", json={"concurrency": 9}).json()["name"] == "After"
    assert api.patch(f"/api/runs/{run['id']}", json={"name": "Third"}).json()["concurrency"] == 9
    assert api.patch(f"/api/runs/{run['id']}", json={}).json()["name"] == "Third"
    # A blank name is ignored.
    assert api.patch(f"/api/runs/{run['id']}", json={"name": "   "}).json()["name"] == "Third"


def test_patch_validation(api, world):
    url = f"/api/runs/{world.other['id']}"
    assert_error(api.patch(url, json={"name": ""}), 422, field="name")
    assert_error(api.patch(url, json={"name": "n" * 301}), 422, field="name")
    assert_error(api.patch(url, json={"concurrency": 0}), 422, field="concurrency")
    assert_error(api.patch(url, json={"concurrency": 4097}), 422, field="concurrency")


# --------------------------------------------------------------------------- finished runs


def test_steering_a_finished_run(api, world):
    rid = world.main["id"]
    assert_error(api.post(f"/api/runs/{rid}/pause"), 409, detail="This run has finished, so there is nothing to pause.")
    assert_error(
        api.post(f"/api/runs/{rid}/resume"), 409,
        detail="This run has finished, so there is nothing to resume. Retry failed samples or start a new run.",
    )
    # Nothing failed, so retrying changes nothing.
    r = api.post(f"/api/runs/{rid}/retry-failed")
    assert r.status_code == 200
    out = r.json()
    check(out, RUN)
    assert out["status"] == "completed" and out["done"] == 60 and out["failed"] == 0


# --------------------------------------------------------------------------- pause and resume


def test_pause_resume_and_change_concurrency(api, world, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Pause endpoint")
    ps = make_prompt_set(24, name="Pause prompts")
    run = make_run(ep["id"], ps["id"], "mock-reasoning-content", concurrency=2)
    assert run["status"] == "running"
    wait_until(lambda: get_run(api, run["id"])["done"] >= 2, timeout=30, what="two samples to finish")

    r = api.post(f"/api/runs/{run['id']}/pause")
    assert r.status_code == 200, r.text
    check(r.json(), RUN)
    assert r.json()["status"] in ("pausing", "paused")

    paused = wait_run(api, run["id"], "paused", timeout=30)
    assert paused["status_reason"] is None  # paused by the user, not by failures
    assert paused["finished_at"] is None
    assert 2 <= paused["done"] < 24 and paused["failed"] == 0
    assert paused["pending"] == paused["total"] - paused["done"]
    live = api.get(f"/api/runs/{run['id']}/live").json()
    check(live, RUN_LIVE)
    assert live["in_flight"] == 0 and live["sequences"] == []
    time.sleep(1.0)
    assert get_run(api, run["id"])["done"] == paused["done"], "a paused run makes no progress"
    # Pausing again is harmless.
    assert api.post(f"/api/runs/{run['id']}/pause").json()["status"] == "paused"
    # The not-yet-run samples are listed as pending.
    pending = api.get(f"/api/runs/{run['id']}/samples", params={"status": "pending"}).json()
    assert pending["total"] == 24 - paused["done"]
    assert all(s["status"] == "pending" and s["response_preview"] is None for s in pending["items"])

    r = api.post(f"/api/runs/{run['id']}/resume")
    assert r.status_code == 200, r.text
    assert r.json()["status"] == "running" and r.json()["status_reason"] is None
    out = api.patch(f"/api/runs/{run['id']}", json={"concurrency": 24}).json()
    assert out["concurrency"] == 24
    final = wait_run(api, run["id"], timeout=60)
    assert (final["done"], final["failed"], final["pending"]) == (24, 0, 0)
    assert final["active_seconds"] > paused["active_seconds"]
    assert final["finished_at"] is not None


def test_run_pauses_itself_after_failures_in_a_row(api, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Failing endpoint")
    ps = make_prompt_set(10, name="Failing prompts")
    run = make_run(ep["id"], ps["id"], "no-such-model", concurrency=1, pause_after_failures=3)
    paused = wait_run(api, run["id"], "paused", timeout=30)
    assert paused["failed"] >= 3 and paused["done"] == 0
    assert paused["status_reason"].startswith("Paused after 3 failures in a row. Last error: ")
    assert paused["failed"] < 10
    failed = api.get(f"/api/runs/{run['id']}/samples", params={"status": "failed"}).json()
    assert failed["total"] == paused["failed"]
    first = failed["items"][0]
    check(first, SAMPLE_SUMMARY)
    assert first["error"] and first["response_preview"] is None and first["has_reasoning"] is False
    detail = api.get(f"/api/samples/{first['id']}").json()
    check(detail, SAMPLE)
    assert detail["status"] == "failed" and detail["error"] == first["error"]
    assert detail["response"] is None and detail["attempts"] >= 1
    assert "no-such-model" in detail["error"] or "404" in detail["error"]


# --------------------------------------------------------------------------- flaky model


def test_flaky_run_and_retry_failed(api, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Flaky endpoint")
    ps = make_prompt_set(40, name="Flaky prompts")
    run = make_run(
        ep["id"], ps["id"], "mock-flaky", concurrency=16,
        max_attempts=1, pause_after_failures=0,
    )
    final = wait_run(api, run["id"], timeout=60)
    assert final["done"] + final["failed"] == 40
    assert final["failed"] > 0, "a 20% failure rate over 40 samples should fail some"
    assert final["status_reason"] is None
    failed = list_samples(api, run["id"], status="failed")
    assert len(failed) == final["failed"]
    assert all(s["error"] and s["response_preview"] is None for s in failed)

    for _ in range(12):
        before = get_run(api, run["id"])
        if before["failed"] == 0:
            break
        r = api.post(f"/api/runs/{run['id']}/retry-failed")
        assert r.status_code == 200, r.text
        out = r.json()
        check(out, RUN)
        assert out["failed"] < before["failed"], "retry-failed resets the failed samples at once"
        assert out["total"] == 40
        assert out["status"] in ("running", "completed")
        wait_run(api, run["id"], timeout=60)
    final = get_run(api, run["id"])
    assert (final["done"], final["failed"], final["pending"]) == (40, 0, 0)
    assert len(list_samples(api, run["id"], status="done")) == 40


def test_retries_hide_flaky_failures(api, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Retrying endpoint")
    ps = make_prompt_set(20, name="Retrying prompts")
    run = make_run(ep["id"], ps["id"], "mock-flaky", concurrency=20, max_attempts=6, pause_after_failures=0)
    final = wait_run(api, run["id"], timeout=90)
    assert final["done"] == 20, final
    attempts = [api.get(f"/api/samples/{s['id']}").json()["attempts"] for s in list_samples(api, run["id"])]
    assert min(attempts) >= 1 and max(attempts) <= 6


# --------------------------------------------------------------------------- delete


def test_delete_a_finished_run(api, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Delete endpoint")
    ps = make_prompt_set(3, name="Delete prompts")
    run = make_run(ep["id"], ps["id"], "mock-plain")
    wait_run(api, run["id"])
    sample_id = list_samples(api, run["id"])[0]["id"]
    assert_no_content(api.delete(f"/api/runs/{run['id']}"))
    assert_error(api.get(f"/api/runs/{run['id']}"), 404)
    assert_error(api.get(f"/api/runs/{run['id']}/samples"), 404)
    assert_error(api.get(f"/api/samples/{sample_id}"), 404)
    assert run["id"] not in [r["id"] for r in api.get("/api/runs").json()]
    assert api.get(f"/api/endpoints/{ep['id']}").json()["run_count"] == 0
    assert api.get(f"/api/prompt-sets/{ps['id']}").json()["run_count"] == 0


def test_a_running_run_blocks_deleting_its_endpoint_and_can_be_deleted(api, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Busy endpoint")
    ps = make_prompt_set(20, name="Busy prompts")
    run = make_run(ep["id"], ps["id"], "mock-reasoning-content", concurrency=1)
    r = api.delete(f"/api/endpoints/{ep['id']}")
    assert_error(r, 409, detail="A run is using this endpoint. Pause it before deleting the endpoint.")
    assert api.get(f"/api/endpoints/{ep['id']}").json()["run_count"] == 1
    # Deleting a running run stops it and removes it.
    assert_no_content(api.delete(f"/api/runs/{run['id']}"))
    assert_error(api.get(f"/api/runs/{run['id']}"), 404)
    assert_no_content(api.delete(f"/api/endpoints/{ep['id']}"))


def test_a_paused_run_does_not_block_deleting_its_endpoint(api, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Paused endpoint")
    ps = make_prompt_set(20, name="Paused prompts")
    run = make_run(ep["id"], ps["id"], "mock-reasoning-content", concurrency=1)
    wait_until(lambda: get_run(api, run["id"])["done"] >= 1, timeout=30, what="a sample to finish")
    api.post(f"/api/runs/{run['id']}/pause")
    wait_run(api, run["id"], "paused", timeout=30)
    assert_no_content(api.delete(f"/api/endpoints/{ep['id']}"))
    after = get_run(api, run["id"])
    assert after["endpoint_id"] is None
    assert after["endpoint_name"] == "Paused endpoint"  # the run remembers the name
    assert after["status"] == "paused"
    # It cannot continue without its endpoint.
    r = api.post(f"/api/runs/{run['id']}/resume")
    assert_error(r, 409, contains="endpoint")
    assert_no_content(api.delete(f"/api/runs/{run['id']}"))
