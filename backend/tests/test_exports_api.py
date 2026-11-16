from __future__ import annotations

import csv
import io
import json
import os
import tempfile
from datetime import datetime, timezone

os.environ.setdefault("STELLARATOR_DATA_DIR", tempfile.mkdtemp(prefix="stellarator-exp-"))

import httpx
import pyarrow.parquet as pq
import pytest
from fastapi import FastAPI

from stellarator import config
from stellarator.api import exports
from stellarator.db import SessionLocal, engine
from stellarator.models import Base, PreferencePair, PreferenceSet, Prompt, PromptSet, Run, Sample, Setting
from stellarator.services import exporters

app = FastAPI()
app.include_router(exports.router)


@pytest.fixture
async def client(monkeypatch):
    if "stellarator-exp-" not in str(config.DATA_DIR) and "pytest" not in str(config.DATA_DIR):
        pytest.skip("refusing to reset a non-temporary data dir")
    monkeypatch.setattr("stellarator.services.settings.ENV_HF_TOKEN", None)
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.drop_all)
        await conn.run_sync(Base.metadata.create_all)
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://t") as c:
        yield c


def _sample(run_id, prompt, sidx, **kw):
    base = dict(
        run_id=run_id, prompt_id=prompt.id, prompt_idx=prompt.idx, sample_idx=sidx, status="done",
        reasoning=f"think {prompt.idx}-{sidx}", response=f"answer {prompt.idx}-{sidx} é", finish_reason="stop",
        prompt_tokens=5, completion_tokens=9, reasoning_tokens=3, latency_ms=50, ttft_ms=5,
        completed_at=datetime(2026, 1, 1, tzinfo=timezone.utc),
    )
    base.update(kw)
    return Sample(**base)


@pytest.fixture
async def seeded(client):
    async with SessionLocal() as s:
        ps = PromptSet(name="GSM8K sample", source_kind="paste", source={}, prompt_count=4)
        s.add(ps)
        await s.flush()
        prompts = [
            Prompt(
                prompt_set_id=ps.id, idx=i,
                messages=[{"role": "system", "content": "sys"}, {"role": "user", "content": f"q{i}"}],
                meta={"ref": i},
            )
            for i in range(4)
        ]
        s.add_all(prompts)
        run = Run(
            name="Qwen3 32B on GSM8K", endpoint_name="spark", model="qwen3-32b", prompt_set_id=ps.id,
            params={"temperature": 0.7, "user_template": "Solve: {prompt}"},
        )
        s.add(run)
        await s.flush()
        samples = [
            _sample(run.id, prompts[0], 0),
            _sample(run.id, prompts[0], 1),
            _sample(run.id, prompts[1], 0, finish_reason="length"),
            _sample(run.id, prompts[2], 0, reasoning=None),
            _sample(run.id, prompts[3], 0, status="failed", reasoning=None, response=None, error="x"),
            _sample(run.id, prompts[3], 1, status="pending", reasoning=None, response=None),
            _sample(run.id, prompts[3], 2, reasoning="", response=""),
        ]
        s.add_all(samples)
        await s.commit()
        return {"run": run.id, "ps": ps.id, "prompts": [p.id for p in prompts], "samples": [x.id for x in samples]}


async def test_preview_counts(client, seeded):
    r = await client.post(f"/api/runs/{seeded['run']}/export/preview", json={})
    assert r.status_code == 200
    body = r.json()
    assert body["count"] == 3  # q0 s0, q0 s1, q2 (truncated excluded)
    assert body["skipped"] == {"failed": 1, "pending": 1, "truncated": 1, "empty_response": 1}
    assert body["filename"] == "qwen3-32b-on-gsm8k-messages.jsonl"
    msgs = body["example"]["messages"]
    assert msgs[1]["content"] == "Solve: q0"
    assert msgs[-1] == {"role": "assistant", "content": "<think>\nthink 0-0\n</think>\n\nanswer 0-0 é"}

    r = await client.post(
        f"/api/runs/{seeded['run']}/export/preview",
        json={"exclude_truncated": False, "require_reasoning": True, "first_sample_only": True, "format": "alpaca"},
    )
    body = r.json()
    assert body["count"] == 2  # q0 s0, q1 (length kept)
    assert body["skipped"] == {"failed": 1, "pending": 1, "no_reasoning": 1, "extra_samples": 1, "empty_response": 1}
    assert body["example"]["instruction"] == "Solve: q0"


