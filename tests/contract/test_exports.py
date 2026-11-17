"""Run exports: preview, every format in every file type, options, empty exports and Hub errors."""

from __future__ import annotations

import json

import pytest

from exportkit import FILE_TYPES, assert_download_headers, assert_same_records, parse_download
from shapes import EXPORT_PREVIEW, Obj, Opt, DT, check
from support import assert_error, create_endpoint, create_run, import_prompts, list_samples, slugify, source_paste, wait_run

SFT_FORMATS = ["messages", "sharegpt", "alpaca", "prompt_completion", "raw"]
PLACEMENTS = ["think_tags", "reasoning_content", "thinking", "omit"]

RAW_RECORD = Obj(
    {
        "id": int,
        "run_id": int,
        "prompt_id": int,
        "prompt_index": int,
        "sample_index": int,
        "model": str,
        "endpoint": str,
        "messages": list,
        "reasoning": str,
        "response": str,
        "finish_reason": Opt(str),
        "usage": Obj({"prompt_tokens": Opt(int), "completion_tokens": Opt(int), "reasoning_tokens": Opt(int)}),
        "latency_ms": Opt(int),
        "ttft_ms": Opt(int),
        "params": dict,
        "meta": dict,
        "created_at": Opt(DT),
    }
)


def think_wrap(reasoning: str, response: str) -> str:
    reasoning = reasoning.strip("\n")
    return f"<think>\n{reasoning}\n</think>\n\n{response}" if reasoning.strip() else response


def bools(options: dict) -> dict:
    """Query-string forms of the options."""
    return {k: ("true" if v is True else "false" if v is False else v) for k, v in options.items()}


def preview(api, run_id: int, **options) -> dict:
    r = api.post(f"/api/runs/{run_id}/export/preview", json=options)
    assert r.status_code == 200, r.text
    out = r.json()
    check(out, EXPORT_PREVIEW)
    return out


def download(api, run_id: int, **options):
    return api.get(f"/api/runs/{run_id}/export", params=bools(options))


@pytest.fixture(scope="module")
def first_sample(api, world):
    """Detail of the main run's first sample (prompt 0, sample 0)."""
    s = list_samples(api, world.main["id"])[0]
    return api.get(f"/api/samples/{s['id']}").json()


# --------------------------------------------------------------------------- preview


def test_preview_defaults(api, world, first_sample):
    out = preview(api, world.main["id"])
    assert out["count"] == 60
    assert out["skipped"] == {}
    assert out["filename"] == "contract-main-run-messages.jsonl"
    prompt = world.prompts[0]["messages"]
    assert out["example"] == {
        "messages": [*prompt, {"role": "assistant", "content": think_wrap(first_sample["reasoning"], first_sample["response"])}]
    }
    assert out["example"]["messages"][0]["role"] == "system"
    assert out["example"]["messages"][-1]["content"].startswith("<think>\n")


@pytest.mark.parametrize("placement", PLACEMENTS)
def test_preview_reasoning_placement(api, world, first_sample, placement):
    out = preview(api, world.main["id"], reasoning=placement)
    assistant = out["example"]["messages"][-1]
    reasoning, response = first_sample["reasoning"], first_sample["response"]
    if placement == "think_tags":
        assert assistant == {"role": "assistant", "content": think_wrap(reasoning, response)}
    elif placement == "omit":
        assert assistant == {"role": "assistant", "content": response}
    else:
        assert assistant == {"role": "assistant", "content": response, placement: reasoning}
    assert out["count"] == 60


def test_preview_include_system_false(api, world, first_sample):
    out = preview(api, world.main["id"], include_system=False)
    roles = [m["role"] for m in out["example"]["messages"]]
    assert roles == ["user", "assistant"]


