"""/api/imports: upload, preview and commit for pasted text and uploaded files."""

from __future__ import annotations

import json
import re
import time

import pytest

from shapes import IMPORT_PREVIEW, PROMPT_SET, check
from support import (
    all_prompts,
    assert_error,
    import_outcome,
    import_prompts,
    jsonl,
    source_file,
    source_paste,
    upload,
    wait_prompt_set_ready,
)

NO_PROMPTS = "No prompts were found with these settings. Check the column or split mode and try again."


def preview(api, source, mapping=None):
    body = {"source": source}
    if mapping is not None:
        body["mapping"] = mapping
    r = api.post("/api/imports/preview", json=body)
    assert r.status_code == 200, r.text
    out = r.json()
    check(out, IMPORT_PREVIEW)
    return out


def user(text):
    return [{"role": "user", "content": text}]


# --------------------------------------------------------------------------- upload


@pytest.mark.parametrize(
    "filename, content, fmt",
    [
        ("notes.md", "# One\nbody\n# Two\nbody\n", "markdown"),
        ("notes.markdown", "text", "markdown"),
        ("notes.txt", "a\nb\nc\n", "text"),
        ("prompts.csv", "prompt,topic\nhello,a\nworld,b\n", "csv"),
        ("prompts.tsv", "prompt\ttopic\nhello\ta\n", "tsv"),
        ("prompts.json", '["a", "b"]', "json"),
        ("prompts.jsonl", '{"prompt": "a"}\n{"prompt": "b"}\n', "jsonl"),
        ("prompts.ndjson", '{"prompt": "a"}\n', "jsonl"),
        ("PROMPTS.CSV", "prompt\nx\n", "csv"),
        ("no-extension", '[{"prompt": "a"}]', "json"),
        ("sniffed", '{"prompt": "a"}\n{"prompt": "b"}\n', "jsonl"),
        ("sniffed-csv", "a,b\n1,2\n3,4\n", "csv"),
        ("sniffed-tsv", "a\tb\n1\t2\n3\t4\n", "tsv"),
        ("sniffed-text", "just some words\nand more\n", "text"),
    ],
)
def test_upload_detects_the_format(api, filename, content, fmt):
    out = upload(api, filename, content)
    assert out["detected_format"] == fmt
    assert out["filename"] == filename
    assert out["size"] == len(content.encode())
    assert re.fullmatch(r"[0-9a-f]{32}", out["upload_id"])


def test_upload_ids_are_unique(api):
    a, b = upload(api, "a.txt", "x"), upload(api, "a.txt", "x")
    assert a["upload_id"] != b["upload_id"]


def test_upload_sanitizes_the_filename(api):
    assert upload(api, "../../etc/pass wd.csv", "prompt\nx\n")["filename"] == "pass wd.csv"
    assert upload(api, "my prompts (v2).md", "x")["filename"] == "my prompts _v2_.md"


def test_upload_of_an_empty_file_is_refused(api):
    r = api.post("/api/imports/upload", files={"file": ("empty.txt", b"", "text/plain")})
    assert_error(r, 422, detail="That file is empty.")


def test_upload_without_a_file_field(api):
    r = api.post("/api/imports/upload", data={"nope": "1"})
    assert_error(r, 422)


# --------------------------------------------------------------------------- preview: pasted text


@pytest.mark.parametrize(
    "text, mode, pieces",
    [
        ("Q one\n---\nQ two\n---\nQ three", "separator", ["Q one", "Q two", "Q three"]),
        ("# A\nbody a\n# B\nbody b", "heading", ["A\n\nbody a", "B\n\nbody b"]),
        ("- one\n- two\n- three", "list_item", ["one", "two", "three"]),
        ("1. one\n2. two", "list_item", ["one", "two"]),
        ("para one\nstill one\n\npara two\n\npara three", "blank_line", ["para one\nstill one", "para two", "para three"]),
        ("line one\nline two\nline three", "line", ["line one", "line two", "line three"]),
        ("a single thought", "whole", ["a single thought"]),
    ],
)
def test_paste_preview_picks_a_split_mode(api, text, mode, pieces):
    out = preview(api, source_paste(text))
    assert out["detected_format"] == "paste"
    assert out["split_mode"] == mode
    assert out["columns"] == [] and out["prompt_field"] is None and out["system_field"] is None
    assert out["prompts"] == [user(p) for p in pieces]
    assert out["total_rows"] == len(pieces)
    assert out["estimated_prompts"] == len(pieces)
    assert out["warnings"] == []
    assert out["hf_splits"] == [] and out["hf_config"] is None and out["hf_split"] is None


