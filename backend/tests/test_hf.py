from __future__ import annotations

import os
import tempfile
from pathlib import Path

os.environ.setdefault("STELLARATOR_DATA_DIR", tempfile.mkdtemp(prefix="stellarator-hf-"))

import httpx
import pyarrow as pa
import pyarrow.parquet as pq
import pytest
import respx

from stellarator import config
from stellarator.schemas import ImportMapping
from stellarator.services import hf
from stellarator.services import importers as imp

BASE = config.HF_DATASETS_SERVER


@pytest.fixture(autouse=True)
async def _db():
    from stellarator.db import init_db

    await init_db()


def test_normalize_dataset():
    assert hf.normalize_dataset("openai/gsm8k") == "openai/gsm8k"
    assert hf.normalize_dataset("https://huggingface.co/datasets/openai/gsm8k") == "openai/gsm8k"
    assert hf.normalize_dataset("https://huggingface.co/datasets/openai/gsm8k/viewer/main/train?x=1") == "openai/gsm8k"
    assert hf.normalize_dataset("huggingface.co/datasets/squad") == "squad"
    assert hf.normalize_dataset(" squad ") == "squad"
    with pytest.raises(imp.ImportProblem):
        hf.normalize_dataset("not a name!")


def test_parse_parquet_url():
    ref = hf.parse_parquet_url(
        "https://huggingface.co/datasets/openai/gsm8k/resolve/refs%2Fconvert%2Fparquet/main/train/0000.parquet"
    )
    assert ref.repo == "openai/gsm8k"
    assert ref.path == "datasets/openai/gsm8k@refs%2Fconvert%2Fparquet/main/train/0000.parquet"


def mock_ok(router: respx.MockRouter, dataset="o/d"):
    router.get(f"{BASE}/splits", params={"dataset": dataset}).respond(
        json={
            "splits": [
                {"dataset": dataset, "config": "main", "split": "train"},
                {"dataset": dataset, "config": "main", "split": "test"},
            ]
        }
    )
    router.get(f"{BASE}/size").respond(
        json={"size": {"splits": [{"config": "main", "split": "train", "num_rows": 100}, {"config": "main", "split": "test", "num_rows": 10}]}}
    )
    router.get(f"{BASE}/first-rows").respond(
        json={
            "features": [
                {"feature_idx": 0, "name": "question", "type": {"dtype": "string", "_type": "Value"}},
                {"feature_idx": 1, "name": "img", "type": {"_type": "Image"}},
            ],
            "rows": [{"row_idx": i, "row": {"question": f"Q{i}", "img": {"src": "x"}}, "truncated_cells": []} for i in range(5)],
        }
    )


async def test_preview_defaults_and_sizes():
    with respx.mock(assert_all_called=False) as router:
        mock_ok(router)
        data = await hf.fetch_preview("https://huggingface.co/datasets/o/d", None, None)
    assert (data.config, data.split, data.num_rows) == ("main", "train", 100)
    assert [(s.split, s.num_rows) for s in data.splits] == [("train", 100), ("test", 10)]
    pv = imp.preview_source(data.table, ImportMapping(limit=3))
    assert pv.prompt_field == "question" and pv.total_rows == 100 and pv.estimated_prompts == 3
    assert [c.name for c in pv.columns] == ["question"]


async def test_size_failure_ignored_and_token_header(monkeypatch):
    monkeypatch.setattr(hf, "hf_token", lambda: _tok("secret"))
    with respx.mock(assert_all_called=False) as router:
        mock_ok(router)
        router.get(f"{BASE}/size").respond(500)
        data = await hf.fetch_preview("o/d", "main", "test")
        assert data.num_rows is None and data.split == "test"
        assert router.calls[0].request.headers["authorization"] == "Bearer secret"


async def _tok(v):
    return v


