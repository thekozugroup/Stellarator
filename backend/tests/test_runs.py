"""Tests for run management endpoints."""

from __future__ import annotations

import pytest
import pytest_asyncio
from httpx import AsyncClient

from tests.conftest import CLAUDE_CODE_TOKEN, OPENAI_TOKEN

pytestmark = pytest.mark.asyncio

CLAUDE_HEADERS = {"Authorization": f"Bearer {CLAUDE_CODE_TOKEN}"}
OPENAI_HEADERS = {"Authorization": f"Bearer {OPENAI_TOKEN}"}

RUN_PAYLOAD = {
    "name": "Test Run",
    "base_model": "meta-llama/Llama-3-8b",
    "method": "sft",
    "hyperparams": {"lr": 1e-4, "epochs": 3},
    "dataset_mixture": [{"name": "alpaca", "weight": 1.0, "source": "huggingface"}],
    "gpu_type": "H100",
    "gpu_count": 1,
    "user_goal": "Improve instruction following",
    "user_context": "Research project",
    "agent_plan": "Standard SFT pipeline",
    "citations": [],
}


# ---------------------------------------------------------------------------
# Create run
# ---------------------------------------------------------------------------


async def test_create_run_success(client: AsyncClient):
    resp = await client.post("/v1/runs/", json=RUN_PAYLOAD, headers=CLAUDE_HEADERS)
    assert resp.status_code == 201, resp.text
    data = resp.json()
    assert data["owner_agent"] == "claude-code"
    assert data["method"] == "sft"
    assert data["tinker_job_id"] == "fake-tinker-job-id"
    assert len(data["id"]) == 32  # uuid4 hex


async def test_create_run_invalid_method(client: AsyncClient):
    payload = {**RUN_PAYLOAD, "method": "not_a_method"}
    resp = await client.post("/v1/runs/", json=payload, headers=CLAUDE_HEADERS)
    assert resp.status_code == 422


async def test_create_run_invalid_dataset_mixture(client: AsyncClient):
    payload = {**RUN_PAYLOAD, "dataset_mixture": [{"name": "x"}]}  # missing weight + source
    resp = await client.post("/v1/runs/", json=payload, headers=CLAUDE_HEADERS)
    assert resp.status_code == 422


async def test_create_run_unauthorized(client: AsyncClient):
    resp = await client.post("/v1/runs/", json=RUN_PAYLOAD)
    assert resp.status_code == 401


# ---------------------------------------------------------------------------
# List runs
# ---------------------------------------------------------------------------


async def test_list_runs_empty(client: AsyncClient):
    resp = await client.get("/v1/runs/", headers=CLAUDE_HEADERS)
    assert resp.status_code == 200
    assert resp.json() == []


async def test_list_runs_status_filter(client: AsyncClient):
    # Create a run
    await client.post("/v1/runs/", json=RUN_PAYLOAD, headers=CLAUDE_HEADERS)
    # Filter by queued (default status)
    resp = await client.get("/v1/runs/?status=queued", headers=CLAUDE_HEADERS)
    assert resp.status_code == 200
    assert len(resp.json()) == 1
    # Filter by running (should be empty)
    resp = await client.get("/v1/runs/?status=running", headers=CLAUDE_HEADERS)
    assert resp.status_code == 200
    assert resp.json() == []


async def test_list_runs_invalid_status(client: AsyncClient):
    resp = await client.get("/v1/runs/?status=bogus", headers=CLAUDE_HEADERS)
    assert resp.status_code == 422


async def test_list_runs_owner_filter(client: AsyncClient):
    # claude-code creates a run
    await client.post("/v1/runs/", json=RUN_PAYLOAD, headers=CLAUDE_HEADERS)
    # filter by openai — should see none
    resp = await client.get("/v1/runs/?owner=openai", headers=OPENAI_HEADERS)
    assert resp.status_code == 200
    assert resp.json() == []
    # filter by claude-code
    resp = await client.get("/v1/runs/?owner=claude-code", headers=OPENAI_HEADERS)
    assert resp.status_code == 200
    assert len(resp.json()) == 1


# ---------------------------------------------------------------------------
# Get run detail
# ---------------------------------------------------------------------------


async def test_get_run_detail(client: AsyncClient):
    cr = await client.post("/v1/runs/", json=RUN_PAYLOAD, headers=CLAUDE_HEADERS)
    run_id = cr.json()["id"]
    resp = await client.get(f"/v1/runs/{run_id}", headers=OPENAI_HEADERS)
    assert resp.status_code == 200
    data = resp.json()
    assert data["id"] == run_id
    assert "notes" in data
    assert "metrics" in data