def test_paste_preview_explicit_split_mode(api):
    text = "one\ntwo\n\nthree"
    assert preview(api, source_paste(text), {"split_mode": "whole"})["prompts"] == [user(text)]
    assert preview(api, source_paste(text), {"split_mode": "line"})["prompts"] == [user(p) for p in ("one", "two", "three")]
    out = preview(api, source_paste(text), {"split_mode": "blank_line"})
    assert out["prompts"] == [user("one\ntwo"), user("three")]
    assert out["split_mode"] == "blank_line"


def test_paste_preview_dedupe_and_warning(api):
    text = "same\nsame\nother\nsame"
    out = preview(api, source_paste(text), {"split_mode": "line"})
    assert out["prompts"] == [user("same"), user("other")]
    assert out["total_rows"] == 4 and out["estimated_prompts"] == 2
    assert out["warnings"] == ["2 duplicate prompts were dropped."]
    out = preview(api, source_paste(text), {"split_mode": "line", "dedupe": False})
    assert len(out["prompts"]) == 4 and out["estimated_prompts"] == 4 and out["warnings"] == []


def test_paste_preview_row_window(api):
    text = "\n".join(f"item {i}" for i in range(10))
    out = preview(api, source_paste(text), {"split_mode": "line", "offset": 2, "limit": 3})
    assert out["prompts"] == [user(f"item {i}") for i in (2, 3, 4)]
    assert out["estimated_prompts"] == 3
    assert out["total_rows"] == 10


def test_paste_preview_shows_at_most_20_prompts(api):
    text = "\n".join(f"item {i}" for i in range(45))
    out = preview(api, source_paste(text), {"split_mode": "line"})
    assert len(out["prompts"]) == 20
    assert out["total_rows"] == 45 and out["estimated_prompts"] == 45


def test_paste_preview_empty_text(api):
    r = api.post("/api/imports/preview", json={"source": source_paste("  \n ")})
    assert_error(r, 422, detail="Paste some text to import.")


def test_preview_validation(api):
    r = api.post("/api/imports/preview", json={"source": source_paste("x"), "mapping": {"offset": -1}})
    assert_error(r, 422, field="mapping.offset")
    r = api.post("/api/imports/preview", json={"source": source_paste("x"), "mapping": {"limit": 0}})
    assert_error(r, 422, field="mapping.limit")
    r = api.post("/api/imports/preview", json={"source": source_paste("x"), "mapping": {"split_mode": "words"}})
    assert_error(r, 422)
    assert_error(api.post("/api/imports/preview", json={"source": {"kind": "ftp"}}), 422)
    assert_error(api.post("/api/imports/preview", json={}), 422)
    r = api.post("/api/imports/preview", json={"source": {"kind": "huggingface", "dataset": ""}})
    body = assert_error(r, 422)
    assert any(k.endswith("dataset") for k in body["fields"]), body


# --------------------------------------------------------------------------- preview: files


CSV = (
    "id,system,question,answer,score\n"
    + "\n".join(f'{i},Be brief.,"What is {i} + {i}?",{i * 2},{i / 2}' for i in range(30))
    + "\n"
)


