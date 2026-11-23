"""/api/prompt-sets: list, get, rename, prompts (search, pages), delete and cascades, stop."""

from __future__ import annotations

import time


from shapes import PROMPT, PROMPT_SET, ListOf, check, page
from support import assert_error, assert_no_content, create_run, wait_run


def test_get_a_prompt_set(api, world):
    r = api.get(f"/api/prompt-sets/{world.prompt_set['id']}")
    assert r.status_code == 200
    ps = r.json()
    check(ps, PROMPT_SET)
    assert ps["name"] == "Contract prompts"
    assert ps["source_kind"] == "file"
    assert ps["prompt_count"] == 30
    assert ps["run_count"] == world.runs_on_prompt_set
    assert ps["source"]["filename"] == "contract-prompts.jsonl"
    assert ps["source"]["prompt_field"] == "messages"


def test_list_prompt_sets_newest_first(api, make_prompt_set):
    a = make_prompt_set(2, name="List A")
    time.sleep(0.02)
    b = make_prompt_set(2, name="List B")
    r = api.get("/api/prompt-sets")
    assert r.status_code == 200
    items = r.json()
    check(items, ListOf(PROMPT_SET))
    ids = [p["id"] for p in items]
    assert ids.index(b["id"]) < ids.index(a["id"])
    assert next(p for p in items if p["id"] == a["id"])["prompt_count"] == 2


def test_unknown_prompt_set(api):
    for method, path in (
        ("get", "/api/prompt-sets/999999"),
        ("get", "/api/prompt-sets/999999/prompts"),
        ("delete", "/api/prompt-sets/999999"),
    ):
        assert_error(getattr(api, method)(path), 404, detail="Prompt set not found")
    assert_error(api.patch("/api/prompt-sets/999999", json={"name": "x"}), 404, detail="Prompt set not found")
    assert_error(api.get("/api/prompt-sets/abc"), 422)


def test_rename(api, make_prompt_set):
    ps = make_prompt_set(2, name="Before")
    r = api.patch(f"/api/prompt-sets/{ps['id']}", json={"name": "After"})
    assert r.status_code == 200
    out = r.json()
    check(out, PROMPT_SET)
    assert out["name"] == "After" and out["id"] == ps["id"] and out["prompt_count"] == 2
    assert api.get(f"/api/prompt-sets/{ps['id']}").json()["name"] == "After"
    assert next(p for p in api.get("/api/prompt-sets").json() if p["id"] == ps["id"])["name"] == "After"


def test_rename_validation(api, make_prompt_set):
    ps = make_prompt_set(2, name="Keep")
    url = f"/api/prompt-sets/{ps['id']}"
    assert_error(api.patch(url, json={"name": ""}), 422, field="name")
    assert_error(api.patch(url, json={"name": "n" * 301}), 422, field="name")
    assert_error(api.patch(url, json={}), 422, field="name")
    assert_error(api.patch(url, json={"name": 5}), 422, field="name")
    assert api.get(url).json()["name"] == "Keep"
    assert api.patch(url, json={"name": "n" * 300}).status_code == 200


def test_prompts_page(api, world):
    url = f"/api/prompt-sets/{world.prompt_set['id']}/prompts"
    r = api.get(url)
    assert r.status_code == 200
    body = r.json()
    check(body, page(PROMPT))
    assert (body["total"], body["offset"], body["limit"]) == (30, 0, 50)
    assert [p["idx"] for p in body["items"]] == list(range(30))
    first = body["items"][0]
    assert first["messages"] == [
        {"role": "system", "content": "You are a careful math tutor."},
        {"role": "user", "content": "Task tag-000: compute 3 plus 40."},
    ]
    assert first["meta"] == {"category": "even"}
    assert body["items"][1]["meta"] == {"category": "odd"}


def test_prompts_pagination(api, world):
    url = f"/api/prompt-sets/{world.prompt_set['id']}/prompts"
    body = api.get(url, params={"offset": 10, "limit": 5}).json()
    assert (body["total"], body["offset"], body["limit"]) == (30, 10, 5)
    assert [p["idx"] for p in body["items"]] == [10, 11, 12, 13, 14]
    body = api.get(url, params={"offset": 28, "limit": 10}).json()
    assert [p["idx"] for p in body["items"]] == [28, 29]
    body = api.get(url, params={"offset": 500}).json()
    assert body["items"] == [] and body["total"] == 30
    # Pages do not overlap and cover everything.
    ids = []
    for off in range(0, 30, 7):
        ids += [p["id"] for p in api.get(url, params={"offset": off, "limit": 7}).json()["items"]]
    assert len(ids) == 30 == len(set(ids))


