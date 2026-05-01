"""Tests for StellaratorClient using respx to mock the backend."""

from __future__ import annotations

import os
import pytest
import respx
import httpx

# Ensure token env var is set before importing the client module
os.environ.setdefault("STELLARATOR_TOKEN", "test-token-abc")

from stellarator_mcp.client import StellaratorClient, StellaratorAPIError


BASE = "http://localhost:8000"


@pytest.fixture
def client():
    return StellaratorClient(base_url=BASE)


# ---------------------------------------------------------------------------
# Happy-path tests
# ---------------------------------------------------------------------------


@respx.mock
@pytest.mark.asyncio
async def test_create_run_happy(client):
    run_payload = {
        "name": "llama3-lora-test",
        "base_model": "meta-llama/Meta-Llama-3-8B",
        "method": "lora",
        "dataset_mixture": ["tatsu-lab/alpaca"],
    }
    expected = {"id": "run-001", **run_payload, "status": "queued"}

    respx.post(f"{BASE}/v1/runs").mock(
        return_value=httpx.Response(201, json=expected)
    )

    result = await client.create_run(run_payload)
    assert result["id"] == "run-001"
    assert result["status"] == "queued"


@respx.mock
@pytest.mark.asyncio
async def test_list_runs_happy(client):
    runs = [
        {"id": "run-001", "name": "run-a", "status": "completed"},
        {"id": "run-002", "name": "run-b", "status": "running"},
    ]
    respx.get(f"{BASE}/v1/runs").mock(return_value=httpx.Response(200, json=runs))

    result = await client.list_runs()
    assert len(result) == 2
    assert result[0]["id"] == "run-001"


@respx.mock
@pytest.mark.asyncio
async def test_list_runs_filtered(client):
    """list_runs passes owner/status query params correctly."""
    route = respx.get(f"{BASE}/v1/runs").mock(
        return_value=httpx.Response(200, json=[])
    )

    await client.list_runs(owner="claude-code", status="running", limit=5)

    request = route.calls.last.request
    assert "owner=claude-code" in str(request.url)
    assert "status=running" in str(request.url)
    assert "limit=5" in str(request.url)


@respx.mock
@pytest.mark.asyncio
async def test_get_run_happy(client):
    run = {"id": "run-001", "status": "running", "metrics": {"loss": 0.42}}
    respx.get(f"{BASE}/v1/runs/run-001").mock(return_value=httpx.Response(200, json=run))

    result = await client.get_run("run-001")
    assert result["metrics"]["loss"] == pytest.approx(0.42)


@respx.mock
@pytest.mark.asyncio
async def test_cancel_run_happy(client):
    respx.post(f"{BASE}/v1/runs/run-001/cancel").mock(
        return_value=httpx.Response(200, json={"status": "cancelled"})
    )
    result = await client.cancel_run("run-001")
    assert result["status"] == "cancelled"


@respx.mock
@pytest.mark.asyncio
async def test_pause_run_happy(client):
    respx.post(f"{BASE}/v1/runs/run-001/pause").mock(
        return_value=httpx.Response(200, json={"status": "paused"})
    )
    result = await client.pause_run("run-001")
    assert result["status"] == "paused"


@respx.mock
@pytest.mark.asyncio
async def test_resume_run_happy(client):
    respx.post(f"{BASE}/v1/runs/run-001/resume").mock(
        return_value=httpx.Response(200, json={"status": "running"})
    )
    result = await client.resume_run("run-001")
    assert result["status"] == "running"


@respx.mock
@pytest.mark.asyncio
async def test_add_note_happy(client):
    expected = {"id": "note-007", "kind": "observation", "body": "Loss plateaued."}
    respx.post(f"{BASE}/v1/runs/run-001/notes").mock(
        return_value=httpx.Response(201, json=expected)
    )
    result = await client.add_note("run-001", "observation", "Loss plateaued.")
    assert result["kind"] == "observation"