def test_csv_preview(api):
    up = upload(api, "q.csv", CSV, "text/csv")
    out = preview(api, source_file(up["upload_id"]))
    assert out["detected_format"] == "csv"
    cols = {c["name"]: c for c in out["columns"]}
    assert list(cols) == ["id", "system", "question", "answer", "score"]
    assert cols["id"]["kind"] == "number" and cols["id"]["example"] == "0"
    assert cols["question"]["kind"] == "text" and cols["question"]["example"] == "What is 0 + 0?"
    assert cols["score"]["kind"] == "number"
    assert out["prompt_field"] == "question" and out["system_field"] == "system"
    assert out["split_mode"] is None
    assert out["total_rows"] == 30 and out["estimated_prompts"] == 30
    assert len(out["prompts"]) == 20
    assert out["prompts"][0] == [
        {"role": "system", "content": "Be brief."},
        {"role": "user", "content": "What is 0 + 0?"},
    ]
    assert out["warnings"] == []


def test_csv_preview_explicit_columns_and_window(api):
    up = upload(api, "q.csv", CSV, "text/csv")
    out = preview(
        api,
        source_file(up["upload_id"], "csv"),
        {"prompt_field": "answer", "system_field": "", "offset": 5, "limit": 4},
    )
    assert out["prompt_field"] == "answer" and out["system_field"] is None
    assert out["prompts"] == [user(str(i * 2)) for i in range(5, 9)]
    assert out["estimated_prompts"] == 4
    # The Python backend only reports the source's row count when no window is set.
    assert out["total_rows"] in (None, 30)


def test_csv_preview_unknown_column(api):
    up = upload(api, "q.csv", CSV, "text/csv")
    r = api.post(
        "/api/imports/preview",
        json={"source": source_file(up["upload_id"]), "mapping": {"prompt_field": "nope"}},
    )
    assert_error(
        r, 422,
        detail="There is no column called 'nope' to use as the prompt. Columns are: id, system, question, answer, score.",
    )


def test_csv_semicolon_delimiter_is_sniffed(api):
    up = upload(api, "semi.csv", "prompt;topic\nhello there;a\nsecond one;b\n")
    out = preview(api, source_file(up["upload_id"]))
    assert [c["name"] for c in out["columns"]] == ["prompt", "topic"]
    assert out["prompts"] == [user("hello there"), user("second one")]


def test_csv_alpaca_instruction_and_input_are_joined(api):
    up = upload(api, "alpaca.csv", 'instruction,input,output\n"Translate","Good morning","Bonjour"\n"Say hi","",Hi\n')
    out = preview(api, source_file(up["upload_id"]))
    assert out["prompt_field"] == "instruction"
    assert out["prompts"] == [user("Translate\n\nGood morning"), user("Say hi")]


def test_csv_empty_rows_and_duplicates_are_reported(api):
    up = upload(api, "dup.csv", "prompt,n\nhello,1\nhello,2\n,3\nworld,4\n")
    out = preview(api, source_file(up["upload_id"]))
    assert out["prompts"] == [user("hello"), user("world")]
    assert out["estimated_prompts"] == 2
    assert out["warnings"] == ["1 row had an empty prompt and was skipped.", "1 duplicate prompt was dropped."]


def test_jsonl_messages_end_at_the_last_user_turn(api):
    rows = [
        {"messages": [
            {"role": "system", "content": "S"},
            {"role": "user", "content": "first"},
            {"role": "assistant", "content": "answer"},
            {"role": "user", "content": "second"},
            {"role": "assistant", "content": "answer 2"},
        ]},
        {"conversations": [{"from": "human", "value": "sharegpt q"}, {"from": "gpt", "value": "a"}]},
    ]
    up = upload(api, "chat.jsonl", jsonl([rows[0]]))
    out = preview(api, source_file(up["upload_id"]))
    assert out["detected_format"] == "jsonl"
    assert out["prompt_field"] == "messages"
    assert [c["kind"] for c in out["columns"]] == ["messages"]
    assert out["prompts"] == [[
        {"role": "system", "content": "S"},
        {"role": "user", "content": "first"},
        {"role": "assistant", "content": "answer"},
        {"role": "user", "content": "second"},
    ]]
    up = upload(api, "sharegpt.jsonl", jsonl([rows[1]]))
    out = preview(api, source_file(up["upload_id"]))
    assert out["prompt_field"] == "conversations"
    assert out["prompts"] == [user("sharegpt q")]


