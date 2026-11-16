"""Hugging Face datasets: browse splits and preview rows through the datasets-server API, and read
rows from the auto-converted Parquet export with range reads."""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any
from urllib.parse import unquote

import httpx

from .. import config
from ..schemas import HFSplit
from .importers import ImportProblem, MemoryTable, ParquetTable
from .settings import hf_token

try:  # imported lazily by tests that replace it
    from huggingface_hub import HfFileSystem
except Exception:  # pragma: no cover
    HfFileSystem = None  # type: ignore[assignment,misc]

TIMEOUT = 30.0
_NAME_RE = re.compile(r"^[\w.\-]+(/[\w.\-]+)?$")
_NOT_NAME_SEGMENTS = {"viewer", "tree", "blob", "resolve", "discussions", "commits", "settings", "raw"}
_URL_RE = re.compile(r"^https?://(?:www\.)?huggingface\.co/datasets/(?P<rest>[^?#]+)", re.IGNORECASE)
_PARQUET_URL_RE = re.compile(
    r"^https?://[^/]+/datasets/(?P<repo>.+?)/resolve/(?P<rev>[^/]+)/(?P<rest>.+)$"
)


def normalize_dataset(raw: str) -> str:
    """'owner/name' from a bare name or a huggingface.co/datasets/... URL."""
    s = (raw or "").strip()
    m = _URL_RE.match(s) or re.match(r"^(?:www\.)?huggingface\.co/datasets/(?P<rest>[^?#]+)", s, re.IGNORECASE)
    if m:
        parts = [p for p in m.group("rest").split("/") if p]
        if len(parts) >= 2 and parts[1].lower() not in _NOT_NAME_SEGMENTS:
            s = "/".join(parts[:2])
        elif parts:
            s = parts[0]
    s = s.strip("/")
    if not _NAME_RE.match(s):
        raise ImportProblem(
            "That doesn't look like a dataset name. Use owner/name or a huggingface.co/datasets link."
        )
    return s


async def _headers() -> dict[str, str]:
    token = await hf_token()
    return {"Authorization": f"Bearer {token}"} if token else {}


def _upstream_message(resp: httpx.Response) -> str:
    try:
        body = resp.json()
        msg = body.get("error") if isinstance(body, dict) else None
    except ValueError:
        msg = None
    msg = (msg or "").strip()
    return msg[:200] if msg else f"status {resp.status_code}"


async def _get(path: str, params: dict[str, str], *, dataset: str, exists: bool, what: str) -> dict[str, Any]:
    """GET from the datasets-server. `exists` says the dataset is known to exist, so a 404 means
    this endpoint has nothing for it rather than that the name is wrong."""
    url = config.HF_DATASETS_SERVER.rstrip("/") + path
    headers = await _headers()
    try:
        async with httpx.AsyncClient(timeout=TIMEOUT, headers=headers) as client:
            resp = await client.get(url, params=params)
    except httpx.TimeoutException as e:
        raise ImportProblem("Hugging Face took too long to answer. Try again in a moment.", 504) from e
    except httpx.HTTPError as e:
        raise ImportProblem(
            "Couldn't reach Hugging Face. Check this machine's internet connection.", 502
        ) from e
    if resp.status_code == 200:
        try:
            data = resp.json()
        except ValueError as e:
            raise ImportProblem("Hugging Face sent a reply that couldn't be read. Try again.", 502) from e
        return data if isinstance(data, dict) else {}
    code = resp.status_code
    if code in (401, 403):
        raise ImportProblem("This dataset needs a Hugging Face token with access. Add one in Settings.", 403)
    if code == 404 and not exists:
        raise ImportProblem(f"No dataset called {dataset} on Hugging Face. Check the name.", 404)
    if code == 429:
        raise ImportProblem("Hugging Face is limiting requests right now. Wait a minute and try again.", 429)
    if code >= 500 and code != 501 and exists is False:
        raise ImportProblem(
            f"Hugging Face couldn't open {dataset} ({_upstream_message(resp)}). Try again shortly.", 502
        )
    raise ImportProblem(
        f"Hugging Face can't provide {what} for {dataset} yet ({_upstream_message(resp)}).", 422
    )


# --------------------------------------------------------------------------- splits and preview


async def fetch_splits(dataset: str) -> list[HFSplit]:
    data = await _get("/splits", {"dataset": dataset}, dataset=dataset, exists=False, what="its splits")
    out = [
        HFSplit(config=s["config"], split=s["split"], num_rows=None)
        for s in data.get("splits", [])
        if isinstance(s, dict) and "config" in s and "split" in s
    ]
    if not out:
        raise ImportProblem(f"{dataset} has no splits that Hugging Face can read.", 422)
    try:
        sizes = await fetch_sizes(dataset)
    except ImportProblem:
        sizes = {}
    for s in out:
        s.num_rows = sizes.get((s.config, s.split))
    return out


async def fetch_sizes(dataset: str) -> dict[tuple[str, str], int]:
    data = await _get("/size", {"dataset": dataset}, dataset=dataset, exists=True, what="row counts")
    size = data.get("size") or {}
    out: dict[tuple[str, str], int] = {}
    for s in size.get("splits", []) or []:
        if isinstance(s, dict) and isinstance(s.get("num_rows"), int):
            out[(s["config"], s["split"])] = s["num_rows"]
    return out


