"""Cost tracking and budget management.

TODO: Integrate with runs.py create_run() endpoint by adding this one-liner before tinker.create_job():
    is_within_budget, budget_info = await check_budget(session, agent, estimated_cost)
    if not is_within_budget:
        raise HTTPException(status.HTTP_402_PAYMENT_REQUIRED, detail={...budget_info...})
"""

from __future__ import annotations

from datetime import datetime, timedelta
from typing import Optional

from sqlalchemy import and_, func, select
from sqlalchemy.ext.asyncio import AsyncSession

from app.core.config import settings
from app.models.budget import Budget
from app.models.run import Run, RunStatus


def projected_total_for(payload) -> float:
    """Estimate USD cost for a not-yet-created run from its create payload.

    Uses GPU rate * (max_steps * estimated_seconds_per_step / 3600).
    Defaults: max_steps=0 -> 0, estimated_seconds_per_step=1.0.
    Accepts either a Pydantic model or a plain dict-like object.
    """
    def _g(obj, key, default=None):
        if isinstance(obj, dict):
            return obj.get(key, default)
        return getattr(obj, key, default)

    gpu_type = _g(payload, "gpu_type", "H100") or "H100"
    gpu_count = int(_g(payload, "gpu_count", 1) or 1)
    hyperparams = _g(payload, "hyperparams", {}) or {}
    max_steps = float(hyperparams.get("max_steps", 0) or 0)
    sec_per_step = float(hyperparams.get("estimated_seconds_per_step", 1.0) or 1.0)

    hours = (max_steps * sec_per_step) / 3600.0
    return rate_per_hour(gpu_type, gpu_count) * hours


def rate_per_hour(gpu_type: str, gpu_count: int) -> float:
    """Calculate hourly GPU cost based on type and count."""
    gpu_type_upper = gpu_type.upper()
    if gpu_type_upper == "H100":
        hourly_rate = settings.cost_h100_usd_per_hr
    elif gpu_type_upper == "A100":
        hourly_rate = settings.cost_a100_usd_per_hr
    else:
        # Fallback to H100 for unknown types
        hourly_rate = settings.cost_h100_usd_per_hr

    return hourly_rate * gpu_count


def project_total(run: Run) -> float:
    """Extrapolate projected total cost from current burn rate.

    Uses started_at, last metric timestamp, and hyperparams.max_steps if available.
    Returns current cost if run is not started yet.
    """
    if run.started_at is None:
        return run.cost_usd

    if run.status in (RunStatus.succeeded, RunStatus.failed, RunStatus.cancelled):
        return run.cost_usd

    # Calculate burn rate from current progress
    now = datetime.utcnow()
    elapsed = (now - run.started_at).total_seconds()

    if elapsed <= 0:
        return run.cost_usd

    burn_per_second = run.cost_usd / elapsed if elapsed > 0 else 0

    # Try to estimate remaining time from hyperparams.max_steps
    max_steps = run.hyperparams.get("max_steps") if run.hyperparams else None
    remaining_seconds = None

    if max_steps and run.metrics:
        last_metric = max(run.metrics, key=lambda m: m.created_at)
        current_step = next(
            (m.value for m in run.metrics if m.name == "step"),
            0
        )
        if current_step > 0 and max_steps > current_step:
            steps_per_second = current_step / elapsed
            remaining_steps = max_steps - current_step
            remaining_seconds = remaining_steps / steps_per_second if steps_per_second > 0 else None

    if remaining_seconds:
        projected_additional = burn_per_second * remaining_seconds
        return run.cost_usd + projected_additional

    # Conservative estimate: assume 10x current elapsed time
    return run.cost_usd + (burn_per_second * elapsed * 10)


async def daily_spend(session: AsyncSession, agent: Optional[str] = None) -> float:
    """Calculate total spend today across runs."""
    today_start = datetime.utcnow().replace(hour=0, minute=0, second=0, microsecond=0)

    query = select(func.coalesce(func.sum(Run.cost_usd), 0.0)).where(
        Run.created_at >= today_start
    )

    if agent:
        query = query.where(Run.owner_agent == agent)

    result = await session.execute(query)
    return float(result.scalar() or 0.0)


async def monthly_spend(session: AsyncSession, agent: Optional[str] = None) -> float:
    """Calculate total spend this month across runs."""
    now = datetime.utcnow()
    month_start = now.replace(day=1, hour=0, minute=0, second=0, microsecond=0)

    query = select(func.coalesce(func.sum(Run.cost_usd), 0.0)).where(
        Run.created_at >= month_start
    )

    if agent:
        query = query.where(Run.owner_agent == agent)

    result = await session.execute(query)
    return float(result.scalar() or 0.0)


async def check_budget(
    session: AsyncSession,
    agent: str,
    projected_cost: float,
) -> tuple[bool, Optional[dict]]:
    """Check if projected cost exceeds agent's budget.

    Returns (is_within_budget, budget_info).
    If over budget, budget_info contains {budget, current_spend, projected, monthly_limit}.
    """
    # Get agent budget (or None if not set)
    stmt = select(Budget).where(
        and_(
            Budget.scope == "agent",
            Budget.scope_id == agent,
        )
    )
    result = await session.execute(stmt)
    budget = result.scalar_one_or_none()

    if not budget or not budget.monthly_limit_usd:
        return True, None

    current = await monthly_spend(session, agent)
    projected = current + projected_cost

    if projected > budget.monthly_limit_usd:
        return False, {
            "budget": budget.monthly_limit_usd,
            "current_spend": current,
            "projected": projected,
            "run_cost": projected_cost,
        }

    return True, None