def test_jsonl_skips_bad_lines_with_a_warning(api):
    up = upload(api, "bad.jsonl", '{"prompt": "ok"}\nnot json\n{"prompt": "fine"}\n')
    out = preview(api, source_file(up["upload_id"]))
    assert out["prompts"] == [user("ok"), user("fine")]
    assert out["warnings"] == ["1 line was not valid JSON objects or strings and was skipped (first: line 2)."]


def test_json_shapes(api):
    up = upload(api, "list.json", json.dumps(["alpha", "beta", {"prompt": "gamma"}]))
    assert preview(api, source_file(up["upload_id"]))["prompts"] == [user("alpha"), user("beta"), user("gamma")]
    up = upload(api, "obj.json", json.dumps({"meta": 1, "data": [{"question": "q1"}, {"question": "q2"}]}))
    out = preview(api, source_file(up["upload_id"]))
    assert out["prompt_field"] == "question" and out["prompts"] == [user("q1"), user("q2")]
    up = upload(api, "none.json", json.dumps({"a": 1}))
    r = api.post("/api/imports/preview", json={"source": source_file(up["upload_id"])})
    assert_error(
        r, 422,
        detail='Couldn\'t find a list of prompts in this JSON. Use an array, or an object like {"data": [...]}.',
    )


def test_markdown_file_preview_strips_front_matter_and_splits_by_heading(api):
    md = "---\ntitle: Notes\n---\n# First\nbody one\n\n## Second\nbody two\n"
    up = upload(api, "notes.md", md)
    out = preview(api, source_file(up["upload_id"]))
    assert out["detected_format"] == "markdown" and out["split_mode"] == "heading"
    assert out["prompts"] == [user("First\n\nbody one"), user("Second\n\nbody two")]


def test_plain_text_file_ignores_headings_unless_asked(api):
    txt = "# not a heading\nsecond line\n# another\n"
    up = upload(api, "plain.txt", txt)
    assert preview(api, source_file(up["upload_id"]))["split_mode"] == "line"
    assert preview(api, source_file(up["upload_id"]), {"split_mode": "heading"})["split_mode"] == "heading"


def test_unknown_upload_ids(api):
    r = api.post("/api/imports/preview", json={"source": source_file("0" * 32)})
    assert_error(r, 404, detail="That upload was not found. It may have expired; upload the file again.")
    r = api.post("/api/imports/preview", json={"source": source_file("../etc")})
    assert_error(r, 404, detail="That upload was not found. Upload the file again.")


# --------------------------------------------------------------------------- commit


def test_commit_paste(api, trash):
    ps = import_prompts(api, source_paste("one\ntwo\nthree"), "  Pasted set  ", {"split_mode": "line"})
    trash.prompt_sets.append(ps["id"])
    check(ps, PROMPT_SET)
    assert ps["name"] == "Pasted set"
    assert ps["source_kind"] == "paste"
    assert ps["prompt_count"] == 3
    assert ps["run_count"] == 0
    assert ps["source"]["format"] == "paste" and ps["source"]["split_mode"] == "line"
    prompts = all_prompts(api, ps["id"])
    assert [p["idx"] for p in prompts] == [0, 1, 2]
    assert [p["messages"] for p in prompts] == [user("one"), user("two"), user("three")]
    assert all(p["meta"] == {} for p in prompts)


def test_commit_file_keeps_extra_columns_as_meta_and_consumes_the_upload(api, trash):
    up = upload(api, "q.csv", CSV, "text/csv")
    ps = import_prompts(api, source_file(up["upload_id"]), "From csv", {"offset": 2, "limit": 5})
    trash.prompt_sets.append(ps["id"])
    assert ps["source_kind"] == "file" and ps["prompt_count"] == 5
    assert ps["source"]["filename"] == "q.csv" and ps["source"]["format"] == "csv"
    assert ps["source"]["prompt_field"] == "question" and ps["source"]["system_field"] == "system"
    prompts = all_prompts(api, ps["id"])
    assert [p["idx"] for p in prompts] == [0, 1, 2, 3, 4]
    assert prompts[0]["messages"] == [
        {"role": "system", "content": "Be brief."},
        {"role": "user", "content": "What is 2 + 2?"},
    ]
    # CSV cells stay strings in meta.
    assert prompts[0]["meta"] == {"id": "2", "answer": "4", "score": "1.0"}
    # The upload is gone once the import has read it.
    r = api.post("/api/imports/preview", json={"source": source_file(up["upload_id"])})
    assert_error(r, 404)