def test_preview_formats(api, world, first_sample):
    rid = world.main["id"]
    prompt = world.prompts[0]["messages"]
    system, user = prompt[0]["content"], prompt[1]["content"]
    text = think_wrap(first_sample["reasoning"], first_sample["response"])

    out = preview(api, rid, format="sharegpt")
    assert out["filename"] == "contract-main-run-sharegpt.jsonl"
    assert out["example"] == {
        "conversations": [
            {"from": "system", "value": system},
            {"from": "human", "value": user},
            {"from": "gpt", "value": text},
        ]
    }

    out = preview(api, rid, format="alpaca")
    assert out["filename"] == "contract-main-run-alpaca.jsonl"
    assert out["example"] == {"instruction": user, "input": "", "output": text, "system": system}

    out = preview(api, rid, format="alpaca", reasoning="reasoning_content", include_system=False)
    assert out["example"] == {
        "instruction": user, "input": "", "output": first_sample["response"], "reasoning": first_sample["reasoning"],
    }

    out = preview(api, rid, format="sharegpt", reasoning="thinking")
    assert out["example"]["reasoning"] == first_sample["reasoning"]
    assert out["example"]["conversations"][-1] == {"from": "gpt", "value": first_sample["response"]}

    out = preview(api, rid, format="prompt_completion")
    assert out["filename"] == "contract-main-run-prompt_completion.jsonl"
    assert out["example"] == {"prompt": prompt, "completion": [{"role": "assistant", "content": text}]}

    out = preview(api, rid, format="raw")
    assert out["filename"] == "contract-main-run-raw.jsonl"
    raw = out["example"]
    check(raw, RAW_RECORD)
    assert raw["id"] == first_sample["id"] and raw["run_id"] == rid
    assert raw["prompt_id"] == world.prompts[0]["id"] and raw["prompt_index"] == 0 and raw["sample_index"] == 0
    assert raw["model"] == "mock-reasoning-content" and raw["endpoint"] == "Contract mock"
    assert raw["messages"] == prompt  # the prompt only, without the answer
    assert raw["reasoning"] == first_sample["reasoning"] and raw["response"] == first_sample["response"]
    assert raw["finish_reason"] == "stop"
    assert raw["usage"] == {
        "prompt_tokens": first_sample["prompt_tokens"],
        "completion_tokens": first_sample["completion_tokens"],
        "reasoning_tokens": first_sample["reasoning_tokens"],
    }
    assert raw["latency_ms"] == first_sample["latency_ms"] and raw["ttft_ms"] == first_sample["ttft_ms"]
    assert raw["params"] == world.main["params"]
    assert raw["meta"] == {"category": "even"}


def test_preview_file_type_only_changes_the_filename(api, world):
    for ft in FILE_TYPES:
        out = preview(api, world.main["id"], file_type=ft)
        assert out["filename"] == f"contract-main-run-messages.{ft}"
        assert out["count"] == 60 and "messages" in out["example"]


def test_preview_filters(api, world):
    out = preview(api, world.main["id"], first_sample_only=True)
    assert out["count"] == 30 and out["skipped"] == {"extra_samples": 30}
    assert out["example"]["messages"][-1]["role"] == "assistant"

    out = preview(api, world.main["id"], require_reasoning=True)
    assert out["count"] == 60 and out["skipped"] == {}

    out = preview(api, world.plain["id"])
    assert out["count"] == 30 and out["skipped"] == {}
    assert "<think>" not in out["example"]["messages"][-1]["content"]  # nothing to wrap
    out = preview(api, world.plain["id"], require_reasoning=True)
    assert out["count"] == 0 and out["skipped"] == {"no_reasoning": 30} and out["example"] is None

    out = preview(api, world.think["id"], require_reasoning=True)
    assert out["count"] == 30


def test_preview_truncated_samples(api, world):
    rid = world.trunc["id"]
    out = preview(api, rid)
    assert out["count"] == 0 and out["skipped"] == {"truncated": 30} and out["example"] is None
    assert out["filename"] == "contract-truncated-run-messages.jsonl"
    out = preview(api, rid, exclude_truncated=False)
    assert out["count"] == 30 and out["skipped"] == {}
    assert out["example"]["messages"][-1]["role"] == "assistant"


def test_preview_validation(api, world):
    url = f"/api/runs/{world.main['id']}/export/preview"
    for field, value in (("format", "csv"), ("file_type", "xml"), ("reasoning", "hidden")):
        assert_error(api.post(url, json={field: value}), 422, field=field)
    assert_error(api.post(url, json={"include_system": "sure"}), 422, field="include_system")
    assert_error(api.post("/api/runs/999999/export/preview", json={}), 404, detail="Run not found")


# --------------------------------------------------------------------------- downloads


