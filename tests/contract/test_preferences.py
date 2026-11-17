"""Preference sets: create, items, pairs, auto-pair, export."""

from __future__ import annotations

import time

import pytest

from exportkit import assert_download_headers, assert_same_records, parse_download
from shapes import (
    AUTO_PAIR,
    EXPORT_PREVIEW,
    PREFERENCE_ITEM,
    PREFERENCE_SET,
    ListOf,
    check,
    page,
)
from support import (
    assert_error,
    assert_no_content,
    list_samples,
    wait_run,
)

N = 30


def think_wrap(reasoning: str | None, response: str | None) -> str:
    reasoning = (reasoning or "").strip("\n")
    response = response or ""
    return f"<think>\n{reasoning}\n</think>\n\n{response}" if reasoning.strip() else response


@pytest.fixture
def pset(api, world, trash):
    """A fresh preference set over the main run (2 samples per prompt) and the think run (1)."""
    r = api.post(
        "/api/preference-sets",
        json={"name": f"Prefs {time.time_ns()}", "run_ids": [world.main["id"], world.think["id"]]},
    )
    assert r.status_code == 201, r.text
    ps = r.json()
    trash.pref_sets.append(ps["id"])
    return ps


def items(api, set_id: int, **params) -> dict:
    r = api.get(f"/api/preference-sets/{set_id}/items", params={"limit": 200, **params})
    assert r.status_code == 200, r.text
    body = r.json()
    check(body, page(PREFERENCE_ITEM))
    return body


def sample_ids(api, run_id: int) -> dict[tuple[int, int], int]:
    return {(s["prompt_idx"], s["sample_idx"]): s["id"] for s in list_samples(api, run_id)}


# --------------------------------------------------------------------------- create, get, list


def test_create_preference_set(api, world, pset):
    check(pset, PREFERENCE_SET)
    assert pset["prompt_set_id"] == world.prompt_set["id"]
    assert pset["prompt_set_name"] == "Contract prompts"
    assert pset["run_ids"] == [world.main["id"], world.think["id"]]
    assert pset["runs"] == [
        {"id": world.main["id"], "name": "Contract Main Run", "model": "mock-reasoning-content"},
        {"id": world.think["id"], "name": "Contract Think Run", "model": "mock-think-tags"},
    ]
    assert pset["prompt_count"] == N
    assert pset["decided"] == 0
    assert api.get(f"/api/preference-sets/{pset['id']}").json() == pset


def test_create_trims_the_name_and_drops_repeated_runs(api, world, trash):
    r = api.post(
        "/api/preference-sets",
        json={"name": "  Repeats  ", "run_ids": [world.think["id"], world.main["id"], world.think["id"]]},
    )
    assert r.status_code == 201, r.text
    ps = r.json()
    trash.pref_sets.append(ps["id"])
    assert ps["name"] == "Repeats"
    assert ps["run_ids"] == [world.think["id"], world.main["id"]]
    assert [x["id"] for x in ps["runs"]] == ps["run_ids"]


def test_one_run_with_two_samples_is_enough(api, world, trash):
    r = api.post("/api/preference-sets", json={"name": "Single run", "run_ids": [world.main["id"]]})
    ps = r.json()
    trash.pref_sets.append(ps["id"])
    assert ps["prompt_count"] == N
    r = api.post("/api/preference-sets", json={"name": "Not enough", "run_ids": [world.think["id"]]})
    assert r.status_code == 201
    trash.pref_sets.append(r.json()["id"])
    assert r.json()["prompt_count"] == 0  # one finished sample per prompt cannot be paired


def test_list_preference_sets_newest_first(api, world, trash):
    ids = []
    for name in ("List one", "List two"):
        ps = api.post("/api/preference-sets", json={"name": name, "run_ids": [world.main["id"]]}).json()
        trash.pref_sets.append(ps["id"])
        ids.append(ps["id"])
        time.sleep(0.02)
    r = api.get("/api/preference-sets")
    assert r.status_code == 200
    listed = r.json()
    check(listed, ListOf(PREFERENCE_SET))
    order = [p["id"] for p in listed]
    assert order.index(ids[1]) < order.index(ids[0])