async def test_errors_map_to_messages():
    with respx.mock(assert_all_called=False) as router:
        router.get(f"{BASE}/splits").respond(404, json={"error": "nope"})
        with pytest.raises(imp.ImportProblem) as e:
            await hf.fetch_preview("o/missing", None, None)
        assert e.value.status == 404
        assert e.value.message == "No dataset called o/missing on Hugging Face. Check the name."
    with respx.mock(assert_all_called=False) as router:
        router.get(f"{BASE}/splits").respond(401, json={"error": "gated"})
        with pytest.raises(imp.ImportProblem) as e:
            await hf.fetch_preview("o/gated", None, None)
        assert e.value.status == 403 and "token" in e.value.message
    with respx.mock(assert_all_called=False) as router:
        mock_ok(router)
        with pytest.raises(imp.ImportProblem) as e:
            await hf.fetch_preview("o/d", "other", None)
        assert "no config" in e.value.message
    with respx.mock(assert_all_called=False) as router:
        router.get(f"{BASE}/splits").mock(side_effect=httpx.ConnectTimeout("t"))
        with pytest.raises(imp.ImportProblem) as e:
            await hf.fetch_preview("o/d", None, None)
        assert e.value.status == 504


async def test_no_parquet_export():
    with respx.mock(assert_all_called=False) as router:
        mock_ok(router)
        router.get(f"{BASE}/parquet").respond(404, json={"error": "no parquet"})
        with pytest.raises(imp.ImportProblem) as e:
            await hf.fetch_parquet("o/d", None, None)
        assert "can't provide a Parquet export" in e.value.message or "Parquet" in e.value.message
    with respx.mock(assert_all_called=False) as router:
        mock_ok(router)
        router.get(f"{BASE}/parquet").respond(json={"parquet_files": [], "pending": [], "failed": []})
        with pytest.raises(imp.ImportProblem) as e:
            await hf.fetch_parquet("o/d", None, None)
        assert "no Parquet export" in e.value.message


async def test_parquet_rows_across_shards(tmp_path, monkeypatch):
    # Two local shards stand in for Hugging Face's converted Parquet files.
    for i in range(2):
        pq.write_table(
            pa.table({"question": [f"Q{i * 10 + j}" for j in range(10)], "answer": ["a"] * 10}),
            tmp_path / f"000{i}.parquet",
            row_group_size=4,
        )

    class FakeFS:
        def __init__(self, token=None):
            self.token = token

        def open(self, path, mode="rb"):
            assert path.startswith("datasets/o/d@refs%2Fconvert%2Fparquet/main/train/")
            return open(tmp_path / path.rsplit("/", 1)[1], "rb")

    monkeypatch.setattr(hf, "HfFileSystem", FakeFS)
    base = "https://huggingface.co/datasets/o/d/resolve/refs%2Fconvert%2Fparquet/main/train"
    with respx.mock(assert_all_called=False) as router:
        mock_ok(router)
        router.get(f"{BASE}/parquet").respond(
            json={
                "parquet_files": [
                    {"dataset": "o/d", "config": "main", "split": "train", "url": f"{base}/0001.parquet", "filename": "0001.parquet", "size": 1},
                    {"dataset": "o/d", "config": "main", "split": "train", "url": f"{base}/0000.parquet", "filename": "0000.parquet", "size": 1},
                    {"dataset": "o/d", "config": "main", "split": "test", "url": f"{base}/x.parquet", "filename": "x", "size": 1},
                ],
                "pending": [],
                "failed": [],
            }
        )
        export = await hf.fetch_parquet("o/d", None, None)
    assert [r.rest.rsplit("/", 1)[1] for r in export.refs] == ["0000.parquet", "0001.parquet"]
    table = hf.open_export(export, None)
    stream = imp.open_stream(table, ImportMapping(offset=8, limit=6))
    got = [m[-1]["content"] for m, _ in stream]
    assert got == ["Q8", "Q9", "Q10", "Q11", "Q12", "Q13"]
    assert stream.prompt_field == "question"


async def test_live_gsm8k():
    try:
        async with httpx.AsyncClient(timeout=10) as c:
            await c.get(config.HF_DATASETS_SERVER + "/is-valid", params={"dataset": "openai/gsm8k"})
    except httpx.HTTPError:
        pytest.skip("no network")
    export = await hf.fetch_parquet("openai/gsm8k", "main", "train")
    token = await hf.hf_token()

    def run():
        table = hf.open_export(export, token)
        stream = imp.open_stream(table, ImportMapping(limit=50))
        return [m for m, _ in stream], stream.prompt_field

    import asyncio

    prompts, field = await asyncio.to_thread(run)
    assert field == "question" and len(prompts) == 50
    assert prompts[0][-1]["content"].startswith("Natalia sold clips")