def test_download_jsonl_matches_the_preview(api, world):
    rid = world.main["id"]
    pv = preview(api, rid)
    r = download(api, rid)
    assert_download_headers(r, "jsonl", pv["filename"])
    records, _ = parse_download("jsonl", r.content)
    assert len(records) == 60
    assert records[0] == pv["example"]
    # Ordered by prompt, then sample.
    users = [rec["messages"][1]["content"] for rec in records]
    assert users == [f"Task tag-{i // 2:03d}: compute {3 + i // 2} plus {40 - i // 2}." for i in range(60)]
    # Text is written as UTF-8, not \u escapes (the mock's answers use curly quotes).
    assert "“".encode() in r.content and b"\\u201c" not in r.content


@pytest.mark.parametrize("fmt", SFT_FORMATS)
def test_every_file_type_carries_the_same_records(api, world, fmt):
    rid = world.main["id"]
    pv = preview(api, rid, format=fmt)
    base = download(api, rid, format=fmt, file_type="jsonl")
    assert_download_headers(base, "jsonl", pv["filename"])
    want, _ = parse_download("jsonl", base.content)
    assert len(want) == 60 and want[0] == pv["example"]
    for ft in ("json", "csv", "parquet"):
        r = download(api, rid, format=fmt, file_type=ft)
        assert_download_headers(r, ft, f"contract-main-run-{fmt}.{ft}")
        got, columns = parse_download(ft, r.content)
        assert_same_records(want, ft, got, columns)


@pytest.mark.parametrize("placement", ["reasoning_content", "omit"])
def test_file_types_with_outside_reasoning(api, world, placement):
    rid = world.main["id"]
    for fmt in ("messages", "alpaca"):
        want, _ = parse_download("jsonl", download(api, rid, format=fmt, reasoning=placement).content)
        for ft in ("json", "csv", "parquet"):
            got, cols = parse_download(ft, download(api, rid, format=fmt, reasoning=placement, file_type=ft).content)
            assert_same_records(want, ft, got, cols)


def test_json_download_is_one_array(api, world):
    r = download(api, world.main["id"], file_type="json", first_sample_only=True)
    text = r.content.decode()
    assert text.startswith("[") and text.rstrip().endswith("]")
    assert len(json.loads(text)) == 30


def test_csv_header_and_structured_cells(api, world):
    r = download(api, world.main["id"], file_type="csv", format="alpaca")
    first_line = r.content.decode().split("\n")[0]
    assert first_line == "instruction,input,output,system,history"
    r = download(api, world.main["id"], file_type="csv", format="messages")
    assert r.content.decode().split("\n")[0] == "messages"
    rows = parse_download("csv", r.content)[0]
    assert json.loads(rows[0]["messages"])[-1]["role"] == "assistant"
    r = download(api, world.main["id"], file_type="csv", format="raw")
    header = r.content.decode().split("\n")[0].split(",")
    assert header == [
        "id", "run_id", "prompt_id", "prompt_index", "sample_index", "model", "endpoint", "messages",
        "reasoning", "response", "finish_reason", "usage", "latency_ms", "ttft_ms", "params", "meta", "created_at",
    ]


def test_parquet_columns(api, world):
    import io

    import pyarrow.parquet as pq

    r = download(api, world.main["id"], file_type="parquet", format="messages", reasoning="reasoning_content")
    table = pq.read_table(io.BytesIO(r.content))
    assert table.num_rows == 60 and table.column_names == ["messages"]
    msg_type = table.schema.field("messages").type.value_type
    assert [msg_type.field(i).name for i in range(msg_type.num_fields)] == ["role", "content", "reasoning_content"]
    r = download(api, world.main["id"], file_type="parquet", format="alpaca")
    assert pq.read_table(io.BytesIO(r.content)).column_names == ["instruction", "input", "output", "system", "history"]
    r = download(api, world.main["id"], file_type="parquet", format="sharegpt")
    assert pq.read_table(io.BytesIO(r.content)).column_names == ["conversations"]
    r = download(api, world.main["id"], file_type="parquet", format="prompt_completion")
    assert pq.read_table(io.BytesIO(r.content)).column_names == ["prompt", "completion"]


def test_download_filters(api, world):
    rid = world.main["id"]
    assert len(parse_download("jsonl", download(api, rid, first_sample_only=True).content)[0]) == 30
    assert len(parse_download("jsonl", download(api, rid, include_system=False).content)[0][0]["messages"]) == 2
    # Booleans arrive as query strings in several spellings.
    for spelling in ("false", "0", "False"):
        r = api.get(f"/api/runs/{world.trunc['id']}/export", params={"exclude_truncated": spelling})
        assert len(parse_download("jsonl", r.content)[0]) == 30, spelling
    for spelling in ("true", "1"):
        r = api.get(f"/api/runs/{world.trunc['id']}/export", params={"exclude_truncated": spelling})
        assert r.content == b"", spelling


