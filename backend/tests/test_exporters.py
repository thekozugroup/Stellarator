from __future__ import annotations

import csv
import io
import json
import os
import tempfile
from datetime import datetime, timezone

os.environ.setdefault("STELLARATOR_DATA_DIR", tempfile.mkdtemp(prefix="stellarator-exp-"))

import pyarrow.parquet as pq
import pytest

from stellarator.models import Run, Sample
from stellarator.schemas import ExportOptions, PrefExportOptions
from stellarator.services import exporters as ex

PROMPT = [
    {"role": "system", "content": "Be brief."},
    {"role": "user", "content": "What is 2+2? é"},
]


def make_run(**params) -> Run:
    return Run(
        id=7, name="Qwen3 32B on gsm8k", endpoint_name="spark", model="qwen3-32b", prompt_set_id=1, params=params
    )


def make_sample(reasoning="2 plus 2 is 4.", response="4", **kw) -> Sample:
    base = dict(
        id=11, run_id=7, prompt_id=3, prompt_idx=2, sample_idx=0, status="done", reasoning=reasoning,
        response=response, finish_reason="stop", prompt_tokens=10, completion_tokens=20, reasoning_tokens=5,
        latency_ms=100, ttft_ms=10, completed_at=datetime(2026, 1, 2, 3, 4, 5, tzinfo=timezone.utc),
    )
    base.update(kw)
    return Sample(**base)


def sft(fmt, placement, sample=None, run=None, **opts):
    o = ExportOptions(format=fmt, reasoning=placement, **opts)
    return ex.sft_record(o, run or make_run(), sample or make_sample(), PROMPT, {"answer": "4"})


THINK = "<think>\n2 plus 2 is 4.\n</think>\n\n4"


@pytest.mark.parametrize("placement", ["think_tags", "reasoning_content", "thinking", "omit"])
def test_messages_placements(placement):
    rec = sft("messages", placement)
    assert rec["messages"][:2] == PROMPT
    a = rec["messages"][-1]
    assert a["role"] == "assistant"
    if placement == "think_tags":
        assert a == {"role": "assistant", "content": THINK}
    elif placement == "omit":
        assert a == {"role": "assistant", "content": "4"}
    else:
        assert a == {"role": "assistant", "content": "4", placement: "2 plus 2 is 4."}
    # no reasoning
    a2 = sft("messages", placement, make_sample(reasoning=None))["messages"][-1]
    assert a2["content"] == "4"
    assert a2.get(placement) is None


@pytest.mark.parametrize("placement", ["think_tags", "reasoning_content", "thinking", "omit"])
def test_sharegpt_alpaca_prompt_completion(placement):
    outside = placement in ("reasoning_content", "thinking")
    text = THINK if placement == "think_tags" else "4"
    sg = sft("sharegpt", placement)
    assert sg["conversations"] == [
        {"from": "system", "value": "Be brief."},
        {"from": "human", "value": "What is 2+2? é"},
        {"from": "gpt", "value": text},
    ]
    assert sg.get("reasoning") == ("2 plus 2 is 4." if outside else None)
    assert ("reasoning" in sg) == outside
    al = sft("alpaca", placement)
    assert al["instruction"] == "What is 2+2? é" and al["input"] == "" and al["output"] == text
    assert al["system"] == "Be brief." and "history" not in al
    assert ("reasoning" in al) == outside
    assert sft("alpaca", placement, make_sample(reasoning=None))["output"] == "4"
    pc = sft("prompt_completion", placement)
    assert pc["prompt"] == PROMPT and pc["completion"][0]["role"] == "assistant"
    assert pc["completion"][0]["content"] == text


def test_alpaca_history_and_no_system():
    msgs = [
        {"role": "user", "content": "a"},
        {"role": "assistant", "content": "b"},
        {"role": "user", "content": "c"},
    ]
    o = ExportOptions(format="alpaca")
    rec = ex.sft_record(o, make_run(), make_sample(), msgs)
    assert rec["history"] == [["a", "b"]] and rec["instruction"] == "c" and "system" not in rec