def test_create_validation(api, world):
    main = world.main["id"]

    def post(**body):
        return api.post("/api/preference-sets", json=body)

    assert_error(post(), 422, any_field=("name", "run_ids"))
    assert_error(post(name="", run_ids=[main]), 422, field="name")
    assert_error(post(name="n" * 301, run_ids=[main]), 422, field="name")
    assert_error(post(name="x", run_ids=[]), 422, field="run_ids")
    assert_error(post(name="   ", run_ids=[main]), 422, field="name")
    assert_error(
        post(name="x", run_ids=[main, 999999]), 422,
        detail="One of the chosen runs no longer exists. Refresh the list and choose again.",
    )
    assert_error(
        post(name="x", run_ids=[main, world.other["id"]]), 422, detail="All runs must use the same prompt set."
    )


def test_unknown_preference_set(api):
    gone = "Preference set not found. It may have been deleted."
    for method, path, kw in (
        ("get", "/api/preference-sets/999999", {}),
        ("delete", "/api/preference-sets/999999", {}),
        ("get", "/api/preference-sets/999999/items", {}),
        ("put", "/api/preference-sets/999999/pairs/1", {"json": {"chosen_sample_id": 1, "rejected_sample_id": 2}}),
        ("delete", "/api/preference-sets/999999/pairs/1", {}),
        ("post", "/api/preference-sets/999999/auto-pair", {"json": {"chosen_run_id": 1, "rejected_run_id": 2}}),
    ):
        assert_error(getattr(api, method)(path, **kw), 404, detail=gone)
    # The export routes use a shorter sentence.
    assert_error(api.post("/api/preference-sets/999999/export/preview", json={}), 404, detail="Preference set not found")
    assert_error(api.get("/api/preference-sets/999999/export"), 404, detail="Preference set not found")
    assert_error(
        api.post("/api/preference-sets/999999/export/hub", json={"target": {"repo_id": "a/b"}}),
        404, detail="Preference set not found",
    )


def test_delete_preference_set(api, world):
    ps = api.post("/api/preference-sets", json={"name": "Doomed", "run_ids": [world.main["id"]]}).json()
    assert_no_content(api.delete(f"/api/preference-sets/{ps['id']}"))
    assert_error(api.get(f"/api/preference-sets/{ps['id']}"), 404)
    assert ps["id"] not in [p["id"] for p in api.get("/api/preference-sets").json()]
    assert api.get(f"/api/runs/{world.main['id']}").status_code == 200  # the runs stay


# --------------------------------------------------------------------------- items


def test_items(api, world, pset):
    body = items(api, pset["id"], limit=20)
    assert (body["total"], body["offset"], body["limit"]) == (N, 0, 20)
    assert [i["prompt_idx"] for i in body["items"]] == list(range(20))
    first = body["items"][0]
    assert first["prompt_id"] == world.prompts[0]["id"]
    assert first["messages"] == world.prompts[0]["messages"]
    assert first["chosen_sample_id"] is None and first["rejected_sample_id"] is None and first["source"] is None
    # Candidates: every finished sample, runs in the order of the set, then by sample index.
    cands = first["candidates"]
    assert [(c["run_id"], c["sample_idx"]) for c in cands] == [
        (world.main["id"], 0), (world.main["id"], 1), (world.think["id"], 0),
    ]
    assert cands[0]["run_name"] == "Contract Main Run" and cands[0]["model"] == "mock-reasoning-content"
    assert cands[2]["model"] == "mock-think-tags"
    ids = sample_ids(api, world.main["id"])
    assert cands[0]["sample_id"] == ids[(0, 0)] and cands[1]["sample_id"] == ids[(0, 1)]
    for c in cands:
        assert c["response"] and c["reasoning"] and c["finish_reason"] == "stop"
        assert c["completion_tokens"] > 0
    # Only a server that reports reasoning separately gives a reasoning token count.
    assert cands[0]["reasoning_tokens"] > 0 and cands[2]["reasoning_tokens"] is None


