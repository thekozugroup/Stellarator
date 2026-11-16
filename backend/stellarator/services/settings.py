"""Key-value settings stored in the database."""

from __future__ import annotations

from typing import Any

from sqlalchemy.ext.asyncio import AsyncSession

from ..config import ENV_HF_TOKEN
from ..db import SessionLocal
from ..models import Setting


async def get_setting(session: AsyncSession, key: str, default: Any = None) -> Any:
    row = await session.get(Setting, key)
    return default if row is None else row.value


async def set_setting(session: AsyncSession, key: str, value: Any) -> None:
    row = await session.get(Setting, key)
    if value is None:
        if row is not None:
            await session.delete(row)
    elif row is None:
        session.add(Setting(key=key, value=value))
    else:
        row.value = value
    await session.commit()


async def hf_token() -> str | None:
    """The Hugging Face token to use: HF_TOKEN from the environment, else the saved one."""
    if ENV_HF_TOKEN:
        return ENV_HF_TOKEN
    async with SessionLocal() as session:
        return await get_setting(session, "hf_token")


def key_hint(secret: str | None) -> str | None:
    if not secret:
        return None
    return "…" + secret[-4:] if len(secret) > 8 else "…"
