"""Cost tracking and budget management endpoints."""

from __future__ import annotations

import logging
from typing import Annotated, Optional

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import select, func
from sqlalchemy.ext.asyncio import AsyncSession

from app.core.auth import CurrentAgent
from app.core.db import get_session
from app.models.budget import Budget
from app.models.run import Run, RunStatus
from app.services import cost

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/v1/cost", tags=["cost"])


class CostSummary:
    """Cost summary response."""

    def __init__(self, today: float, this_month: float, by_agent: dict, top_runs: list):
        self.today = today
        self.this_month = this_month
        self.by_agent = by_agent
        self.top_runs = top_runs

    def dict(self):
        return {
            "today": self.today,
            "this_month": self.this_month,
            "by_agent": self.by_agent,
            "top_runs": self.top_runs,
        }


class ProjectionResponse:
    """Run cost projection response."""

    def __init__(self, current: float, burn_per_hour: float, projected_total: float, eta: Optional[str]):
        self.current = current
        self.burn_per_hour = burn_per_hour
        self.projected_total = projected_total
        self.eta = eta

    def dict(self):
        return {
            "current": self.current,
            "burn_per_hour": self.burn_per_hour,
            "projected_total": self.projected_total,
            "eta": self.eta,
        }


class BudgetResponse:
    """Budget response."""

    def __init__(self, id: int, scope: str, scope_id: str, monthly_limit: Optional[float], daily_limit: Optional[float], alert_threshold: float):
        self.id = id
        self.scope = scope
        self.scope_id = scope_id
        self.monthly_limit_usd = monthly_limit
        self.daily_limit_usd = daily_limit
        self.alert_threshold_pct = alert_threshold

    def dict(self):
        return {
            "id": self.id,
            "scope": self.scope,
            "scope_id": self.scope_id,
            "monthly_limit_usd": self.monthly_limit_usd,
            "daily_limit_usd": self.daily_limit_usd,
            "alert_threshold_pct": self.alert_threshold_pct,
        }


@router.get("/summary")
async def get_cost_summary(
    _agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
):
    """Get cost summary: today, this month, by agent, top runs."""
    today = await cost.daily_spend(session)
    this_month = await cost.monthly_spend(session)

    # Aggregate by agent
    stmt = select(Run.owner_agent, func.sum(Run.cost_usd).label("total")).group_by(Run.owner_agent)
    result = await session.execute(stmt)
    by_agent = {row[0]: round(float(row[1]), 4) for row in result.all()}

    # Top 10 most expensive runs
    stmt = select(Run).order_by(Run.cost_usd.desc()).limit(10)
    result = await session.execute(stmt)
    top_runs = [
        {
            "id": r.id,
            "name": r.name,
            "owner_agent": r.owner_agent,
            "cost_usd": r.cost_usd,
            "status": r.status,
        }
        for r in result.scalars().all()
    ]

    return CostSummary(
        today=round(today, 4),
        this_month=round(this_month, 4),
        by_agent=by_agent,
        top_runs=top_runs,
    ).dict()


@router.get("/runs/{run_id}/projection")
async def get_run_projection(
    run_id: str,
    _agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
):
    """Get projected total cost and ETA for a run."""
    stmt = select(Run).where(Run.id == run_id)
    result = await session.execute(stmt)
    run = result.scalar_one_or_none()

    if not run:
        raise HTTPException(status.HTTP_404_NOT_FOUND, f"Run '{run_id}' not found")

    if run.owner_agent != _agent:
        raise HTTPException(status.HTTP_403_FORBIDDEN, "Not authorized to view this run")

    burn_per_hour = cost.rate_per_hour(run.gpu_type, run.gpu_count)
    projected = cost.project_total(run)

    eta = None
    if run.started_at and run.status == RunStatus.running:
        from datetime import datetime, timedelta

        if run.cost_usd > 0:
            elapsed_seconds = (datetime.utcnow() - run.started_at).total_seconds()
            burn_per_second = run.cost_usd / elapsed_seconds if elapsed_seconds > 0 else 0
            remaining_cost = projected - run.cost_usd
            if burn_per_second > 0:
                remaining_seconds = remaining_cost / burn_per_second
                eta_time = datetime.utcnow() + timedelta(seconds=remaining_seconds)
                eta = eta_time.isoformat()

    return ProjectionResponse(
        current=round(run.cost_usd, 4),
        burn_per_hour=round(burn_per_hour, 4),
        projected_total=round(projected, 4),
        eta=eta,
    ).dict()