def test_prompts_page_bounds(api, world):
    url = f"/api/prompt-sets/{world.prompt_set['id']}/prompts"
    assert api.get(url, params={"limit": 500}).status_code == 200
    assert_error(api.get(url, params={"limit": 0}), 422, field="limit")
    assert_error(api.get(url, params={"limit": 501}), 422, field="limit")
    assert_error(api.get(url, params={"offset": -1}), 422, field="offset")
    assert_error(api.get(url, params={"limit": "many"}), 422, field="limit")


def test_prompts_search(api, world):
    url = f"/api/prompt-sets/{world.prompt_set['id']}/prompts"
    body = api.get(url, params={"q": "tag-007"}).json()
    assert body["total"] == 1 and [p["idx"] for p in body["items"]] == [7]
    # Case-insensitive.
    assert api.get(url, params={"q": "TAG-007"}).json()["total"] == 1
    assert api.get(url, params={"q": "Careful Math"}).json()["total"] == 30
    assert api.get(url, params={"q": "no such text anywhere"}).json() == {
        "items": [], "total": 0, "offset": 0, "limit": 50,
    }
    # Search and paging combine: total counts every match, items are one page.
    body = api.get(url, params={"q": "compute", "offset": 25, "limit": 10}).json()
    assert body["total"] == 30 and [p["idx"] for p in body["items"]] == [25, 26, 27, 28, 29]
    # An empty query matches everything.
    assert api.get(url, params={"q": ""}).json()["total"] == 30


def test_delete_a_prompt_set(api, make_prompt_set):
    ps = make_prompt_set(3, name="Delete me")
    assert_no_content(api.delete(f"/api/prompt-sets/{ps['id']}"))
    assert_error(api.get(f"/api/prompt-sets/{ps['id']}"), 404)
    assert_error(api.get(f"/api/prompt-sets/{ps['id']}/prompts"), 404)
    assert ps["id"] not in [p["id"] for p in api.get("/api/prompt-sets").json()]
    assert_error(api.delete(f"/api/prompt-sets/{ps['id']}"), 404)


def test_stop_keeps_a_set_whose_import_finished(api, make_prompt_set):
    # "Stop and delete" on an import that has finished deletes nothing: the set is kept.
    ps = make_prompt_set(3, name="Finished import")
    r = api.post(f"/api/prompt-sets/{ps['id']}/stop")
    assert_error(r, 409, detail="This import already finished, so the prompt set was kept.")
    kept = api.get(f"/api/prompt-sets/{ps['id']}").json()
    assert kept["status"] == "ready" and kept["prompt_count"] == 3
    assert_error(api.post("/api/prompt-sets/999999/stop"), 404, detail="Prompt set not found")


def test_delete_cascades_to_runs_samples_and_preference_sets(api, make_endpoint, make_prompt_set):
    ep = make_endpoint(name="Cascade")
    ps = make_prompt_set(3, name="Cascade set")
    runs = [create_run(api, ep["id"], ps["id"], "mock-plain", name=f"Cascade run {i}") for i in range(2)]
    for run in runs:
        wait_run(api, run["id"])
    sample_id = api.get(f"/api/runs/{runs[0]['id']}/samples").json()["items"][0]["id"]
    pref = api.post(
        "/api/preference-sets", json={"name": "Cascade prefs", "run_ids": [r["id"] for r in runs]}
    ).json()
    assert api.get(f"/api/endpoints/{ep['id']}").json()["run_count"] == 2

    assert_no_content(api.delete(f"/api/prompt-sets/{ps['id']}"))
    for run in runs:
        assert_error(api.get(f"/api/runs/{run['id']}"), 404, detail="Run not found")
    assert_error(api.get(f"/api/samples/{sample_id}"), 404, detail="Sample not found")
    assert_error(api.get(f"/api/preference-sets/{pref['id']}"), 404)
    assert api.get(f"/api/endpoints/{ep['id']}").json()["run_count"] == 0


def test_cannot_delete_a_prompt_set_while_a_run_is_going(api, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Busy")
    ps = make_prompt_set(20, name="Busy set")
    run = make_run(ep["id"], ps["id"], "mock-reasoning-content", concurrency=1)
    assert run["status"] == "running"
    r = api.delete(f"/api/prompt-sets/{ps['id']}")
    assert_error(r, 409, detail="A run on this prompt set is still going. Pause it first.")
    # Deleting the run frees the set.
    assert_no_content(api.delete(f"/api/runs/{run['id']}"))
    assert_no_content(api.delete(f"/api/prompt-sets/{ps['id']}"))
