"""Tests for the ML Intern loop refactor.

Covers:
  * research() returns structured JSON with required keys (mock LLM + mock tools)
  * Pre-flight rejected: scale run without preflight -> 412
  * Pre-flight rejected: stale sandbox lineage (>24h) -> 412 + hint
  * Pre-flight accepted: matching schema + valid sandbox_run_id -> 201
  * Alert POST persists; GET returns ordered
  * Doom-loop detector triggers on 3x repeat
  * GitHub service rate-limit handled gracefully (429 -> empty + note)
"""

from __future__ import annotations

from datetime import datetime, timedelta

import pytest
import pytest_asyncio
from httpx import AsyncClient

from app.agents import doom_loop, research_subagent
from app.agents.doom_loop import ToolCall
from app.models.run import Run, RunStatus
from app.services.github import GitHubService

from tests.conftest import CLAUDE_CODE_TOKEN

pytestmark = pytest.mark.asyncio

CLAUDE_HEADERS = {"Authorization": f"Bearer {CLAUDE_CODE_TOKEN}"}

SCALE_PAYLOAD = {
    "name": "Scale Run",
    "base_model": "meta-llama/Llama-3-8b",
    "method": "sft",
    "hyperparams": {"lr": 1e-4},
    "dataset_mixture": [{"name": "alpaca", "weight": 1.0, "source": "huggingface"}],
    "gpu_type": "H100",
    "gpu_count": 8,
    "user_goal": "scale up",
    "agent_plan": "scale plan",
    "citations": [],
    "is_sandbox": False,
}

SANDBOX_PAYLOAD = {
    **SCALE_PAYLOAD,
    "name": "Sandbox",
    "gpu_type": "cpu",
    "gpu_count": 1,
    "is_sandbox": True,
}


def _preflight_for(sandbox_id: str) -> dict:
    return {
        "model": "meta-llama/Llama-3-8b",
        "method": "sft",
        "dataset_mixture": [{"name": "alpaca", "weight": 1.0, "source": "huggingface"}],
        "hyperparams": {"lr": 1e-4, "batch_size": 16},
        "sandbox_run_id": sandbox_id,
        "sandbox_summary": "loss decreased over 50 steps; no OOM",
        "projected_cost_usd": 42.0,
        "citations": [{"source": "arxiv", "id": "2401.12345", "title": "X", "note": ""}],
    }


# ---------------------------------------------------------------------------
# Pre-flight gate
# ---------------------------------------------------------------------------


async def test_scale_run_without_preflight_rejected(client: AsyncClient):
    resp = await client.post("/v1/runs/", json=SCALE_PAYLOAD, headers=CLAUDE_HEADERS)
    assert resp.status_code == 412, resp.text
    body = resp.json()
    assert body["detail"]["error"] == "preflight_missing"


async def test_scale_run_stale_sandbox_lineage(client: AsyncClient, session):
    # Insert a sandbox run owned by claude-code that finished 25h ago.
    stale = Run(
        id="sandbox-stale",
        owner_agent="claude-code",
        name="stale sandbox",
        status=RunStatus.succeeded.value,
        base_model="meta-llama/Llama-3-8b",
        method="sft",
        hyperparams={},
        dataset_mixture=[],
        citations=[],
        gpu_type="cpu",
        gpu_count=1,
        is_sandbox=True,
        finished_at=datetime.utcnow() - timedelta(hours=25),
    )
    session.add(stale)
    await session.commit()

    payload = {
        **SCALE_PAYLOAD,
        "preflight_json": _preflight_for("sandbox-stale"),
        "sandbox_run_id": "sandbox-stale",
    }
    resp = await client.post("/v1/runs/", json=payload, headers=CLAUDE_HEADERS)
    assert resp.status_code == 412, resp.text
    body = resp.json()
    assert body["detail"]["error"] == "sandbox_stale"
    assert "hint" in body["detail"]


async def test_scale_run_with_valid_preflight_accepted(client: AsyncClient, session):
    fresh = Run(
        id="sandbox-fresh",
        owner_agent="claude-code",
        name="fresh sandbox",
        status=RunStatus.succeeded.value,
        base_model="meta-llama/Llama-3-8b",
        method="sft",
        hyperparams={},
        dataset_mixture=[],
        citations=[],
        gpu_type="cpu",
        gpu_count=1,
        is_sandbox=True,
        finished_at=datetime.utcnow() - timedelta(minutes=10),
    )
    session.add(fresh)
    await session.commit()

    payload = {
        **SCALE_PAYLOAD,
        "preflight_json": _preflight_for("sandbox-fresh"),
        "sandbox_run_id": "sandbox-fresh",
    }
    resp = await client.post("/v1/runs/", json=payload, headers=CLAUDE_HEADERS)
    assert resp.status_code == 201, resp.text