def test_items_paging_and_bounds(api, pset):
    body = items(api, pset["id"], offset=25, limit=10)
    assert [i["prompt_idx"] for i in body["items"]] == [25, 26, 27, 28, 29]
    assert body["total"] == N
    assert items(api, pset["id"], offset=100)["items"] == []
    default = api.get(f"/api/preference-sets/{pset['id']}/items").json()
    assert default["limit"] == 20 and len(default["items"]) == 20
    url = f"/api/preference-sets/{pset['id']}/items"
    assert_error(api.get(url, params={"limit": 0}), 422, field="limit")
    assert_error(api.get(url, params={"limit": 201}), 422, field="limit")
    assert_error(api.get(url, params={"offset": -1}), 422, field="offset")
    assert_error(api.get(url, params={"filter": "maybe"}), 422, field="filter")


# --------------------------------------------------------------------------- pairs


def test_pair_a_prompt_by_hand(api, world, pset):
    main_ids, think_ids = sample_ids(api, world.main["id"]), sample_ids(api, world.think["id"])
    url = f"/api/preference-sets/{pset['id']}/pairs/{world.prompts[3]['id']}"
    r = api.put(url, json={"chosen_sample_id": main_ids[(3, 0)], "rejected_sample_id": think_ids[(3, 0)]})
    assert r.status_code == 200, r.text
    item = r.json()
    check(item, PREFERENCE_ITEM)
    assert item["prompt_idx"] == 3 and len(item["candidates"]) == 3
    assert item["chosen_sample_id"] == main_ids[(3, 0)] and item["rejected_sample_id"] == think_ids[(3, 0)]
    assert item["source"] == "manual"

    assert api.get(f"/api/preference-sets/{pset['id']}").json()["decided"] == 1
    assert items(api, pset["id"], filter="decided")["total"] == 1
    undecided = items(api, pset["id"], filter="undecided")
    assert undecided["total"] == N - 1 and 3 not in [i["prompt_idx"] for i in undecided["items"]]
    assert items(api, pset["id"], filter="all")["total"] == N
    listed = next(i for i in items(api, pset["id"])["items"] if i["prompt_idx"] == 3)
    assert listed == item

    # Changing the pair replaces it; there is still one pair for the prompt.
    r = api.put(url, json={"chosen_sample_id": think_ids[(3, 0)], "rejected_sample_id": main_ids[(3, 1)]})
    assert r.json()["chosen_sample_id"] == think_ids[(3, 0)]
    assert api.get(f"/api/preference-sets/{pset['id']}").json()["decided"] == 1

    # Removing it (twice is fine).
    assert_no_content(api.delete(url))
    assert_no_content(api.delete(url))
    assert api.get(f"/api/preference-sets/{pset['id']}").json()["decided"] == 0
    assert items(api, pset["id"], filter="decided")["total"] == 0


def test_pair_errors(api, world, pset):
    main_ids, think_ids = sample_ids(api, world.main["id"]), sample_ids(api, world.think["id"])
    other_ids = sample_ids(api, world.other["id"])
    p0 = world.prompts[0]["id"]
    url = f"/api/preference-sets/{pset['id']}/pairs/{p0}"
    a, b = main_ids[(0, 0)], think_ids[(0, 0)]

    assert_error(api.put(url, json={"chosen_sample_id": a, "rejected_sample_id": a}), 422)
    assert_error(api.put(url, json={}), 422, any_field=("chosen_sample_id", "rejected_sample_id"))
    assert_error(
        api.put(url, json={"chosen_sample_id": 999999, "rejected_sample_id": b}), 422,
        detail="The chosen response does not exist. Reload the prompt and pick again.",
    )
    assert_error(
        api.put(url, json={"chosen_sample_id": a, "rejected_sample_id": 999999}), 422,
        detail="The rejected response does not exist. Reload the prompt and pick again.",
    )
    assert_error(
        api.put(url, json={"chosen_sample_id": a, "rejected_sample_id": other_ids[(0, 0)]}), 422,
        detail="The rejected response comes from a run that is not in this preference set.",
    )
    assert_error(
        api.put(url, json={"chosen_sample_id": a, "rejected_sample_id": main_ids[(1, 0)]}), 422,
        detail="The rejected response answers a different prompt. Reload and pick again.",
    )
    assert_error(
        api.put(f"/api/preference-sets/{pset['id']}/pairs/999999", json={"chosen_sample_id": a, "rejected_sample_id": b}),
        404, detail="Prompt not found in this preference set's prompt set.",
    )
    other_prompt = api.get(f"/api/prompt-sets/{world.other_prompt_set['id']}/prompts").json()["items"][0]["id"]
    assert_error(
        api.put(f"/api/preference-sets/{pset['id']}/pairs/{other_prompt}", json={"chosen_sample_id": a, "rejected_sample_id": b}),
        404, detail="Prompt not found in this preference set's prompt set.",
    )
    assert api.get(f"/api/preference-sets/{pset['id']}").json()["decided"] == 0


