"""Tests: PerAgentSemaphore — per-agent and global cap enforcement."""

from __future__ import annotations

import asyncio

import pytest

from app.core.semaphores import PerAgentSemaphore


@pytest.mark.asyncio
async def test_per_agent_cap_enforced() -> None:
    """An agent that fills its per-agent cap blocks further acquires from that agent."""
    sem = PerAgentSemaphore(per_agent=2, global_=10)

    await sem.acquire("agent-a")
    await sem.acquire("agent-a")

    # Third acquire for agent-a should time out (cap=2 already full).
    with pytest.raises(asyncio.TimeoutError):
        await asyncio.wait_for(sem.acquire("agent-a"), timeout=0.05)

    # agent-b is unaffected.
    await sem.acquire("agent-b")
    sem.release("agent-b")

    sem.release("agent-a")
    sem.release("agent-a")


@pytest.mark.asyncio
async def test_one_agent_does_not_block_other() -> None:
    """Saturating agent-a does not prevent agent-b from acquiring."""
    sem = PerAgentSemaphore(per_agent=2, global_=10)

    await sem.acquire("agent-a")
    await sem.acquire("agent-a")

    # agent-b acquires fine.
    await asyncio.wait_for(sem.acquire("agent-b"), timeout=0.1)
    sem.release("agent-b")

    sem.release("agent-a")
    sem.release("agent-a")


@pytest.mark.asyncio
async def test_global_cap_enforced() -> None:
    """When the global cap is reached, even a different agent cannot acquire."""
    sem = PerAgentSemaphore(per_agent=10, global_=3)

    await sem.acquire("agent-a")
    await sem.acquire("agent-b")
    await sem.acquire("agent-c")

    # Global full — agent-d cannot acquire.
    with pytest.raises(asyncio.TimeoutError):
        await asyncio.wait_for(sem.acquire("agent-d"), timeout=0.05)

    sem.release("agent-a")
    sem.release("agent-b")
    sem.release("agent-c")


@pytest.mark.asyncio
async def test_release_restores_slot() -> None:
    """Releasing a slot allows the next acquire to proceed."""
    sem = PerAgentSemaphore(per_agent=1, global_=10)

    await sem.acquire("agent-a")
    sem.release("agent-a")

    # After release, a new acquire succeeds.
    await asyncio.wait_for(sem.acquire("agent-a"), timeout=0.1)
    sem.release("agent-a")


@pytest.mark.asyncio
async def test_global_slot_released_if_per_agent_acquire_fails() -> None:
    """If the per-agent semaphore blocks and we time out, the global slot is freed."""
    sem = PerAgentSemaphore(per_agent=1, global_=2)

    # Fill the per-agent cap for agent-a.
    await sem.acquire("agent-a")

    # Attempt (and time out) a second acquire for agent-a; global slot must be returned.
    with pytest.raises(asyncio.TimeoutError):
        await asyncio.wait_for(sem.acquire("agent-a"), timeout=0.05)

    # Global now has 1 free slot — agent-b can acquire.
    await asyncio.wait_for(sem.acquire("agent-b"), timeout=0.1)
    sem.release("agent-b")
    sem.release("agent-a")


@pytest.mark.asyncio
async def test_ws_connection_slots_singleton_exists() -> None:
    from app.core.semaphores import ws_connection_slots, PerAgentSemaphore
    assert isinstance(ws_connection_slots, PerAgentSemaphore)
