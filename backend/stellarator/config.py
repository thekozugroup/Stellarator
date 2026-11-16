"""Runtime configuration, read from environment variables."""

from __future__ import annotations

import os
from pathlib import Path

DATA_DIR = Path(os.environ.get("STELLARATOR_DATA_DIR", "./data")).resolve()
DATA_DIR.mkdir(parents=True, exist_ok=True)

UPLOAD_DIR = DATA_DIR / "uploads"
UPLOAD_DIR.mkdir(parents=True, exist_ok=True)

DB_URL = os.environ.get("STELLARATOR_DB_URL", f"sqlite+aiosqlite:///{DATA_DIR / 'stellarator.db'}")

# Built frontend. In Docker the image copies `frontend/dist` here.
STATIC_DIR = Path(
    os.environ.get("STELLARATOR_STATIC_DIR", Path(__file__).resolve().parent / "static")
)

# Hugging Face token from the environment wins over the one saved in settings.
ENV_HF_TOKEN = os.environ.get("HF_TOKEN") or os.environ.get("HUGGING_FACE_HUB_TOKEN")

HF_DATASETS_SERVER = os.environ.get(
    "STELLARATOR_HF_DATASETS_SERVER", "https://datasets-server.huggingface.co"
)