@pytest.mark.parametrize("ft", FILE_TYPES)
def test_empty_exports_are_valid_files(api, world, ft):
    r = download(api, world.trunc["id"], file_type=ft)  # every sample is truncated, so nothing is left
    assert_download_headers(r, ft, f"contract-truncated-run-messages.{ft}")
    records, columns = parse_download(ft, r.content)
    assert records == []
    if ft == "csv":
        assert columns == ["messages"]
    if ft == "parquet":
        assert columns == ["messages"]
    if ft == "jsonl":
        assert r.content == b""


def test_download_validation(api, world):
    rid = world.main["id"]
    for field, value in (("format", "csv"), ("file_type", "xml"), ("reasoning", "hidden"), ("include_system", "maybe")):
        assert_error(api.get(f"/api/runs/{rid}/export", params={field: value}), 422, field=field)
    assert_error(api.get("/api/runs/999999/export"), 404, detail="Run not found")


def test_filename_slug(api, world, make_endpoint, make_prompt_set, make_run):
    ep = make_endpoint(name="Slug endpoint")
    ps = make_prompt_set(1, name="Slug prompts")
    run = make_run(ep["id"], ps["id"], "mock-plain", name="Café Ünïcode: Run #1 / test!")
    out = preview(api, run["id"], format="sharegpt", file_type="csv")
    assert out["filename"] == f"{slugify('Café Ünïcode: Run #1 / test!')}-sharegpt.csv" == "cafe-unicode-run-1-test-sharegpt.csv"
    api.delete(f"/api/runs/{run['id']}")


# --------------------------------------------------------------------------- failed samples


@pytest.fixture(scope="module")
def failing_run(api, mock_url):
    ep = create_endpoint(api, mock_url, name="Export flaky endpoint")
    ps = import_prompts(api, source_paste("\n".join(f"Export flaky prompt {i}" for i in range(40))), "Export flaky prompts", {"split_mode": "line"})
    run = create_run(api, ep["id"], ps["id"], "mock-flaky", name="Export flaky", max_attempts=1, pause_after_failures=0)
    run = wait_run(api, run["id"], timeout=60)
    yield run
    api.delete(f"/api/runs/{run['id']}")
    api.delete(f"/api/prompt-sets/{ps['id']}")
    api.delete(f"/api/endpoints/{ep['id']}")


def test_failed_samples_are_skipped(api, failing_run):
    assert failing_run["failed"] > 0
    out = preview(api, failing_run["id"])
    assert out["count"] == failing_run["done"]
    assert out["skipped"] == {"failed": failing_run["failed"]}
    records, _ = parse_download("jsonl", download(api, failing_run["id"]).content)
    assert len(records) == failing_run["done"]


# --------------------------------------------------------------------------- hub


def test_hub_push_needs_a_token(api, world, hf_token_cleared):
    body = {"target": {"repo_id": "someone/dataset"}}
    r = api.post(f"/api/runs/{world.main['id']}/export/hub", json=body)
    assert_error(r, 400, detail="Add a Hugging Face token in Settings to push datasets.")


def test_hub_push_validation(api, world):
    url = f"/api/runs/{world.main['id']}/export/hub"
    assert_error(api.post(url, json={"target": {"repo_id": "ab"}}), 422, field="target.repo_id")
    assert_error(api.post(url, json={}), 422, field="target")
    assert_error(api.post(url, json={"target": {"repo_id": "a/b"}, "options": {"format": "nope"}}), 422)
    r = api.post("/api/runs/999999/export/hub", json={"target": {"repo_id": "someone/dataset"}})
    assert_error(r, 404, detail="Run not found")


def test_hub_push_refuses_before_any_upload(api, world, hf_token_cleared):
    api.put("/api/settings", json={"hf_token": "hf_not_a_real_token_0000"})
    url = f"/api/runs/{world.main['id']}/export/hub"
    r = api.post(url, json={"target": {"repo_id": "someone/dataset", "split": "bad split!"}})
    assert_error(r, 400, detail="The split name can only use letters, numbers, dots, dashes and underscores.")
    r = api.post(f"/api/runs/{world.trunc['id']}/export/hub", json={"target": {"repo_id": "someone/dataset"}})
    assert_error(r, 400, detail="There are no records to push with these options.")
