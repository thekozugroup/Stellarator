from __future__ import annotations

from fastapi import APIRouter, Depends
from sqlalchemy.ext.asyncio import AsyncSession

from .. import __version__
from ..config import DATA_DIR, ENV_HF_TOKEN
from ..db import get_session
from ..schemas import SettingsIn, SettingsOut
from ..services.settings import get_setting, key_hint, set_setting

router = APIRouter(prefix="/api/settings", tags=["settings"])


async def _out(session: AsyncSession) -> SettingsOut:
    saved = await get_setting(session, "hf_token")
    token = ENV_HF_TOKEN or saved
    return SettingsOut(
        hf_token_set=bool(token),
        hf_token_hint=key_hint(token),
        hf_token_from_env=bool(ENV_HF_TOKEN),
        data_dir=str(DATA_DIR),
        version=__version__,
    )


@router.get("", response_model=SettingsOut)
async def read_settings(session: AsyncSession = Depends(get_session)):
    return await _out(session)


@router.put("", response_model=SettingsOut)
async def write_settings(body: SettingsIn, session: AsyncSession = Depends(get_session)):
    if body.hf_token is not None:
        await set_setting(session, "hf_token", body.hf_token.strip() or None)
    return await _out(session)