def choose_split(
    dataset: str, splits: list[HFSplit], config_name: str | None, split: str | None
) -> tuple[str, str]:
    configs = list(dict.fromkeys(s.config for s in splits))
    if config_name is None:
        config_name = "default" if "default" in configs else configs[0]
    elif config_name not in configs:
        raise ImportProblem(
            f"{dataset} has no config called '{config_name}'. Configs are: {', '.join(configs[:10])}.", 422
        )
    names = [s.split for s in splits if s.config == config_name]
    if split is None:
        split = "train" if "train" in names else names[0]
    elif split not in names:
        raise ImportProblem(
            f"{dataset} ({config_name}) has no split called '{split}'. Splits are: {', '.join(names[:10])}.", 422
        )
    return config_name, split


@dataclass
class HFPreviewData:
    dataset: str
    config: str
    split: str
    splits: list[HFSplit]
    table: MemoryTable
    num_rows: int | None


_MEDIA_TYPES = {"Image", "Audio", "Video", "Pdf"}


async def fetch_preview(dataset: str, config_name: str | None, split: str | None) -> HFPreviewData:
    dataset = normalize_dataset(dataset)
    splits = await fetch_splits(dataset)
    config_name, split = choose_split(dataset, splits, config_name, split)
    data = await _get(
        "/first-rows",
        {"dataset": dataset, "config": config_name, "split": split},
        dataset=dataset,
        exists=True,
        what="a preview",
    )
    skip: set[str] = set()
    names: list[str] = []
    for f in data.get("features", []) or []:
        if not isinstance(f, dict) or "name" not in f:
            continue
        t = f.get("type")
        if isinstance(t, dict) and t.get("_type") in _MEDIA_TYPES:
            skip.add(f["name"])
        else:
            names.append(f["name"])
    rows = [
        {k: v for k, v in r["row"].items() if k not in skip}
        for r in data.get("rows", []) or []
        if isinstance(r, dict) and isinstance(r.get("row"), dict)
    ]
    num_rows = next((s.num_rows for s in splits if s.config == config_name and s.split == split), None)
    table = MemoryTable(rows, names or None, detected_format="huggingface", total_rows=num_rows)
    return HFPreviewData(dataset, config_name, split, splits, table, num_rows)


# --------------------------------------------------------------------------- parquet export


@dataclass
class ParquetRef:
    repo: str
    revision: str  # as it appears in the URL, e.g. "refs%2Fconvert%2Fparquet"
    rest: str  # e.g. "main/train/0000.parquet"
    size: int | None = None

    @property
    def path(self) -> str:
        return f"datasets/{self.repo}@{self.revision}/{unquote(self.rest) if '%' in self.rest else self.rest}"


def parse_parquet_url(url: str, size: int | None = None) -> ParquetRef | None:
    m = _PARQUET_URL_RE.match(url)
    if not m:
        return None
    return ParquetRef(m.group("repo"), m.group("rev"), m.group("rest"), size)


@dataclass
class ParquetExport:
    dataset: str
    config: str
    split: str
    refs: list[ParquetRef]
    partial: bool
    num_rows: int | None


async def fetch_parquet(dataset: str, config_name: str | None, split: str | None) -> ParquetExport:
    dataset = normalize_dataset(dataset)
    splits = await fetch_splits(dataset)
    config_name, split = choose_split(dataset, splits, config_name, split)
    data = await _get("/parquet", {"dataset": dataset}, dataset=dataset, exists=True, what="a Parquet export")
    files = [
        f
        for f in data.get("parquet_files", []) or []
        if isinstance(f, dict) and f.get("config") == config_name and f.get("split") == split
    ]
    if not files:
        pending = data.get("pending") or []
        if pending:
            raise ImportProblem(
                "Hugging Face is still preparing a Parquet copy of this dataset. Try again in a few minutes.", 409
            )
        raise ImportProblem(
            f"Hugging Face has no Parquet export for {dataset} ({config_name}, {split}), so it can't be read yet.",
            422,
        )
    files.sort(key=lambda f: str(f.get("filename") or f.get("url")))
    refs = []
    for f in files:
        ref = parse_parquet_url(str(f.get("url", "")), f.get("size"))
        if ref is None:
            raise ImportProblem("Hugging Face returned a Parquet file address that couldn't be understood.", 502)
        refs.append(ref)
    num_rows = next((s.num_rows for s in splits if s.config == config_name and s.split == split), None)
    return ParquetExport(dataset, config_name, split, refs, bool(data.get("partial")), num_rows)


def open_export(export: ParquetExport, token: str | None) -> ParquetTable:
    """A lazily read table over the export's shards. Blocking: call in a thread."""
    if HfFileSystem is None:  # pragma: no cover
        raise ImportProblem("huggingface_hub is not installed on the server.", 500)
    fs = HfFileSystem(token=token)
    openers = [(lambda p=ref.path: fs.open(p, "rb")) for ref in export.refs]
    try:
        return ParquetTable(
            openers, total_rows=export.num_rows, detected_format="huggingface", small=False
        )
    except ImportProblem:
        raise
    except Exception as e:
        text = str(e)
        if "401" in text or "403" in text or "Unauthorized" in text or "Forbidden" in text:
            raise ImportProblem("This dataset needs a Hugging Face token with access. Add one in Settings.", 403) from e
        raise ImportProblem(f"Couldn't read the Parquet files from Hugging Face: {text[:200]}", 502) from e
