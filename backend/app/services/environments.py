"""Environment registry — well-known RL/eval task presets.

The catalog is stored in environments_catalog.json alongside this module.
Each entry contains the recommended training method, dataset mixture,
hyperparameters, and evaluation metric for a known benchmark.

Public API
----------
list_environments() -> list[dict]
    Return all catalog entries.

pick_environment(env_id: str) -> dict | None
    Return the entry for *env_id*, or None if not found.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

_CATALOG_PATH = Path(__file__).with_name("environments_catalog.json")

# Loaded once at import time; the file is small and static.
def _load_catalog() -> list[dict[str, Any]]:
    with _CATALOG_PATH.open("r", encoding="utf-8") as fh:
        data = json.load(fh)
    if not isinstance(data, list):
        raise ValueError("environments_catalog.json must be a JSON array")
    return data


_CATALOG: list[dict[str, Any]] = _load_catalog()
_INDEX: dict[str, dict[str, Any]] = {entry["id"]: entry for entry in _CATALOG}


def list_environments() -> list[dict[str, Any]]:
    """Return all environment catalog entries."""
    return list(_CATALOG)


def pick_environment(env_id: str) -> dict[str, Any] | None:
    """Return the catalog entry for *env_id*, or None if unknown."""
    return _INDEX.get(env_id)