def test_raw():
    rec = sft("raw", "think_tags", run=make_run(temperature=0.7))
    assert rec["id"] == 11 and rec["run_id"] == 7 and rec["prompt_id"] == 3
    assert rec["prompt_index"] == 2 and rec["sample_index"] == 0
    assert rec["model"] == "qwen3-32b" and rec["endpoint"] == "spark"
    assert rec["messages"] == PROMPT and rec["reasoning"] == "2 plus 2 is 4." and rec["response"] == "4"
    assert rec["usage"] == {"prompt_tokens": 10, "completion_tokens": 20, "reasoning_tokens": 5}
    assert rec["params"] == {"temperature": 0.7} and rec["meta"] == {"answer": "4"}
    assert rec["created_at"] == "2026-01-02T03:04:05+00:00"


def test_params_applied_and_include_system():
    run = make_run(system_prompt="Answer in JSON.", user_template="Solve:\n{prompt}\nThanks")
    rec = sft("messages", "omit", run=run)
    assert rec["messages"][0] == {"role": "system", "content": "Answer in JSON."}
    assert rec["messages"][1]["content"] == "Solve:\nWhat is 2+2? é\nThanks"
    no_sys = sft("messages", "omit", run=run, include_system=False)
    assert [m["role"] for m in no_sys["messages"]] == ["user", "assistant"]
    # system prompt added when prompt has none
    o = ExportOptions()
    rec = ex.sft_record(o, run, make_sample(), [PROMPT[1]])
    assert rec["messages"][0]["role"] == "system"
    assert "system" not in sft("alpaca", "omit", include_system=False)


def test_think_tags_empty_reasoning():
    assert sft("messages", "think_tags", make_sample(reasoning="  \n"))["messages"][-1]["content"] == "4"


def pair_samples():
    return make_sample(id=1), make_sample(id=2, reasoning=None, response="5")


@pytest.mark.parametrize("placement", ["think_tags", "reasoning_content", "thinking", "omit"])
def test_dpo_kto(placement):
    c, r = pair_samples()
    run = {"system_prompt": None}
    conv = ex.pref_records(PrefExportOptions(reasoning=placement), c, r, PROMPT, run)
    assert len(conv) == 1 and set(conv[0]) == {"prompt", "chosen", "rejected"}
    assert conv[0]["prompt"] == PROMPT
    assert conv[0]["rejected"][0]["content"] == "5"
    assert conv[0]["chosen"][0]["content"] == (THINK if placement == "think_tags" else "4")
    text = ex.pref_records(PrefExportOptions(reasoning=placement, conversational=False), c, r, PROMPT, run)[0]
    assert text["prompt"] == "Be brief.\n\nWhat is 2+2? é"
    assert text["chosen"] == (THINK if placement == "think_tags" else "4") and text["rejected"] == "5"
    assert ("chosen_reasoning" in text) == (placement in ("reasoning_content", "thinking"))
    kto = ex.pref_records(PrefExportOptions(format="kto", reasoning=placement), c, r, PROMPT, run)
    assert [k["label"] for k in kto] == [True, False] and set(kto[0]) == {"prompt", "completion", "label"}
    ktxt = ex.pref_records(PrefExportOptions(format="kto", reasoning=placement, conversational=False), c, r, PROMPT, run)
    assert isinstance(ktxt[0]["completion"], str) and ktxt[1]["label"] is False


def test_prompt_text_multiturn_and_no_system():
    msgs = [
        {"role": "user", "content": "a"},
        {"role": "assistant", "content": "b"},
        {"role": "user", "content": "c"},
    ]
    assert ex.render_prompt_text(msgs) == "User: a\n\nAssistant: b\n\nUser: c"
    assert ex.render_prompt_text(PROMPT[1:]) == "What is 2+2? é"


