"""Pre-flight gate for production-scale runs.

Pattern lifted from Hugging Face's ML Intern: before requesting expensive
GPUs, the agent must declare a structured plan including:
  * model + method
  * dataset mixture (with weights)
  * hyperparameters
  * the sandbox (smoke-test) run that justifies these choices
  * projected cost
  * citations to the recipes used

The server validates the schema's presence and the sandbox lineage's
freshness; it does NOT vet the recipe's correctness — that is the agent's
job. The gate exists to short-circuit doom-loops where the agent skips the
research/sandbox steps and goes straight to a 50-GPU run.
"""

from __future__ import annotations

from datetime import datetime, timedelta
from typing import Any

from pydantic import BaseModel, Field, ValidationError
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.models.run import Run, RunStatus

# Sandbox lineage must be no older than this. 24h matches HF's pattern of
# requiring fresh evidence for the proposed scale-up.
SANDBOX_MAX_AGE = timedelta(hours=24)

# GPU profile that does NOT require preflight. Anything else is "scale".
SANDBOX_GPU_TYPES = frozenset({"cpu"})
SANDBOX_MAX_GPU_COUNT = 1


class PreflightCitation(BaseModel):
    source: str = Field(min_length=1)  # 'hf' | 'arxiv' | 'github' | 'web'
    id: str = Field(min_length=1)
    title: str = Field(min_length=1)
    note: str = ""


class PreflightDataset(BaseModel):
    name: str = Field(min_length=1)
    weight: float = Field(gt=0)
    source: str = Field(min_length=1)


class Preflight(BaseModel):
    model: str = Field(min_length=1)
    method: str = Field(min_length=1)
    dataset_mixture: list[PreflightDataset] = Field(min_length=1)
    hyperparams: dict[str, Any] = Field(default_factory=dict)
    sandbox_run_id: str = Field(min_length=1)
    sandbox_summary: str = Field(min_length=1)
    projected_cost_usd: float = Field(ge=0)
    citations: list[PreflightCitation] = Field(default_factory=list)


class PreflightError(Exception):
    """Raised when preflight is missing or fails validation."""

    def __init__(self, code: str, message: str, hint: str | None = None) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.hint = hint

    def to_dict(self) -> dict[str, Any]:
        out: dict[str, Any] = {"error": self.code, "message": self.message}
        if self.hint:
            out["hint"] = self.hint
        return out


def is_scale_request(gpu_type: str, gpu_count: int) -> bool:
    """Return True iff the request needs a preflight gate.

    A request counts as "sandbox tier" (no preflight) when:
      * gpu_type is CPU, OR
      * gpu_count <= 1 on any single GPU (treated as smoke-test capacity).

    Multi-GPU requests on any accelerator class trigger the gate.
    """
    if gpu_type.lower() in SANDBOX_GPU_TYPES:
        return False
    if gpu_count <= SANDBOX_MAX_GPU_COUNT:
        return False
    return True


def parse_preflight(raw: dict[str, Any] | None) -> Preflight:
    """Validate the preflight JSON. Raises PreflightError on failure."""
    if not raw:
        raise PreflightError(
            "preflight_missing",
            "Scale runs require preflight_json. Sandbox first, then submit_preflight().",
            hint="Call sandbox_create() then submit_preflight() before run_create().",
        )
    try:
        return Preflight.model_validate(raw)
    except ValidationError as exc:
        raise PreflightError(
            "preflight_invalid",
            "preflight_json failed schema validation",
            hint=str(exc.errors()[:5]),
        ) from exc


async def validate_sandbox_lineage(
    preflight: Preflight, agent: str, session: AsyncSession
) -> Run:
    """Ensure the cited sandbox_run_id is owned, recent, and completed.

    Returns the sandbox Run row on success.
    """
    row = await session.get(Run, preflight.sandbox_run_id)
    if row is None:
        raise PreflightError(
            "sandbox_not_found",
            f"sandbox_run_id '{preflight.sandbox_run_id}' does not exist",
        )
    if row.owner_agent != agent:
        raise PreflightError(
            "sandbox_owner_mismatch",
            "sandbox_run_id must belong to the requesting agent",
        )
    if not row.is_sandbox:
        raise PreflightError(
            "sandbox_not_sandbox",
            "Cited run is not a sandbox run; rerun with is_sandbox=True at small scale.",
        )
    if row.status not in (RunStatus.succeeded.value, RunStatus.running.value):
        raise PreflightError(
            "sandbox_not_completed",
            f"Sandbox run status is '{row.status}', need 'succeeded' or 'running'.",
            hint="Wait for the sandbox to finish before scale-up.",
        )

    age_anchor = row.finished_at or row.started_at or row.created_at
    if age_anchor and datetime.utcnow() - age_anchor > SANDBOX_MAX_AGE:
        raise PreflightError(
            "sandbox_stale",
            f"Sandbox lineage older than {SANDBOX_MAX_AGE}; rerun a fresh sandbox.",
            hint="Re-run sandbox_create() then re-submit_preflight().",
        )
    return row
