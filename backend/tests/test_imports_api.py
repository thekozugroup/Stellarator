from __future__ import annotations

import os
import tempfile
import time
from pathlib import Path

os.environ.setdefault("STELLARATOR_DATA_DIR", tempfile.mkdtemp(prefix="stellarator-api-"))

import httpx
import pytest
import respx
from sqlalchemy import func, select

from stellarator import config
from stellarator.api import imports as imports_api
from stellarator.config import UPLOAD_DIR
from stellarator.db import SessionLocal, init_db
from stellarator.main import app
from stellarator.models import Prompt

FIX = Path(__file__).parent / "fixtures" / "imports"
HF = config.HF_DATASETS_SERVER


@pytest.fixture
async def client():
    await init_db()
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://t") as c:
        yield c


async def upload(client, name: str, data: bytes):
    r = await client.post("/api/imports/upload", files={"file": (name, data)})
    assert r.status_code == 200, r.text
    return r.json()


async def test_upload_preview_commit_csv(client):
    up = await upload(client, "../my prompts.csv", (FIX / "bom_quoted.csv").read_bytes())
    assert up["filename"] == "my prompts.csv" and up["detected_format"] == "csv"
    assert (UPLOAD_DIR / up["upload_id"] / "my prompts.csv").is_file()
    src = {"kind": "file", "upload_id": up["upload_id"], "format": "auto"}
    r = await client.post("/api/imports/preview", json={"source": src})
    assert r.status_code == 200, r.text
    pv = r.json()
    assert pv["detected_format"] == "csv" and pv["prompt_field"] == "prompt"
    assert pv["total_rows"] == 2 and pv["estimated_prompts"] == 2
    assert pv["prompts"][0] == [{"role": "user", "content": "Line one\nline two"}]
    assert [c["kind"] for c in pv["columns"]] == ["number", "text", "text"]

    r = await client.post("/api/imports", json={"source": src, "name": "My set"})
    assert r.status_code == 201, r.text
    ps = r.json()
    assert ps["prompt_count"] == 2 and ps["source_kind"] == "file"
    assert ps["source"]["filename"] == "my prompts.csv" and ps["source"]["prompt_field"] == "prompt"
    assert not (UPLOAD_DIR / up["upload_id"]).exists()
    page = (await client.get(f"/api/prompt-sets/{ps['id']}/prompts")).json()
    assert page["total"] == 2 and page["items"][1]["messages"][0]["content"] == 'Say "hi"'
    assert page["items"][0]["meta"] == {"id": "1", "answer": "a"}


async def test_paste_preview_commit(client):
    src = {"kind": "paste", "text": "# A\nx\n# B\ny"}
    pv = (await client.post("/api/imports/preview", json={"source": src})).json()
    assert pv["split_mode"] == "heading" and len(pv["prompts"]) == 2
    r = await client.post("/api/imports", json={"source": src, "name": "Pasted", "mapping": {"split_mode": "line"}})
    assert r.status_code == 201 and r.json()["prompt_count"] == 4
    assert r.json()["source_kind"] == "paste"


async def test_errors(client):
    r = await client.post("/api/imports/preview", json={"source": {"kind": "file", "upload_id": "0" * 32}})
    assert r.status_code == 404
    r = await client.post("/api/imports/preview", json={"source": {"kind": "file", "upload_id": "../x"}})
    assert r.status_code == 404
    r = await client.post("/api/imports", json={"source": {"kind": "paste", "text": "   "}, "name": "x"})
    assert r.status_code == 422
    up = await upload(client, "a.csv", b"prompt\nhi\n")
    src = {"kind": "file", "upload_id": up["upload_id"]}
    r = await client.post("/api/imports", json={"source": src, "mapping": {"prompt_field": "zzz"}, "name": "x"})
    assert r.status_code == 422 and "zzz" in r.json()["detail"]
    # nothing created for a failed import, upload still there for a retry
    assert (UPLOAD_DIR / up["upload_id"]).exists()
    r = await client.post("/api/imports/upload", files={"file": ("e.txt", b"")})
    assert r.status_code == 422


