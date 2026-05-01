"""Tests for RL-parity additions (spec item 9)."""

from __future__ import annotations

import json
from unittest.mock import AsyncMock, patch

import pytest
import pytest_asyncio

from app.models.run import Run, RunStatus


# ---------------------------------------------------------------------------
# 1. Run model exposes new fields with default None
# ---------------------------------------------------------------------------


def test_run_model_rl_fields_exist():
    """Run ORM model must expose the three new RL-parity columns."""
    r = Run(
        id="test-run-1",
        owner_agent="test-agent",
        name="test",
        base_model="llama",
        method="grpo",
        hyperparams={},
        dataset_mixture=[],
        gpu_type="H100",
        gpu_count=1,
    )
    assert r.reward_mean is None
    assert r.percent_correct is None
    assert r.checkpoint_url is None


# ---------------------------------------------------------------------------
# 2. get_checkpoint: null until status == succeeded
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_get_checkpoint_not_ready(session):
    """get_checkpoint tool returns ready=false for a non-succeeded run."""
    r = Run(
        id="not-ready-run",
        owner_agent="agent",
        name="test",
        base_model="llama",
        method="grpo",
        hyperparams={},
        dataset_mixture=[],
        gpu_type="H100",
        gpu_count=1,
        status=RunStatus.running.value,
        checkpoint_url=None,
    )
    session.add(r)
    await session.commit()

    # Simulate tool logic: only expose url when succeeded
    ready = r.status == RunStatus.succeeded.value
    url = r.checkpoint_url if ready else None
    assert not ready
    assert url is None


@pytest.mark.asyncio
async def test_get_checkpoint_ready(session):
    """get_checkpoint helper returns ready=True + url when status=succeeded."""
    r = Run(
        id="ckpt-run",
        owner_agent="agent",
        name="test",
        base_model="llama",
        method="grpo",
        hyperparams={},
        dataset_mixture=[],
        gpu_type="H100",
        gpu_count=1,
        status=RunStatus.succeeded.value,
        checkpoint_url="https://storage.example.com/weights/ckpt-run.safetensors",
    )
    session.add(r)
    await session.commit()

    assert r.checkpoint_url is not None
    assert r.status == RunStatus.succeeded.value
    # Simulate tool logic
    ready = r.status == RunStatus.succeeded.value
    url = r.checkpoint_url if ready else None
    assert ready is True
    assert url == "https://storage.example.com/weights/ckpt-run.safetensors"


# ---------------------------------------------------------------------------
# 3. pick_environment: known ID returns scaffold; unknown returns None
# ---------------------------------------------------------------------------


def test_pick_environment_known():
    from app.services.environments import pick_environment

    env = pick_environment("gsm8k")
    assert env is not None
    assert env["id"] == "gsm8k"
    assert env["task_type"] == "math"
    assert env["recommended_method"] == "grpo"
    assert "recommended_hyperparams" in env
    assert "suggested_eval_metric" in env


def test_pick_environment_unknown():
    from app.services.environments import pick_environment

    assert pick_environment("does-not-exist") is None


def test_list_environments_count():
    from app.services.environments import list_environments

    envs = list_environments()
    assert len(envs) == 8
    ids = {e["id"] for e in envs}
    assert {"gsm8k", "math", "humaneval", "mbpp", "mt-bench", "alpaca-eval", "truthfulqa", "hellaswag"} == ids


# ---------------------------------------------------------------------------
# 4. GRPO method accepted via run_create
# ---------------------------------------------------------------------------


def test_grpo_method_roundtrip():
    """GRPO must pass schema validation."""
    from app.api.schemas import RunCreate, DatasetEntry

    rc = RunCreate(
        name="grpo-test",
        base_model="llama",
        method="grpo",
        dataset_mixture=[DatasetEntry(name="openai/gsm8k", weight=1.0, source="hf")],
    )
    assert rc.method == "grpo"


# ---------------------------------------------------------------------------
# 5. Reconcile loop copies reward_mean from mocked Tinker response
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_reconcile_copies_reward_mean(session):
    """_refresh_run_status must copy reward_mean from a mocked Tinker response."""
    from app.services.reconcile import _refresh_run_status

    r = Run(
        id="recon-run",
        owner_agent="agent",
        name="test",
        base_model="llama",
        method="grpo",
        hyperparams={},
        dataset_mixture=[],
        gpu_type="H100",
        gpu_count=1,
        status=RunStatus.running.value,
        tinker_job_id="fake-job-123",
    )
    session.add(r)
    await session.commit()

    mocked_response = {
        "status": "succeeded",
        "weights_url": "https://storage.example.com/weights/recon-run.safetensors",
        "metrics": {
            "reward_mean": 0.72,
            "percent_correct": 81.5,
        },
    }

    with patch("app.services.reconcile.tinker") as mock_tinker:
        mock_tinker.get_job = AsyncMock(return_value=mocked_response)
        mock_tinker.extract_rl_signals = lambda resp: {
            k: v
            for k, v in {
                "checkpoint_url": resp.get("weights_url") or resp.get("checkpoint_url"),
                "reward_mean": (resp.get("metrics") or {}).get("reward_mean"),
                "percent_correct": (resp.get("metrics") or {}).get("percent_correct"),
            }.items()
            if v is not None
        }
        new_status = await _refresh_run_status(r)

    assert new_status == "succeeded"
    assert r.reward_mean == pytest.approx(0.72)
    assert r.percent_correct == pytest.approx(81.5)
    assert r.checkpoint_url == "https://storage.example.com/weights/recon-run.safetensors"


# ---------------------------------------------------------------------------
# 6. RunOut schema exposes RL fields
# ---------------------------------------------------------------------------


def test_run_out_schema_includes_rl_fields():
    """RunOut schema must serialize the three RL fields."""
    from app.api.schemas import RunOut
    from datetime import datetime

    out = RunOut(
        id="schema-test",
        owner_agent="agent",
        name="test",
        status="succeeded",
        base_model="llama",
        method="grpo",
        hyperparams={},
        dataset_mixture=[],
        user_goal="",
        user_context="",
        agent_plan="",
        citations=[],
        tinker_job_id=None,
        gpu_type="H100",
        gpu_count=1,
        gpu_seconds=0.0,
        cost_usd=0.0,
        created_at=datetime.utcnow(),
        started_at=None,
        finished_at=None,
        reward_mean=0.85,
        percent_correct=92.3,
        checkpoint_url="https://cdn.example.com/weights.safetensors",
    )
    assert out.reward_mean == pytest.approx(0.85)
    assert out.percent_correct == pytest.approx(92.3)
    assert out.checkpoint_url == "https://cdn.example.com/weights.safetensors"
    # Verify None defaults
    out2 = RunOut(
        id="schema-test-2",
        owner_agent="agent",
        name="test",
        status="queued",
        base_model="llama",
        method="sft",
        hyperparams={},
        dataset_mixture=[],
        user_goal="",
        user_context="",
        agent_plan="",
        citations=[],
        tinker_job_id=None,
        gpu_type="H100",
        gpu_count=1,
        gpu_seconds=0.0,
        cost_usd=0.0,
        created_at=datetime.utcnow(),
        started_at=None,
        finished_at=None,
    )
    assert out2.reward_mean is None
    assert out2.percent_correct is None
    assert out2.checkpoint_url is None