async def test_sandbox_run_bypasses_preflight(client: AsyncClient):
    resp = await client.post("/v1/runs/", json=SANDBOX_PAYLOAD, headers=CLAUDE_HEADERS)
    assert resp.status_code == 201, resp.text
    assert resp.json()["owner_agent"] == "claude-code"


# ---------------------------------------------------------------------------
# Alerts
# ---------------------------------------------------------------------------


async def test_alerts_post_and_get_ordered(client: AsyncClient):
    sandbox = await client.post(
        "/v1/runs/", json=SANDBOX_PAYLOAD, headers=CLAUDE_HEADERS
    )
    run_id = sandbox.json()["id"]

    for level, title in [
        ("info", "step 10"),
        ("warn", "loss spike"),
        ("error", "nan"),
    ]:
        r = await client.post(
            f"/v1/runs/{run_id}/alerts",
            json={"level": level, "title": title, "body": "..."},
            headers=CLAUDE_HEADERS,
        )
        assert r.status_code == 201, r.text

    r = await client.get(f"/v1/runs/{run_id}/alerts", headers=CLAUDE_HEADERS)
    assert r.status_code == 200
    rows = r.json()
    assert [row["title"] for row in rows] == ["step 10", "loss spike", "nan"]


# ---------------------------------------------------------------------------
# Doom-loop detector
# ---------------------------------------------------------------------------


def test_doom_loop_triggers_on_3x_repeat():
    history = [
        ToolCall("read_alerts", {"run_id": "abc"}),
        ToolCall("read_alerts", {"run_id": "abc"}),
        ToolCall("read_alerts", {"run_id": "abc"}),
    ]
    note = doom_loop.detect(history)
    assert note is not None
    assert "DOOM LOOP" in note
    assert "read_alerts" in note


def test_doom_loop_no_trigger_on_varied_calls():
    history = [
        ToolCall("read_alerts", {"run_id": "abc"}),
        ToolCall("read_run", {"run_id": "abc"}),
        ToolCall("read_alerts", {"run_id": "abc"}),
    ]
    assert doom_loop.detect(history) is None


def test_doom_loop_default_suggestion_for_unknown_tool():
    history = [
        ToolCall("weird_custom_tool", {"x": 1}),
        ToolCall("weird_custom_tool", {"x": 1}),
        ToolCall("weird_custom_tool", {"x": 1}),
    ]
    note = doom_loop.detect(history)
    assert note is not None and "weird_custom_tool" in note


# ---------------------------------------------------------------------------
# Research sub-agent
# ---------------------------------------------------------------------------


async def test_research_returns_structured_json(monkeypatch):
    """Mock LLM emits a final JSON answer immediately; verify schema coercion."""

    final = {
        "summary": "use lr=1e-5 with cosine schedule",
        "datasets": [{"name": "alpaca", "weight": 1.0, "source": "hf"}],
        "hyperparams": {"lr": 1e-5, "batch_size": 8},
        "code_snippets": [
            {"lang": "python", "source": "huggingface/trl:examples/sft.py", "body": "..."}
        ],
        "citations": [
            {
                "source": "arxiv",
                "id": "2401.12345",
                "title": "Direct Preference Optimization",
                "url": "https://arxiv.org/abs/2401.12345",
                "relevance_score": 0.92,
            }
        ],
    }

    import json as _json

    async def fake_llm(messages, tools):
        return {
            "choices": [
                {"message": {"role": "assistant", "content": _json.dumps(final), "tool_calls": []}}
            ]
        }

    result = await research_subagent.research(
        "best lr for SFT on Llama-3-8B",
        context="user wants instruction following",
        agent="claude-code",
        driver=fake_llm,
    )

    for key in research_subagent._REQUIRED_KEYS:
        assert key in result
    assert result["summary"].startswith("use lr=1e-5")
    assert result["citations"][0]["id"] == "2401.12345"


async def test_research_handles_no_driver(monkeypatch):
    research_subagent.CALLING_DRIVER.set(None)
    result = await research_subagent.research(
        "task", context="ctx", agent="claude-code"
    )
    assert "summary" in result
    assert result["datasets"] == []


# ---------------------------------------------------------------------------
# GitHub service rate limit
# ---------------------------------------------------------------------------


async def test_github_search_rate_limited(monkeypatch):
    """If GitHub returns 429, the service must return empty list + note."""
    import httpx

    class FakeResp:
        status_code = 429
        text = ""

        def json(self):
            return {}

    class FakeClient:
        def __init__(self, *a, **kw):
            pass

        async def __aenter__(self):
            return self

        async def __aexit__(self, *a):
            return None

        async def get(self, *a, **kw):
            return FakeResp()

    monkeypatch.setattr(httpx, "AsyncClient", FakeClient)
    svc = GitHubService()
    out = await svc.find_examples("dpo training", lang="python", limit=3)
    assert out["items"] == []
    assert out["note"] == "rate_limited"