async def test_upload_too_large(client, monkeypatch):
    monkeypatch.setattr(imports_api, "MAX_UPLOAD_BYTES", 10)
    r = await client.post("/api/imports/upload", files={"file": ("big.txt", b"x" * 100)})
    assert r.status_code == 413 and "1 GB" in r.json()["detail"]
    assert not any(UPLOAD_DIR.glob("*/big.txt"))


async def test_upload_cleans_old(client):
    old = UPLOAD_DIR / ("a" * 32)
    old.mkdir(exist_ok=True)
    (old / "f.txt").write_text("x")
    t = time.time() - 30 * 3600
    os.utime(old, (t, t))
    await upload(client, "n.txt", b"hello")
    assert not old.exists()


async def test_commit_100k_rows_fast(client):
    data = "prompt,n\n" + "".join(f"Question {i} here,{i}\n" for i in range(100_000))
    up = await upload(client, "big.csv", data.encode())
    t0 = time.time()
    r = await client.post("/api/imports", json={"source": {"kind": "file", "upload_id": up["upload_id"]}, "name": "big"})
    elapsed = time.time() - t0
    assert r.status_code == 201 and r.json()["prompt_count"] == 100_000
    assert elapsed < 10, elapsed
    async with SessionLocal() as s:
        n = (await s.execute(select(func.count()).select_from(Prompt).where(Prompt.prompt_set_id == r.json()["id"]))).scalar_one()
        assert n == 100_000


async def test_hf_preview_and_errors(client):
    with respx.mock(assert_all_called=False) as router:
        router.get(f"{HF}/splits").respond(json={"splits": [{"dataset": "o/d", "config": "default", "split": "train"}]})
        router.get(f"{HF}/size").respond(json={"size": {"splits": [{"config": "default", "split": "train", "num_rows": 500}]}})
        router.get(f"{HF}/first-rows").respond(
            json={
                "features": [{"name": "prompt", "type": {"dtype": "string", "_type": "Value"}}],
                "rows": [{"row_idx": i, "row": {"prompt": f"P{i}"}} for i in range(30)],
            }
        )
        r = await client.post(
            "/api/imports/preview",
            json={"source": {"kind": "huggingface", "dataset": "https://huggingface.co/datasets/o/d"}, "mapping": {"limit": 100}},
        )
    pv = r.json()
    assert r.status_code == 200, pv
    assert pv["hf_config"] == "default" and pv["hf_split"] == "train"
    assert pv["total_rows"] == 500 and pv["estimated_prompts"] == 100
    assert pv["hf_splits"] == [{"config": "default", "split": "train", "num_rows": 500}]
    assert len(pv["prompts"]) == 20

    with respx.mock(assert_all_called=False) as router:
        router.get(f"{HF}/splits").respond(404, json={"error": "x"})
        r = await client.post("/api/imports/preview", json={"source": {"kind": "huggingface", "dataset": "o/nope"}})
    assert r.status_code == 404
    assert r.json()["detail"] == "No dataset called o/nope on Hugging Face. Check the name."


async def test_hf_commit_live_gsm8k(client):
    try:
        async with httpx.AsyncClient(timeout=10) as c:
            await c.get(HF + "/is-valid", params={"dataset": "openai/gsm8k"})
    except httpx.HTTPError:
        pytest.skip("no network")
    r = await client.post(
        "/api/imports",
        json={
            "source": {"kind": "huggingface", "dataset": "openai/gsm8k", "config": "main", "split": "train"},
            "mapping": {"limit": 50},
            "name": "gsm8k 50",
        },
    )
    assert r.status_code == 201, r.text
    ps = r.json()
    assert ps["prompt_count"] == 50 and ps["source_kind"] == "huggingface"
    assert ps["source"]["dataset"] == "openai/gsm8k" and ps["source"]["prompt_field"] == "question"
    first = (await client.get(f"/api/prompt-sets/{ps['id']}/prompts?limit=1")).json()["items"][0]
    assert first["messages"][-1]["content"].startswith("Natalia sold clips")
    assert "answer" in first["meta"]