# --------------------------------------------------------------------------- auto-pair


def test_auto_pair(api, world, pset):
    main_ids, think_ids = sample_ids(api, world.main["id"]), sample_ids(api, world.think["id"])
    url = f"/api/preference-sets/{pset['id']}/auto-pair"
    body = {"chosen_run_id": world.main["id"], "rejected_run_id": world.think["id"]}
    r = api.post(url, json=body)
    assert r.status_code == 200, r.text
    check(r.json(), AUTO_PAIR)
    assert r.json() == {"created": N, "skipped": 0}
    assert api.get(f"/api/preference-sets/{pset['id']}").json()["decided"] == N
    for item in items(api, pset["id"])["items"]:
        i = item["prompt_idx"]
        assert item["source"] == "rule"
        assert item["chosen_sample_id"] == main_ids[(i, 0)], "the run's first finished sample"
        assert item["rejected_sample_id"] == think_ids[(i, 0)]

    # A pair made by hand survives a second auto-pair unless overwrite is set.
    p5 = next(i for i in items(api, pset["id"])["items"] if i["prompt_idx"] == 5)
    r = api.put(
        f"/api/preference-sets/{pset['id']}/pairs/{p5['prompt_id']}",
        json={"chosen_sample_id": main_ids[(5, 1)], "rejected_sample_id": main_ids[(5, 0)]},
    )
    assert r.json()["source"] == "manual"
    assert api.post(url, json=body).json() == {"created": N - 1, "skipped": 1}
    kept = next(i for i in items(api, pset["id"])["items"] if i["prompt_idx"] == 5)
    assert kept["source"] == "manual" and kept["chosen_sample_id"] == main_ids[(5, 1)]
    assert api.post(url, json={**body, "overwrite": True}).json() == {"created": N, "skipped": 0}
    redone = next(i for i in items(api, pset["id"])["items"] if i["prompt_idx"] == 5)
    assert redone["source"] == "rule" and redone["chosen_sample_id"] == main_ids[(5, 0)]
    # The other direction.
    assert api.post(url, json={"chosen_run_id": world.think["id"], "rejected_run_id": world.main["id"], "overwrite": True}).json() == {"created": N, "skipped": 0}
    flipped = items(api, pset["id"])["items"][0]
    assert flipped["chosen_sample_id"] == think_ids[(0, 0)] and flipped["rejected_sample_id"] == main_ids[(0, 0)]


def test_auto_pair_errors(api, world, pset):
    url = f"/api/preference-sets/{pset['id']}/auto-pair"
    main, think = world.main["id"], world.think["id"]
    assert_error(api.post(url, json={"chosen_run_id": main, "rejected_run_id": main}), 422)
    assert_error(
        api.post(url, json={"chosen_run_id": main, "rejected_run_id": world.plain["id"]}), 422,
        detail="The rejected run is not in this preference set.",
    )
    assert_error(
        api.post(url, json={"chosen_run_id": world.plain["id"], "rejected_run_id": think}), 422,
        detail="The chosen run is not in this preference set.",
    )
    assert_error(api.post(url, json={}), 422, any_field=("chosen_run_id", "rejected_run_id"))
    assert api.get(f"/api/preference-sets/{pset['id']}").json()["decided"] == 0


def test_deleting_a_run_removes_it_from_the_set(api, make_endpoint, make_prompt_set, make_run, trash):
    ep = make_endpoint(name="Prefs endpoint")
    ps = make_prompt_set(4, name="Prefs prompts")
    a = make_run(ep["id"], ps["id"], "mock-plain", name="Pref run A", samples_per_prompt=2)
    b = make_run(ep["id"], ps["id"], "mock-plain", name="Pref run B")
    wait_run(api, a["id"])
    wait_run(api, b["id"])
    pref = api.post("/api/preference-sets", json={"name": "Shrinking", "run_ids": [a["id"], b["id"]]}).json()
    trash.pref_sets.append(pref["id"])
    assert pref["prompt_count"] == 4
    assert_no_content(api.delete(f"/api/runs/{b['id']}"))
    after = api.get(f"/api/preference-sets/{pref['id']}").json()
    check(after, PREFERENCE_SET)
    assert after["run_ids"] == [a["id"]] and [r["id"] for r in after["runs"]] == [a["id"]]
    assert after["prompt_count"] == 4  # run A alone still has two samples per prompt
    assert all(len(i["candidates"]) == 2 for i in items(api, pref["id"])["items"])