async def test_download_jsonl(client, seeded):
    r = await client.get(f"/api/runs/{seeded['run']}/export", params={"format": "messages", "reasoning": "reasoning_content"})
    assert r.status_code == 200
    assert r.headers["content-type"].startswith("application/x-ndjson")
    assert r.headers["content-disposition"] == 'attachment; filename="qwen3-32b-on-gsm8k-messages.jsonl"'
    recs = [json.loads(line) for line in r.text.splitlines()]
    assert len(recs) == 3
    for rec in recs:
        assert rec["messages"][-1]["role"] == "assistant"
        assert "reasoning_content" in rec["messages"][-1]
    assert [x["messages"][1]["content"] for x in recs] == ["Solve: q0", "Solve: q0", "Solve: q2"]
    assert "é" in r.text
    assert recs[2]["messages"][-1]["reasoning_content"] is None


async def test_download_types(client, seeded):
    base = f"/api/runs/{seeded['run']}/export"
    r = await client.get(base, params={"file_type": "json", "format": "sharegpt"})
    assert r.headers["content-type"].startswith("application/json")
    assert r.headers["content-disposition"].endswith('sharegpt.json"')
    assert len(r.json()) == 3
    r = await client.get(base, params={"file_type": "csv", "format": "raw", "exclude_truncated": "false"})
    assert r.headers["content-type"].startswith("text/csv")
    rows = list(csv.DictReader(io.StringIO(r.text, newline="")))
    assert len(rows) == 4 and json.loads(rows[0]["params"])["temperature"] == 0.7
    r = await client.get(base, params={"file_type": "parquet", "format": "prompt_completion"})
    assert r.headers["content-type"] == "application/vnd.apache.parquet"
    assert r.headers["content-disposition"].endswith('prompt_completion.parquet"')
    path = config.DATA_DIR / "t.parquet"
    path.write_bytes(r.content)
    rows = pq.read_table(path).to_pylist()
    assert len(rows) == 3 and rows[0]["completion"][0]["role"] == "assistant"
    assert list(exporters.export_dir().glob("*.parquet")) == []
    assert (await client.get(base, params={"format": "nope"})).status_code == 422


async def test_batching_keyset(client, seeded, monkeypatch):
    monkeypatch.setattr(exporters, "BATCH", 1)
    r = await client.get(f"/api/runs/{seeded['run']}/export")
    assert len(r.text.splitlines()) == 3
    from stellarator.schemas import ExportOptions

    async with SessionLocal() as s:
        run = await s.get(Run, seeded["run"])
    got = [x async for x in exporters.iter_sft_records(run, ExportOptions(), batch=2)]
    assert len(got) == 3


async def test_missing(client):
    assert (await client.post("/api/runs/99/export/preview", json={})).status_code == 404
    assert (await client.get("/api/runs/99/export")).status_code == 404
    assert (await client.post("/api/preference-sets/99/export/preview", json={})).status_code == 404
    assert (await client.get("/api/preference-sets/99/export")).status_code == 404
    r = await client.post("/api/runs/99/export/hub", json={"target": {"repo_id": "a/b"}})
    assert r.status_code == 404


@pytest.fixture
async def pref(seeded):
    async with SessionLocal() as s:
        pset = PreferenceSet(name="GSM8K prefs", prompt_set_id=seeded["ps"], run_ids=[seeded["run"]])
        s.add(pset)
        await s.flush()
        sm = seeded["samples"]
        s.add_all(
            [
                PreferencePair(preference_set_id=pset.id, prompt_id=seeded["prompts"][0], chosen_sample_id=sm[0], rejected_sample_id=sm[1]),
                PreferencePair(preference_set_id=pset.id, prompt_id=seeded["prompts"][2], chosen_sample_id=sm[3], rejected_sample_id=sm[2]),
            ]
        )
        await s.commit()
        return pset.id