@router.post("/budgets")
async def create_budget(
    scope: str,
    scope_id: str,
    monthly_limit_usd: Optional[float] = None,
    daily_limit_usd: Optional[float] = None,
    alert_threshold_pct: float = 80.0,
    _agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
):
    """Create or update a budget."""
    # Auth: agent can only set budgets for themselves unless scope=run
    if scope == "agent" and scope_id != _agent:
        raise HTTPException(status.HTTP_403_FORBIDDEN, "Can only set budgets for your own agent")

    if scope == "run":
        # Verify agent owns the run
        stmt = select(Run).where(Run.id == scope_id)
        result = await session.execute(stmt)
        run = result.scalar_one_or_none()
        if not run or run.owner_agent != _agent:
            raise HTTPException(status.HTTP_403_FORBIDDEN, "Can only set budgets for your own runs")

    # Check for existing budget
    stmt = select(Budget).where(
        (Budget.scope == scope) & (Budget.scope_id == scope_id)
    )
    result = await session.execute(stmt)
    budget = result.scalar_one_or_none()

    if budget:
        budget.monthly_limit_usd = monthly_limit_usd
        budget.daily_limit_usd = daily_limit_usd
        budget.alert_threshold_pct = alert_threshold_pct
    else:
        budget = Budget(
            scope=scope,
            scope_id=scope_id,
            monthly_limit_usd=monthly_limit_usd,
            daily_limit_usd=daily_limit_usd,
            alert_threshold_pct=alert_threshold_pct,
        )
        session.add(budget)

    await session.commit()
    await session.refresh(budget)

    return BudgetResponse(
        id=budget.id,
        scope=budget.scope,
        scope_id=budget.scope_id,
        monthly_limit=budget.monthly_limit_usd,
        daily_limit=budget.daily_limit_usd,
        alert_threshold=budget.alert_threshold_pct,
    ).dict()


@router.get("/budgets")
async def list_budgets(
    _agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
):
    """List budgets for agent (agent scope) or agent's runs."""
    stmt = select(Budget).where(
        (Budget.scope == "agent") & (Budget.scope_id == _agent)
    )
    result = await session.execute(stmt)
    budgets = result.scalars().all()

    # Also include run budgets for runs owned by agent
    run_ids_stmt = select(Run.id).where(Run.owner_agent == _agent)
    run_ids_result = await session.execute(run_ids_stmt)
    run_ids = [r[0] for r in run_ids_result.all()]

    if run_ids:
        run_budgets_stmt = select(Budget).where(
            (Budget.scope == "run") & (Budget.scope_id.in_(run_ids))
        )
        run_budgets_result = await session.execute(run_budgets_stmt)
        budgets.extend(run_budgets_result.scalars().all())

    return [
        BudgetResponse(
            id=b.id,
            scope=b.scope,
            scope_id=b.scope_id,
            monthly_limit=b.monthly_limit_usd,
            daily_limit=b.daily_limit_usd,
            alert_threshold=b.alert_threshold_pct,
        ).dict()
        for b in budgets
    ]


@router.delete("/budgets/{budget_id}")
async def delete_budget(
    budget_id: int,
    _agent: str = CurrentAgent,
    session: AsyncSession = Depends(get_session),
):
    """Delete a budget (auth: owner only)."""
    stmt = select(Budget).where(Budget.id == budget_id)
    result = await session.execute(stmt)
    budget = result.scalar_one_or_none()

    if not budget:
        raise HTTPException(status.HTTP_404_NOT_FOUND, "Budget not found")

    # Auth check
    if budget.scope == "agent" and budget.scope_id != _agent:
        raise HTTPException(status.HTTP_403_FORBIDDEN, "Not authorized to delete this budget")

    if budget.scope == "run":
        stmt = select(Run).where(Run.id == budget.scope_id)
        result = await session.execute(stmt)
        run = result.scalar_one_or_none()
        if not run or run.owner_agent != _agent:
            raise HTTPException(status.HTTP_403_FORBIDDEN, "Not authorized to delete this budget")

    await session.delete(budget)
    await session.commit()
    return {"status": "deleted"}