# --------------------------------------------------------------------------- export


def pref_preview(api, set_id: int, **options) -> dict:
    r = api.post(f"/api/preference-sets/{set_id}/export/preview", json=options)
    assert r.status_code == 200, r.text
    out = r.json()
    check(out, EXPORT_PREVIEW)
    return out


def pref_download(api, set_id: int, **options):
    q = {k: ("true" if v is True else "false" if v is False else v) for k, v in options.items()}
    return api.get(f"/api/preference-sets/{set_id}/export", params=q)


@pytest.fixture
def paired(api, world, pset):
    """The set with every prompt paired main(sample 0) over think(sample 0)."""
    r = api.post(
        f"/api/preference-sets/{pset['id']}/auto-pair",
        json={"chosen_run_id": world.main["id"], "rejected_run_id": world.think["id"]},
    )
    assert r.json()["created"] == N
    return pset


def sample_detail(api, sid: int) -> dict:
    return api.get(f"/api/samples/{sid}").json()


def test_export_of_an_empty_preference_set(api, pset):
    out = pref_preview(api, pset["id"])
    assert out["count"] == 0 and out["skipped"] == {} and out["example"] is None
    assert out["filename"].endswith("-dpo.jsonl")
    r = pref_download(api, pset["id"])
    assert_download_headers(r, "jsonl", out["filename"])
    assert r.content == b""


def test_dpo_conversational_export(api, world, paired):
    out = pref_preview(api, paired["id"])
    assert out["count"] == N and out["skipped"] == {}
    assert out["filename"] == f"{paired['name'].lower().replace(' ', '-')}-dpo.jsonl"
    ids = sample_ids(api, world.main["id"]), sample_ids(api, world.think["id"])
    chosen, rejected = sample_detail(api, ids[0][(0, 0)]), sample_detail(api, ids[1][(0, 0)])
    assert out["example"] == {
        "prompt": world.prompts[0]["messages"],
        "chosen": [{"role": "assistant", "content": think_wrap(chosen["reasoning"], chosen["response"])}],
        "rejected": [{"role": "assistant", "content": think_wrap(rejected["reasoning"], rejected["response"])}],
    }
    r = pref_download(api, paired["id"])
    assert_download_headers(r, "jsonl", out["filename"])
    records, _ = parse_download("jsonl", r.content)
    assert len(records) == N and records[0] == out["example"]
    assert [rec["prompt"][1]["content"] for rec in records][:3] == [
        f"Task tag-{i:03d}: compute {3 + i} plus {40 - i}." for i in range(3)
    ]


def test_dpo_text_export_and_reasoning_placement(api, world, paired):
    ids = sample_ids(api, world.main["id"]), sample_ids(api, world.think["id"])
    chosen, rejected = sample_detail(api, ids[0][(0, 0)]), sample_detail(api, ids[1][(0, 0)])
    system, user = (m["content"] for m in world.prompts[0]["messages"])

    out = pref_preview(api, paired["id"], conversational=False)
    assert out["example"] == {
        "prompt": f"{system}\n\n{user}",
        "chosen": think_wrap(chosen["reasoning"], chosen["response"]),
        "rejected": think_wrap(rejected["reasoning"], rejected["response"]),
    }
    out = pref_preview(api, paired["id"], conversational=False, reasoning="reasoning_content", include_system=False)
    assert out["example"] == {
        "prompt": user,
        "chosen": chosen["response"],
        "rejected": rejected["response"],
        "chosen_reasoning": chosen["reasoning"],
        "rejected_reasoning": rejected["reasoning"],
    }
    out = pref_preview(api, paired["id"], reasoning="thinking")
    assert out["example"]["chosen"] == [{"role": "assistant", "content": chosen["response"], "thinking": chosen["reasoning"]}]
    out = pref_preview(api, paired["id"], reasoning="omit")
    assert out["example"]["rejected"] == [{"role": "assistant", "content": rejected["response"]}]