async def test_pref_preview_download(client, pref):
    r = await client.post(f"/api/preference-sets/{pref}/export/preview", json={})
    body = r.json()
    assert body["count"] == 2 and body["filename"] == "gsm8k-prefs-dpo.jsonl"
    assert set(body["example"]) == {"prompt", "chosen", "rejected"}
    assert body["example"]["prompt"][-1]["content"] == "Solve: q0"
    r = await client.post(f"/api/preference-sets/{pref}/export/preview", json={"format": "kto", "conversational": False})
    assert r.json()["count"] == 4 and r.json()["example"]["label"] is True
    assert r.json()["example"]["prompt"] == "sys\n\nSolve: q0"

    r = await client.get(f"/api/preference-sets/{pref}/export", params={"conversational": "false"})
    recs = [json.loads(x) for x in r.text.splitlines()]
    assert len(recs) == 2 and isinstance(recs[0]["chosen"], str)
    assert r.headers["content-disposition"] == 'attachment; filename="gsm8k-prefs-dpo.jsonl"'
    r = await client.get(f"/api/preference-sets/{pref}/export", params={"format": "kto", "file_type": "parquet"})
    path = config.DATA_DIR / "k.parquet"
    path.write_bytes(r.content)
    rows = pq.read_table(path).to_pylist()
    assert [x["label"] for x in rows] == [True, False, True, False]


class FakeApi:
    calls: list = []
    fail: Exception | None = None

    def __init__(self, token=None):
        self.token = token

    def create_repo(self, **kw):
        if FakeApi.fail:
            raise FakeApi.fail
        FakeApi.calls.append(("create_repo", kw))

    def upload_file(self, **kw):
        if isinstance(kw["path_or_fileobj"], str):
            kw = {**kw, "rows": pq.read_table(kw["path_or_fileobj"]).num_rows}
        FakeApi.calls.append(("upload_file", kw))


async def set_token(value="hf_x"):
    async with SessionLocal() as s:
        s.add(Setting(key="hf_token", value=value))
        await s.commit()


async def test_hub_push(client, seeded, pref, monkeypatch):
    FakeApi.calls, FakeApi.fail = [], None
    monkeypatch.setattr(exporters, "HfApi", FakeApi)
    body = {"options": {"format": "messages"}, "target": {"repo_id": "me/ds", "private": True}}
    r = await client.post(f"/api/runs/{seeded['run']}/export/hub", json=body)
    assert r.status_code == 400 and r.json()["detail"] == "Add a Hugging Face token in Settings to push datasets."
    await set_token()
    r = await client.post(f"/api/runs/{seeded['run']}/export/hub", json=body)
    assert r.status_code == 200, r.text
    assert r.json() == {"url": "https://huggingface.co/datasets/me/ds", "count": 3}
    names = [c[0] for c in FakeApi.calls]
    assert names == ["create_repo", "upload_file", "upload_file"]
    assert FakeApi.calls[0][1] == {"repo_id": "me/ds", "repo_type": "dataset", "private": True, "exist_ok": True}
    up = FakeApi.calls[1][1]
    assert up["path_in_repo"] == "data/train-00000-of-00001.parquet" and up["rows"] == 3
    card = FakeApi.calls[2][1]["path_or_fileobj"].decode()
    assert FakeApi.calls[2][1]["path_in_repo"] == "README.md"
    assert card.startswith("---\nconfigs:") and "path: data/train-*" in card
    assert "qwen3-32b" in card and "spark" in card and "GSM8K sample" in card and "Records: 3" in card
    assert list(exporters.export_dir().glob("*.parquet")) == []

    FakeApi.calls = []
    r = await client.post(f"/api/preference-sets/{pref}/export/hub", json={"options": {"format": "kto"}, "target": {"repo_id": "me/p", "split": "test"}})
    assert r.json()["count"] == 4
    assert FakeApi.calls[1][1]["path_in_repo"] == "data/test-00000-of-00001.parquet" and FakeApi.calls[1][1]["rows"] == 4


async def test_hub_errors(client, seeded, monkeypatch):
    from huggingface_hub.errors import HfHubHTTPError

    monkeypatch.setattr(exporters, "HfApi", FakeApi)
    await set_token()
    body = {"target": {"repo_id": "me/ds"}}
    for status, code in [(401, 400), (403, 400), (404, 400), (409, 409)]:
        FakeApi.fail = HfHubHTTPError("boom", response=httpx.Response(status, request=httpx.Request("GET", "http://x")))
        r = await client.post(f"/api/runs/{seeded['run']}/export/hub", json=body)
        assert r.status_code == code, (status, r.text)
        assert r.json()["detail"].endswith(".")
    FakeApi.fail = ConnectionError("down")
    assert (await client.post(f"/api/runs/{seeded['run']}/export/hub", json=body)).status_code == 502
    FakeApi.fail = None
    assert list(exporters.export_dir().glob("*.parquet")) == []
