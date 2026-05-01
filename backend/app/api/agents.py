"""Agent identity endpoints."""

from __future__ import annotations

from fastapi import APIRouter

from app.core.auth import CurrentAgent

from .schemas import WhoAmIOut

router = APIRouter(tags=["agents"])


@router.get("/whoami", response_model=WhoAmIOut)
async def whoami(agent: str = CurrentAgent) -> WhoAmIOut:
    return WhoAmIOut(agent=agent)
