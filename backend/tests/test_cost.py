"""Tests for cost tracking and budget enforcement."""

from __future__ import annotations

from datetime import datetime, timedelta
from unittest.mock import AsyncMock

import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from app.models.budget import Budget
from app.models.run import Run, RunStatus
from app.services.cost import (
    check_budget,
    daily_spend,
    monthly_spend,
    project_total,
    rate_per_hour,
)


class TestRatePerHour:
    """Test GPU hourly rate calculation."""

    def test_h100_single_gpu(self):
        """H100 with 1 GPU at default rate."""
        # Assuming default is ~4.50/hr
        rate = rate_per_hour("H100", 1)
        assert rate > 0
        assert rate == pytest.approx(4.50, rel=0.1)

    def test_a100_single_gpu(self):
        """A100 with 1 GPU at default rate."""
        # Assuming default is ~2.20/hr
        rate = rate_per_hour("A100", 1)
        assert rate > 0
        assert rate == pytest.approx(2.20, rel=0.1)

    def test_multiple_gpus(self):
        """Hourly rate scales with GPU count."""
        rate_1 = rate_per_hour("H100", 1)
        rate_8 = rate_per_hour("H100", 8)
        assert rate_8 == pytest.approx(rate_1 * 8)

    def test_unknown_gpu_type_defaults_to_h100(self):
        """Unknown GPU type falls back to H100 rate."""
        unknown = rate_per_hour("UNKNOWN", 1)
        h100 = rate_per_hour("H100", 1)
        assert unknown == h100


class TestProjectionMath:
    """Test cost projection and ETA calculations."""

    def test_projection_not_started(self):
        """Projection returns current cost if run not started."""
        run = Run(
            id="test-1",
            owner_agent="test-agent",
            name="test",
            base_model="llama",
            method="sft",
            gpu_type="H100",
            gpu_count=1,
            cost_usd=0.0,
            started_at=None,
        )
        projected = project_total(run)
        assert projected == 0.0

    def test_projection_completed(self):
        """Projection returns current cost if run is done."""
        run = Run(
            id="test-2",
            owner_agent="test-agent",
            name="test",
            base_model="llama",
            method="sft",
            gpu_type="H100",
            gpu_count=1,
            cost_usd=100.0,
            started_at=datetime.utcnow() - timedelta(hours=1),
            status=RunStatus.succeeded,
        )
        projected = project_total(run)
        assert projected == 100.0

    def test_projection_running(self):
        """Projection extrapolates for running job."""
        now = datetime.utcnow()
        run = Run(
            id="test-3",
            owner_agent="test-agent",
            name="test",
            base_model="llama",
            method="sft",
            gpu_type="H100",
            gpu_count=1,
            cost_usd=10.0,
            started_at=now - timedelta(hours=1),
            status=RunStatus.running,
        )
        projected = project_total(run)
        # Should be conservative estimate of 10x elapsed
        assert projected >= 10.0


class TestBudgetEnforcement:
    """Test budget checking and monthly/daily spend queries."""

    @pytest.mark.asyncio
    async def test_check_budget_no_limit(self, session: AsyncSession):
        """No budget returns True (always within limit)."""
        is_within, info = await check_budget(session, "test-agent", 100.0)
        assert is_within is True
        assert info is None

    @pytest.mark.asyncio
    async def test_check_budget_within_limit(self, session: AsyncSession):
        """Cost within limit returns True."""
        # Create budget of $500/month
        budget = Budget(
            scope="agent",
            scope_id="test-agent",
            monthly_limit_usd=500.0,
        )
        session.add(budget)
        await session.commit()

        is_within, info = await check_budget(session, "test-agent", 100.0)
        assert is_within is True
        assert info is None

    @pytest.mark.asyncio
    async def test_check_budget_exceeds_limit(self, session: AsyncSession):
        """Cost exceeding limit returns False with details."""
        budget = Budget(
            scope="agent",
            scope_id="test-agent",
            monthly_limit_usd=200.0,
        )
        session.add(budget)
        await session.commit()

        # Try to add 150 when already at 0 (under 200)
        is_within, info = await check_budget(session, "test-agent", 150.0)
        assert is_within is True

        # But 250 should exceed
        is_within, info = await check_budget(session, "test-agent", 250.0)
        assert is_within is False
        assert info is not None
        assert info["budget"] == 200.0
        assert info["projected"] == 250.0


@pytest.fixture
async def session():
    """Provide a test async session."""
    # This would be connected to a test database
    # For now, return a mock
    return AsyncMock(spec=AsyncSession)