def test_slug():
    assert ex.filename_for("Qwen3-32B on GSM8K!", "messages", "jsonl") == "qwen3-32b-on-gsm8k-messages.jsonl"
    assert ex.slugify("日本語") == "export"


# ---------------------------------------------------------------- writers


async def agen(records):
    for r in records:
        yield r


async def collect(file_type, records, schema) -> bytes:
    return b"".join([c async for c in ex.stream_file(file_type, agen(records), schema)])


def many(options):
    recs = []
    for i, (reasoning, resp) in enumerate([("why\nbecause \"quotes\", commas", "line1\nline2 é"), (None, "x")]):
        recs.append(ex.sft_record(options, make_run(), make_sample(id=i, reasoning=reasoning, response=resp), PROMPT, {"k": 1}))
    return recs


async def test_jsonl_valid_non_ascii():
    o = ExportOptions()
    data = await collect("jsonl", many(o), ex.sft_schema(o))
    lines = data.decode().splitlines()
    assert len(lines) == 2
    for line in lines:
        rec = json.loads(line)
        assert rec["messages"][-1]["role"] == "assistant"
    assert "é" in data.decode() and "\\u" not in data.decode()


async def test_json_array():
    o = ExportOptions(file_type="json")
    recs = many(o)
    assert json.loads(await collect("json", recs, ex.sft_schema(o))) == recs
    assert json.loads(await collect("json", [], ex.sft_schema(o))) == []


async def test_csv_quoting_and_nested():
    o = ExportOptions(format="alpaca", reasoning="reasoning_content")
    recs = many(o)
    data = (await collect("csv", recs, ex.sft_schema(o))).decode()
    rows = list(csv.DictReader(io.StringIO(data, newline="")))
    assert len(rows) == 2
    assert rows[0]["output"] == "line1\nline2 é" and rows[0]["reasoning"].startswith("why\nbecause \"quotes\"")
    assert rows[1]["reasoning"] == "" and rows[0]["system"] == "Be brief."
    o2 = ExportOptions()
    rows = list(csv.DictReader(io.StringIO((await collect("csv", many(o2), ex.sft_schema(o2))).decode(), newline="")))
    msgs = json.loads(rows[0]["messages"])
    assert msgs[-1]["role"] == "assistant" and msgs[0]["content"] == "Be brief."


@pytest.mark.parametrize("fmt", ["messages", "sharegpt", "alpaca", "prompt_completion", "raw"])
@pytest.mark.parametrize("placement", ["think_tags", "reasoning_content", "thinking"])
async def test_parquet_round_trip(fmt, placement, tmp_path):
    o = ExportOptions(format=fmt, reasoning=placement)
    recs = many(o)
    data = await collect("parquet", recs, ex.sft_schema(o))
    p = tmp_path / "x.parquet"
    p.write_bytes(data)
    back = pq.read_table(p).to_pylist()
    assert len(back) == 2
    want = [ex.strip_none(r) for r in recs]
    got = [ex.strip_none(r) for r in back]
    if fmt == "raw":
        for g in got:
            g["params"] = json.loads(g["params"])
            g["meta"] = json.loads(g["meta"])
    assert got == want


@pytest.mark.parametrize("fmt", ["dpo", "kto"])
@pytest.mark.parametrize("conversational", [True, False])
async def test_pref_parquet_round_trip(fmt, conversational, tmp_path):
    o = PrefExportOptions(format=fmt, conversational=conversational, reasoning="thinking")
    c, r = pair_samples()
    recs = ex.pref_records(o, c, r, PROMPT, {})
    p = tmp_path / "p.parquet"
    p.write_bytes(await collect("parquet", recs, ex.pref_schema(o)))
    assert [ex.strip_none(x) for x in pq.read_table(p).to_pylist()] == [ex.strip_none(x) for x in recs]


async def test_parquet_empty_and_tempfile_cleanup():
    o = ExportOptions()
    data = await collect("parquet", [], ex.sft_schema(o))
    assert data[:4] == b"PAR1"
    assert list(ex.export_dir().glob("*.parquet")) == []