async def test_get_run_not_found(client: AsyncClient):
    resp = await client.get("/v1/runs/doesnotexist", headers=CLAUDE_HEADERS)
    assert resp.status_code == 404


# ---------------------------------------------------------------------------
# Cancel / pause / resume — ownership enforcement
# ---------------------------------------------------------------------------


async def test_cancel_run_owner_only(client: AsyncClient):
    cr = await client.post("/v1/runs/", json=RUN_PAYLOAD, headers=CLAUDE_HEADERS)
    run_id = cr.json()["id"]

    # openai cannot cancel
    resp = await client.post(f"/v1/runs/{run_id}/cancel", headers=OPENAI_HEADERS)
    assert resp.status_code == 403

    # owner can cancel
    resp = await client.post(f"/v1/runs/{run_id}/cancel", headers=CLAUDE_HEADERS)
    assert resp.status_code == 200
    assert resp.json()["status"] == "cancelled"


async def test_pause_resume_owner_only(client: AsyncClient):
    cr = await client.post("/v1/runs/", json=RUN_PAYLOAD, headers=CLAUDE_HEADERS)
    run_id = cr.json()["id"]

    # openai cannot pause
    resp = await client.post(f"/v1/runs/{run_id}/pause", headers=OPENAI_HEADERS)
    assert resp.status_code == 403

    # owner pauses
    resp = await client.post(f"/v1/runs/{run_id}/pause", headers=CLAUDE_HEADERS)
    assert resp.status_code == 200
    assert resp.json()["status"] == "paused"

    # owner resumes
    resp = await client.post(f"/v1/runs/{run_id}/resume", headers=CLAUDE_HEADERS)
    assert resp.status_code == 200
    assert resp.json()["status"] == "running"


# ---------------------------------------------------------------------------
# Notes
# ---------------------------------------------------------------------------


async def test_add_note_owner_only(client: AsyncClient):
    cr = await client.post("/v1/runs/", json=RUN_PAYLOAD, headers=CLAUDE_HEADERS)
    run_id = cr.json()["id"]

    # openai cannot add note
    resp = await client.post(
        f"/v1/runs/{run_id}/notes",
        json={"kind": "progress", "body": "50% done"},
        headers=OPENAI_HEADERS,
    )
    assert resp.status_code == 403

    # owner can add note
    resp = await client.post(
        f"/v1/runs/{run_id}/notes",
        json={"kind": "progress", "body": "50% done"},
        headers=CLAUDE_HEADERS,
    )
    assert resp.status_code == 201
    note = resp.json()
    assert note["author_agent"] == "claude-code"
    assert note["kind"] == "progress"


async def test_list_notes_any_agent(client: AsyncClient):
    cr = await client.post("/v1/runs/", json=RUN_PAYLOAD, headers=CLAUDE_HEADERS)
    run_id = cr.json()["id"]

    await client.post(
        f"/v1/runs/{run_id}/notes",
        json={"kind": "plan", "body": "Initial plan"},
        headers=CLAUDE_HEADERS,
    )

    # openai can read notes
    resp = await client.get(f"/v1/runs/{run_id}/notes", headers=OPENAI_HEADERS)
    assert resp.status_code == 200
    assert len(resp.json()) == 1


async def test_note_invalid_kind(client: AsyncClient):
    cr = await client.post("/v1/runs/", json=RUN_PAYLOAD, headers=CLAUDE_HEADERS)
    run_id = cr.json()["id"]
    resp = await client.post(
        f"/v1/runs/{run_id}/notes",
        json={"kind": "nonsense", "body": "x"},
        headers=CLAUDE_HEADERS,
    )
    assert resp.status_code == 422


# ---------------------------------------------------------------------------
# Whoami
# ---------------------------------------------------------------------------


async def test_whoami(client: AsyncClient):
    resp = await client.get("/v1/whoami", headers=CLAUDE_HEADERS)
    assert resp.status_code == 200
    assert resp.json() == {"agent": "claude-code"}


async def test_whoami_openai(client: AsyncClient):
    resp = await client.get("/v1/whoami", headers=OPENAI_HEADERS)
    assert resp.status_code == 200
    assert resp.json() == {"agent": "openai"}


# ---------------------------------------------------------------------------
# Healthz
# ---------------------------------------------------------------------------


async def test_healthz(client: AsyncClient):
    resp = await client.get("/healthz")
    assert resp.status_code == 200
    assert resp.json()["status"] == "ok"