@respx.mock
@pytest.mark.asyncio
async def test_search_papers_happy(client):
    papers = [{"paper_id": "2106.09685", "source": "arxiv", "title": "LoRA"}]
    respx.get(f"{BASE}/v1/research/papers/search").mock(
        return_value=httpx.Response(200, json=papers)
    )
    result = await client.search_papers("LoRA fine-tuning")
    assert result[0]["paper_id"] == "2106.09685"


@respx.mock
@pytest.mark.asyncio
async def test_get_paper_happy(client):
    paper = {"paper_id": "2106.09685", "source": "arxiv", "abstract": "..."}
    respx.get(f"{BASE}/v1/research/papers/arxiv/2106.09685").mock(
        return_value=httpx.Response(200, json=paper)
    )
    result = await client.get_paper("arxiv", "2106.09685")
    assert result["paper_id"] == "2106.09685"


@respx.mock
@pytest.mark.asyncio
async def test_cite_paper_happy(client):
    expected = {"id": "cite-01", "source": "arxiv", "paper_id": "2106.09685"}
    respx.post(f"{BASE}/v1/research/runs/run-001/cite").mock(
        return_value=httpx.Response(201, json=expected)
    )
    result = await client.cite_paper(
        "run-001", "arxiv", "2106.09685", "Used to select LoRA rank."
    )
    assert result["id"] == "cite-01"


# ---------------------------------------------------------------------------
# Error / ownership tests
# ---------------------------------------------------------------------------


@respx.mock
@pytest.mark.asyncio
async def test_403_ownership_error(client):
    """403 from cancel must surface as a human-readable ownership message."""
    respx.post(f"{BASE}/v1/runs/run-999/cancel").mock(
        return_value=httpx.Response(403, json={"detail": "forbidden"})
    )

    with pytest.raises(StellaratorAPIError) as exc_info:
        await client.cancel_run("run-999")

    msg = str(exc_info.value)
    assert "owned by another agent" in msg
    assert exc_info.value.status_code == 403


@respx.mock
@pytest.mark.asyncio
async def test_403_ownership_on_pause(client):
    """403 on pause also reports ownership correctly."""
    respx.post(f"{BASE}/v1/runs/run-888/pause").mock(
        return_value=httpx.Response(403, json={"detail": "not your run"})
    )

    with pytest.raises(StellaratorAPIError) as exc_info:
        await client.pause_run("run-888")

    assert "owned by another agent" in str(exc_info.value)


@respx.mock
@pytest.mark.asyncio
async def test_401_token_hint(client):
    """401 must mention STELLARATOR_TOKEN in the error message."""
    respx.get(f"{BASE}/v1/runs").mock(return_value=httpx.Response(401))

    with pytest.raises(StellaratorAPIError) as exc_info:
        await client.list_runs()

    assert "STELLARATOR_TOKEN" in str(exc_info.value)
    assert exc_info.value.status_code == 401


@respx.mock
@pytest.mark.asyncio
async def test_404_error(client):
    respx.get(f"{BASE}/v1/runs/nonexistent").mock(
        return_value=httpx.Response(404, json={"detail": "not found"})
    )

    with pytest.raises(StellaratorAPIError) as exc_info:
        await client.get_run("nonexistent")

    assert exc_info.value.status_code == 404


@respx.mock
@pytest.mark.asyncio
async def test_bearer_token_sent(client):
    """Verify Authorization header is sent (without logging the value in test output)."""
    route = respx.get(f"{BASE}/v1/runs").mock(
        return_value=httpx.Response(200, json=[])
    )

    await client.list_runs()

    request = route.calls.last.request
    auth_header = request.headers.get("authorization", "")
    assert auth_header.startswith("Bearer "), "Authorization header missing or malformed"
    # Do NOT print the token value — just verify format.
    assert len(auth_header) > len("Bearer "), "Token appears empty"