def test_commit_jsonl_prompt_with_chat_messages(api, trash):
    up = upload(api, "chat.jsonl", jsonl([
        {"messages": [{"role": "user", "content": "hello"}, {"role": "assistant", "content": "hi"}], "tag": "x"},
        {"messages": [{"role": "user", "content": "bye"}], "tag": "y"},
    ]))
    ps = import_prompts(api, source_file(up["upload_id"]), "Chat set")
    trash.prompt_sets.append(ps["id"])
    prompts = all_prompts(api, ps["id"])
    assert [p["messages"] for p in prompts] == [user("hello"), user("bye")]
    assert [p["meta"] for p in prompts] == [{"tag": "x"}, {"tag": "y"}]


def test_commit_validation(api):
    r = api.post("/api/imports", json={"source": source_paste("x")})
    assert_error(r, 422, field="name")
    r = api.post("/api/imports", json={"source": source_paste("x"), "name": ""})
    assert_error(r, 422, field="name")
    r = api.post("/api/imports", json={"source": source_paste("x"), "name": "n" * 301})
    assert_error(r, 422, field="name")
    r = api.post("/api/imports", json={"name": "x"})
    assert_error(r, 422)
    r = api.post("/api/imports", json={"source": source_paste("x"), "name": "x", "mapping": {"offset": -3}})
    assert_error(r, 422, field="mapping.offset")


def test_commit_of_blank_paste_is_refused(api):
    status, out = import_outcome(api, {"source": source_paste("   "), "name": "Blank"})
    if status == 422:
        assert_error_body(out, "Paste some text to import.")
    else:
        assert out["status"] == "failed" and "Paste some text" in (out["status_detail"] or "")


def test_commit_with_nothing_in_the_window_is_refused(api):
    body = {"source": source_paste("a\nb\nc"), "name": "Empty window", "mapping": {"split_mode": "line", "offset": 10}}
    status, out = import_outcome(api, body)
    if status == 422:
        assert_error_body(out, NO_PROMPTS)
    else:
        assert out["status"] == "failed"
        assert NO_PROMPTS in (out["status_detail"] or "")


def test_commit_without_a_prompt_column_is_refused(api):
    up = upload(api, "numbers.csv", "a,b\n1,2\n3,4\n")
    status, out = import_outcome(api, {"source": source_file(up["upload_id"]), "name": "No text"})
    expected = "Couldn't find a text column to use as the prompt. Choose the column that holds the prompts."
    if status == 422:
        assert_error_body(out, expected)
    else:
        assert out["status"] == "failed" and expected in (out["status_detail"] or "")


def test_commit_with_unknown_upload(api):
    r = api.post("/api/imports", json={"source": source_file("f" * 32), "name": "Lost"})
    assert_error(r, 404, detail="That upload was not found. It may have expired; upload the file again.")


def assert_error_body(body: dict, detail: str) -> None:
    assert body["detail"] == detail, body


@pytest.mark.v1
def test_commit_answers_201_with_the_finished_set(api, trash):
    r = api.post(
        "/api/imports",
        json={"source": source_paste("one\ntwo"), "name": "Sync", "mapping": {"split_mode": "line"}},
    )
    assert r.status_code == 201, r.text
    ps = r.json()
    trash.prompt_sets.append(ps["id"])
    check(ps, PROMPT_SET)
    assert ps["prompt_count"] == 2
    assert "status" not in ps  # the Python backend has no import status


# --------------------------------------------------------------------------- v2


def big_csv(n: int) -> str:
    return "prompt,n\n" + "\n".join(f"Explain item number {i} in one sentence,{i}" for i in range(n)) + "\n"