def test_kto_export_doubles_the_records(api, world, paired):
    ids = sample_ids(api, world.main["id"]), sample_ids(api, world.think["id"])
    chosen, rejected = sample_detail(api, ids[0][(0, 0)]), sample_detail(api, ids[1][(0, 0)])
    out = pref_preview(api, paired["id"], format="kto")
    assert out["count"] == 2 * N
    assert out["filename"].endswith("-kto.jsonl")
    prompt = world.prompts[0]["messages"]
    assert out["example"] == {
        "prompt": prompt,
        "completion": [{"role": "assistant", "content": think_wrap(chosen["reasoning"], chosen["response"])}],
        "label": True,
    }
    records, _ = parse_download("jsonl", pref_download(api, paired["id"], format="kto").content)
    assert len(records) == 2 * N
    assert [r["label"] for r in records[:4]] == [True, False, True, False]
    assert records[1]["completion"][0]["content"] == think_wrap(rejected["reasoning"], rejected["response"])

    text = pref_preview(api, paired["id"], format="kto", conversational=False, reasoning="reasoning_content", include_system=False)
    assert text["example"] == {
        "prompt": prompt[1]["content"],
        "completion": chosen["response"],
        "label": True,
        "reasoning": chosen["reasoning"],
    }


@pytest.mark.parametrize("options", [
    {"format": "dpo"},
    {"format": "dpo", "conversational": False},
    {"format": "dpo", "conversational": False, "reasoning": "thinking"},
    {"format": "kto"},
    {"format": "kto", "conversational": False, "reasoning": "reasoning_content"},
])
def test_pref_file_types_carry_the_same_records(api, paired, options):
    base = pref_download(api, paired["id"], **options)
    want, _ = parse_download("jsonl", base.content)
    assert want
    for ft in ("json", "csv", "parquet"):
        r = pref_download(api, paired["id"], file_type=ft, **options)
        pv = pref_preview(api, paired["id"], file_type=ft, **options)
        assert_download_headers(r, ft, pv["filename"])
        got, cols = parse_download(ft, r.content)
        assert_same_records(want, ft, got, cols)
    # kto labels are real booleans in parquet and "true"/"false" in csv.
    if options["format"] == "kto":
        rows, _ = parse_download("parquet", pref_download(api, paired["id"], file_type="parquet", **options).content)
        assert [r["label"] for r in rows[:2]] == [True, False]
        rows, _ = parse_download("csv", pref_download(api, paired["id"], file_type="csv", **options).content)
        assert [r["label"] for r in rows[:2]] == ["true", "false"]


def test_pref_export_validation(api, paired):
    url = f"/api/preference-sets/{paired['id']}/export"
    assert_error(api.get(url, params={"format": "sft"}), 422, field="format")
    assert_error(api.get(url, params={"file_type": "xml"}), 422, field="file_type")
    assert_error(api.get(url, params={"conversational": "maybe"}), 422, field="conversational")
    assert_error(api.post(url + "/preview", json={"format": "alpaca"}), 422, field="format")


def test_pref_hub_errors(api, paired, hf_token_cleared):
    url = f"/api/preference-sets/{paired['id']}/export/hub"
    body = {"target": {"repo_id": "someone/dataset"}}
    assert_error(api.post(url, json=body), 400, detail="Add a Hugging Face token in Settings to push datasets.")
    assert_error(api.post(url, json={"target": {"repo_id": "x"}}), 422, field="target.repo_id")
    api.put("/api/settings", json={"hf_token": "hf_not_a_real_token_0000"})
    r = api.post(url, json={"target": {"repo_id": "someone/dataset", "split": "no good"}})
    assert_error(r, 400, detail="The split name can only use letters, numbers, dots, dashes and underscores.")


def test_pref_hub_with_nothing_to_push(api, pset, hf_token_cleared):
    api.put("/api/settings", json={"hf_token": "hf_not_a_real_token_0000"})
    r = api.post(f"/api/preference-sets/{pset['id']}/export/hub", json={"target": {"repo_id": "someone/dataset"}})
    assert_error(r, 400, detail="There are no records to push with these options.")