@pytest.mark.v2
def test_commit_runs_in_the_background(api, trash):
    n = 20000
    up = upload(api, "big.csv", big_csv(n), "text/csv")
    t0 = time.monotonic()
    r = api.post("/api/imports", json={"source": source_file(up["upload_id"]), "name": "Background"})
    assert r.status_code == 202, r.text
    ps = r.json()
    trash.prompt_sets.append(ps["id"])
    check(ps, PROMPT_SET.require())
    assert ps["status"] == "importing"
    assert ps["source_kind"] == "file"
    assert ps["status_detail"] is None or isinstance(ps["status_detail"], str)

    seen = [ps["prompt_count"]]
    deadline = time.monotonic() + 90
    final = None
    while time.monotonic() < deadline:
        cur = api.get(f"/api/prompt-sets/{ps['id']}").json()
        check(cur, PROMPT_SET.require())
        seen.append(cur["prompt_count"])
        assert cur["rows_read"] >= 0
        if cur["status"] != "importing":
            final = cur
            break
        time.sleep(0.05)
    assert final is not None, "import never finished"
    assert final["status"] == "ready", final
    assert final["prompt_count"] == n
    assert final["rows_read"] == n
    assert final["rows_total"] in (None, n)
    assert final["status_detail"] is None
    assert seen == sorted(seen), "prompt_count only grows while importing"
    assert time.monotonic() - t0 < 90


@pytest.mark.v2
def test_listed_prompt_sets_carry_the_status_fields(api, make_prompt_set):
    ps = make_prompt_set(3)
    check(ps, PROMPT_SET.require())
    assert ps["status"] == "ready" and ps["rows_read"] >= 3
    listed = next(p for p in api.get("/api/prompt-sets").json() if p["id"] == ps["id"])
    check(listed, PROMPT_SET.require())
    assert listed["status"] == "ready"


@pytest.mark.v2
def test_turns_first_user_keeps_system_and_first_user_only(api, trash):
    conv = {"messages": [
        {"role": "system", "content": "S"},
        {"role": "user", "content": "first question"},
        {"role": "assistant", "content": "first answer"},
        {"role": "user", "content": "second question"},
        {"role": "assistant", "content": "second answer"},
    ]}
    up = upload(api, "conv.jsonl", jsonl([conv]))
    src = source_file(up["upload_id"])

    default = preview(api, src)
    check(default, IMPORT_PREVIEW.require())
    assert default["has_conversations"] is True
    assert default["prompts"][0][-1] == {"role": "user", "content": "second question"}
    assert any("assistant" in d for d in default["dropped"]), default["dropped"]

    first = preview(api, src, {"turns": "first_user"})
    assert first["prompts"] == [[
        {"role": "system", "content": "S"},
        {"role": "user", "content": "first question"},
    ]]

    ps = import_prompts(api, src, "First user only", {"turns": "first_user"})
    trash.prompt_sets.append(ps["id"])
    assert all_prompts(api, ps["id"])[0]["messages"] == first["prompts"][0]


@pytest.mark.v2
def test_turns_must_be_a_known_value(api):
    r = api.post(
        "/api/imports/preview",
        json={"source": source_paste("x"), "mapping": {"turns": "all"}},
    )
    assert_error(r, 422)


@pytest.mark.v2
def test_preview_flags_conversations_and_dropped_outputs(api):
    out = preview(api, source_paste("one\ntwo"), {"split_mode": "line"})
    check(out, IMPORT_PREVIEW.require())
    assert out["has_conversations"] is False and out["dropped"] == []

    up = upload(api, "outputs.csv", "topic,response,reasoning\nShort topic,A very long model answer that goes on and on,thinking text\nOther,Another long answer here,more thinking\n")
    out = preview(api, source_file(up["upload_id"]))
    check(out, IMPORT_PREVIEW.require())
    cols = {c["name"]: c for c in out["columns"]}
    assert cols["response"]["looks_like_output"] is True
    assert cols["reasoning"]["looks_like_output"] is True
    assert cols["topic"]["looks_like_output"] is False
    assert out["prompt_field"] == "topic", "an output column is never picked as the prompt"
    assert "response" in out["dropped"] and "reasoning" in out["dropped"]
    assert out["has_conversations"] is False
