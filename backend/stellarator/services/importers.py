"""Turn files, pasted text and table rows into prompts.

A prompt is stored as a list of chat messages ending in a user message, with an optional system
message first. Other source columns are kept as scalar `meta`.

Two kinds of source:
- `TextDoc`: plain text or Markdown, split into prompts by a `SplitMode`.
- `Table`: rows with named columns (CSV, TSV, JSON, JSONL, Parquet, Hugging Face).

Everything here is synchronous; callers run it in a thread.
"""

from __future__ import annotations

import codecs
import contextlib
import csv
import itertools
import json
import math
import re
import shutil
import sys
import time
from collections.abc import Callable, Collection, Iterable, Iterator
from dataclasses import dataclass, field
from datetime import date, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any, BinaryIO

from ..schemas import ColumnInfo, ImportMapping

MAX_META_CHARS = 4000
PREVIEW_PROMPTS = 20
SAMPLE_ROWS = 200
FULL_PASS_MAX_BYTES = 50 * 1024 * 1024
UPLOAD_MAX_AGE_S = 24 * 3600

PROMPT_NAMES = ("prompt", "question", "instruction", "input", "query", "problem", "text", "content", "user")
MESSAGES_NAMES = ("messages", "conversations", "conversation", "chat")
SYSTEM_NAMES = ("system", "system_prompt")

try:  # CSV cells can be large
    csv.field_size_limit(1 << 30)
except OverflowError:  # pragma: no cover
    csv.field_size_limit(sys.maxsize // 2)


class ImportProblem(Exception):
    """Something the user can fix. `status` becomes the HTTP status."""

    def __init__(self, message: str, status: int = 422):
        super().__init__(message)
        self.message = message
        self.status = status


# --------------------------------------------------------------------------- uploads


def cleanup_old_uploads(root: Path, max_age_s: float = UPLOAD_MAX_AGE_S) -> int:
    """Delete upload folders not modified for `max_age_s`. Returns how many were removed."""
    removed = 0
    cutoff = time.time() - max_age_s
    try:
        children = list(root.iterdir())
    except OSError:
        return 0
    for child in children:
        try:
            if child.stat().st_mtime < cutoff:
                shutil.rmtree(child, ignore_errors=True) if child.is_dir() else child.unlink()
                removed += 1
        except OSError:
            continue
    return removed


def sanitize_filename(name: str | None) -> str:
    base = (name or "").replace("\\", "/").split("/")[-1]
    base = re.sub(r"[^\w.\- ]+", "_", base, flags=re.UNICODE).strip(" .")
    base = re.sub(r"\s+", " ", base)
    if len(base) > 120:
        stem, dot, ext = base.rpartition(".")
        base = (stem[:100] + dot + ext[:15]) if dot and len(ext) <= 15 else base[:120]
    return base or "upload"


# --------------------------------------------------------------------------- format detection

_EXT_FORMATS = {
    ".txt": "text",
    ".text": "text",
    ".md": "markdown",
    ".markdown": "markdown",
    ".csv": "csv",
    ".tsv": "tsv",
    ".json": "json",
    ".jsonl": "jsonl",
    ".ndjson": "jsonl",
    ".parquet": "parquet",
}


def detect_format(path: Path, filename: str | None = None) -> str:
    """Format from the extension, else by sniffing the content."""
    ext = Path(filename or path.name).suffix.lower()
    if ext in _EXT_FORMATS:
        return _EXT_FORMATS[ext]
    try:
        with open(path, "rb") as f:
            head = f.read(65536)
    except OSError:
        return "text"
    if head.startswith(b"PAR1"):
        return "parquet"
    text = head.decode("utf-8-sig", errors="replace")
    stripped = text.lstrip()
    if stripped.startswith("["):
        return "json"
    if stripped.startswith("{"):
        lines = [ln for ln in stripped.split("\n") if ln.strip()]
        if len(lines) >= 2:
            try:
                if isinstance(json.loads(lines[0]), dict):
                    return "jsonl"
            except ValueError:
                pass
        return "json"
    lines = [ln for ln in text.split("\n")[:-1] if ln.strip()] or [ln for ln in text.split("\n") if ln.strip()]
    if len(lines) >= 2:
        for delim, fmt in (("\t", "tsv"), (",", "csv"), (";", "csv"), ("|", "csv")):
            counts = {ln.count(delim) for ln in lines[:20]}
            if len(counts) == 1 and next(iter(counts)) >= 1:
                return fmt
    return "text"


def detect_encoding(path: Path) -> tuple[str, list[str]]:
    """'utf-8-sig' when the file is valid UTF-8 (BOM tolerated), else 'latin-1' with a warning."""
    dec = codecs.getincrementaldecoder("utf-8")()
    try:
        with open(path, "rb") as f:
            while chunk := f.read(1 << 20):
                dec.decode(chunk)
            dec.decode(b"", final=True)
        return "utf-8-sig", []
    except UnicodeDecodeError:
        return "latin-1", [
            "This file is not valid UTF-8, so it was read as Latin-1. Accented characters may look wrong."
        ]


# --------------------------------------------------------------------------- text splitting

_SEP_RE = re.compile(r"^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$")
_HEADING_RE = re.compile(r"^ {0,3}(#{1,6})[ \t]+(.+?)(?:[ \t]+#+)?[ \t]*$")
_ITEM_RE = re.compile(r"^(?:[-*+]|\d{1,9}[.)])[ \t]+(.*)$")
_FENCE_RE = re.compile(r"^ {0,3}(```|~~~)")
_FRONT_MATTER_RE = re.compile(r"\A---[ \t]*\n(.*?)\n(?:---|\.\.\.)[ \t]*(?:\n|\Z)", re.DOTALL)
_BLANK_SPLIT_RE = re.compile(r"\n[ \t]*\n")


def strip_front_matter(text: str) -> str:
    m = _FRONT_MATTER_RE.match(text)
    if m and re.search(r"^[\w.-]+[ \t]*:", m.group(1), re.MULTILINE):
        return text[m.end() :]
    return text


def _fenced_lines(text: str) -> list[tuple[str, bool]]:
    """(line, inside_code_fence). Structural markers inside fences are ignored."""
    out: list[tuple[str, bool]] = []
    fence: str | None = None
    for line in text.split("\n"):
        m = _FENCE_RE.match(line)
        if fence is None:
            if m:
                fence = m.group(1)
                out.append((line, True))
            else:
                out.append((line, False))
        else:
            out.append((line, True))
            if m and m.group(1) == fence:
                fence = None
    return out


def _clean(pieces: Iterable[str]) -> list[str]:
    out = []
    for p in pieces:
        p = p.strip()
        if p:
            out.append(p)
    return out


def _split_separator(lines: list[tuple[str, bool]]) -> list[str]:
    pieces: list[list[str]] = [[]]
    for line, code in lines:
        if not code and _SEP_RE.match(line):
            pieces.append([])
        else:
            pieces[-1].append(line)
    return _clean("\n".join(p) for p in pieces)


def _split_heading(lines: list[tuple[str, bool]]) -> list[str]:
    # (level, heading text or None for the preamble, body lines)
    sections: list[tuple[int, str | None, list[str]]] = [(0, None, [])]
    for line, code in lines:
        m = None if code else _HEADING_RE.match(line)
        if m:
            sections.append((len(m.group(1)), m.group(2).strip(), []))
        else:
            sections[-1][2].append(line)
    out = []
    for i, (level, title, body) in enumerate(sections):
        body_text = "\n".join(body).strip()
        if title is None:
            if body_text:
                out.append(body_text)
            continue
        nxt = sections[i + 1] if i + 1 < len(sections) else None
        if not body_text and nxt is not None and nxt[0] > level:
            continue  # a container heading with no text of its own
        out.append(f"{title}\n\n{body_text}" if body_text else title)
    return _clean(out)


def _split_list_items(lines: list[tuple[str, bool]]) -> list[str]:
    items: list[list[str]] = []
    for line, code in lines:
        m = None if code else _ITEM_RE.match(line)
        if m:
            items.append([m.group(1)])
        elif items:
            items[-1].append(line)
    out = []
    for item in items:
        first, rest = item[0], item[1:]
        rest_text = _dedent_continuation(rest)
        out.append(first + ("\n" + rest_text if rest_text.strip() else ""))
    return _clean(out)


def _dedent_continuation(lines: list[str]) -> str:
    indents = [len(ln) - len(ln.lstrip(" ")) for ln in lines if ln.strip()]
    cut = min(indents) if indents else 0
    return "\n".join(ln[cut:] if ln.strip() else "" for ln in lines).strip("\n")


def _auto_mode(text: str, lines: list[tuple[str, bool]], markdown: bool | None) -> str:
    plain = [ln for ln, code in lines if not code]
    if any(_SEP_RE.match(ln) for ln in plain):
        return "separator"
    if markdown is not False and sum(1 for ln in plain if _HEADING_RE.match(ln)) >= 2:
        return "heading"
    nonblank = [ln for ln in plain if ln.strip()]
    items = sum(1 for ln in nonblank if _ITEM_RE.match(ln))
    if items >= 2:
        other = sum(1 for ln in nonblank if not _ITEM_RE.match(ln) and not ln.startswith((" ", "\t")))
        if other <= max(1, int(0.1 * len(nonblank))):
            return "list_item"
    if len(_clean(_BLANK_SPLIT_RE.split(text))) >= 2:
        return "blank_line"
    all_lines = [ln for ln in text.split("\n") if ln.strip()]
    if len(all_lines) >= 2 and max(len(ln) for ln in all_lines) <= 500:
        return "line"
    return "whole"


def split_text(text: str, mode: str = "auto", markdown: bool | None = False) -> tuple[list[str], str]:
    """Split text into prompt strings. Returns (pieces, the mode actually used).

    markdown: True for Markdown files (front matter stripped, headings considered), False for plain
    text (headings only with an explicit "heading" mode), None for pasted text (headings considered).
    """
    text = text.lstrip("﻿").replace("\r\n", "\n").replace("\r", "\n")
    if markdown:
        text = strip_front_matter(text)
    lines = _fenced_lines(text)
    if mode == "auto":
        mode = _auto_mode(text, lines, markdown)
    if mode == "separator":
        return _split_separator(lines), mode
    if mode == "heading":
        return _split_heading(lines), mode
    if mode == "list_item":
        return _split_list_items(lines), mode
    if mode == "blank_line":
        return _clean(_BLANK_SPLIT_RE.split(text)), mode
    if mode == "line":
        return _clean(text.split("\n")), mode
    if mode == "whole":
        return _clean([text]), mode
    raise ImportProblem(f"Unknown split mode '{mode}'.")


# --------------------------------------------------------------------------- messages


_ROLE_MAP = {
    "system": "system",
    "developer": "system",
    "user": "user",
    "human": "user",
    "assistant": "assistant",
    "gpt": "assistant",
    "model": "assistant",
    "bot": "assistant",
    "chatgpt": "assistant",
}


def _content_text(content: Any) -> str:
    if content is None:
        return ""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = []
        for p in content:
            if isinstance(p, str):
                parts.append(p)
            elif isinstance(p, dict):
                t = p.get("text")
                if isinstance(t, str) and p.get("type") in (None, "text", "input_text", "output_text"):
                    parts.append(t)
        return "\n".join(parts)
    if isinstance(content, dict):
        t = content.get("text")
        return t if isinstance(t, str) else ""
    return str(content)


def is_messages_value(value: Any) -> bool:
    if not isinstance(value, list) or not value:
        return False
    for m in value[:3]:
        if not isinstance(m, dict):
            return False
        has_openai = m.get("role") is not None and "content" in m
        has_sharegpt = m.get("from") is not None and "value" in m
        if not (has_openai or has_sharegpt):
            return False
    return True


def normalize_messages(value: Any) -> list[dict[str, str]] | None:
    """OpenAI {role, content} or ShareGPT {from, value} -> messages ending in the last user turn."""
    if not isinstance(value, list):
        return None
    out: list[dict[str, str]] = []
    for m in value:
        if not isinstance(m, dict):
            continue
        if m.get("role") is not None:
            role, content = m.get("role"), m.get("content")
        elif m.get("from") is not None:
            role, content = m.get("from"), m.get("value")
        else:
            continue
        role = _ROLE_MAP.get(str(role).strip().lower())
        if role is None:
            continue
        out.append({"role": role, "content": _content_text(content).strip()})
    last_user = max((i for i, m in enumerate(out) if m["role"] == "user"), default=None)
    if last_user is None or not out[last_user]["content"]:
        return None
    return out[: last_user + 1]


# --------------------------------------------------------------------------- schema


@dataclass
class Schema:
    columns: list[ColumnInfo]
    scalar: set[str]  # columns whose values can go into meta

    def kind(self, name: str | None) -> str | None:
        for c in self.columns:
            if c.name == name:
                return c.kind
        return None

    def names(self) -> list[str]:
        return [c.name for c in self.columns]


_NUMBER_RE = re.compile(r"^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$")


def _shorten(s: str, n: int = 160) -> str:
    s = " ".join(s.split())
    return s if len(s) <= n else s[: n - 1] + "…"


def _example(value: Any) -> str:
    if isinstance(value, str):
        return _shorten(value)
    if is_messages_value(value):
        msgs = normalize_messages(value)
        if msgs:
            return _shorten(msgs[-1]["content"])
    return _shorten(json.dumps(value, ensure_ascii=False, default=str))


def infer_schema(names: Iterable[str], sample: list[dict[str, Any]], numeric_strings: bool = False) -> Schema:
    columns: list[ColumnInfo] = []
    scalar: set[str] = set()
    for name in names:
        vals = [r[name] for r in sample if r.get(name) is not None and r.get(name) != ""]
        if not vals:
            kind = "other"
        elif all(isinstance(v, str) for v in vals):
            kind = "number" if numeric_strings and all(_NUMBER_RE.match(v.strip()) for v in vals) else "text"
        elif all(isinstance(v, (int, float)) and not isinstance(v, bool) for v in vals):
            kind = "number"
        elif all(isinstance(v, (str, int, float)) and not isinstance(v, bool) for v in vals):
            kind = "text"
        elif all(is_messages_value(v) for v in vals):
            kind = "messages"
        else:
            kind = "other"
        if kind in ("text", "number") or (vals and all(isinstance(v, bool) for v in vals)):
            scalar.add(name)
        columns.append(ColumnInfo(name=name, kind=kind, example=_example(vals[0]) if vals else None))
    return Schema(columns, scalar)


def _avg_len(sample: list[dict[str, Any]], name: str) -> float:
    lens = [len(str(r[name])) for r in sample if r.get(name) not in (None, "")]
    return sum(lens) / len(lens) if lens else 0.0


def auto_prompt_field(schema: Schema, sample: list[dict[str, Any]]) -> str | None:
    by_lower = {c.name.lower(): c for c in schema.columns}
    for n in MESSAGES_NAMES:
        c = by_lower.get(n)
        if c and c.kind == "messages":
            return c.name
    for n in PROMPT_NAMES:
        c = by_lower.get(n)
        if c and c.kind == "text":
            return c.name
    for c in schema.columns:
        if c.kind == "messages":
            return c.name
    texts = [c.name for c in schema.columns if c.kind == "text"]
    if texts:
        return max(texts, key=lambda n: (_avg_len(sample, n), -texts.index(n)))
    return None


def auto_system_field(schema: Schema, prompt_field: str | None) -> str | None:
    for c in schema.columns:
        if c.name.lower() in SYSTEM_NAMES and c.kind == "text" and c.name != prompt_field:
            return c.name
    return None


# --------------------------------------------------------------------------- tables


def _window(it: Iterator[Any], offset: int, limit: int | None) -> Iterator[Any]:
    try:
        yield from itertools.islice(it, offset, None if limit is None else offset + limit)
    finally:
        close = getattr(it, "close", None)
        if close:
            close()


class Table:
    """Rows with named columns."""

    detected_format = ""
    total_rows: int | None = None
    small = True  # cheap enough to scan fully for a preview
    numeric_strings = False

    def __init__(self) -> None:
        self.notes: dict[str, str] = {}
        self.base_warnings: list[str] = []
        self._schema: Schema | None = None
        self._sample: list[dict[str, Any]] | None = None

    # subclasses
    def names(self) -> list[str]:
        raise NotImplementedError

    def rows(
        self, columns: Collection[str] | None = None, offset: int = 0, limit: int | None = None
    ) -> Iterator[dict[str, Any]]:
        raise NotImplementedError

    def close(self) -> None:
        pass

    # shared
    def sample(self) -> list[dict[str, Any]]:
        if self._sample is None:
            with contextlib.closing(self.rows(None, 0, SAMPLE_ROWS)) as it:
                self._sample = list(it)
        return self._sample

    @property
    def schema(self) -> Schema:
        if self._schema is None:
            self._schema = infer_schema(self.names(), self.sample(), self.numeric_strings)
        return self._schema

    @property
    def warnings(self) -> list[str]:
        return [*self.base_warnings, *self.notes.values()]


class MemoryTable(Table):
    def __init__(
        self,
        rows: list[dict[str, Any]],
        names: list[str] | None = None,
        detected_format: str = "json",
        total_rows: int | None = None,
    ):
        super().__init__()
        self._rows = rows
        if names is None:
            seen: dict[str, None] = {}
            for r in rows:
                for k in r:
                    seen.setdefault(k, None)
            names = list(seen)
        self._names = names
        self.detected_format = detected_format
        self.total_rows = len(rows) if total_rows is None else total_rows

    def names(self) -> list[str]:
        return self._names

    def rows(self, columns=None, offset=0, limit=None):
        return _window(iter(self._rows), offset, limit)


def _unique_headers(raw: list[str]) -> list[str]:
    out: list[str] = []
    seen: set[str] = set()
    for i, h in enumerate(raw):
        name = (h or "").strip().lstrip("﻿") or f"column_{i + 1}"
        base, n = name, 2
        while name in seen:
            name = f"{base}_{n}"
            n += 1
        seen.add(name)
        out.append(name)
    return out


class CsvTable(Table):
    numeric_strings = True

    def __init__(self, path: Path, fmt: str = "csv"):
        super().__init__()
        self.path = path
        self.detected_format = fmt
        self.small = path.stat().st_size <= FULL_PASS_MAX_BYTES
        self.encoding, enc_warn = detect_encoding(path)
        self.base_warnings.extend(enc_warn)
        self.delimiter = "\t" if fmt == "tsv" else self._sniff()
        self._names = self._read_header()

    def _sniff(self) -> str:
        with open(self.path, "r", encoding=self.encoding, newline="") as f:
            head = f.read(65536)
        cut = head.rfind("\n")
        if cut > 0:
            head = head[:cut]
        try:
            return csv.Sniffer().sniff(head, delimiters=",;\t|").delimiter
        except csv.Error:
            return ","

    def _open(self):
        return open(self.path, "r", encoding=self.encoding, newline="")

    def _read_header(self) -> list[str]:
        try:
            with self._open() as f:
                for row in csv.reader(f, delimiter=self.delimiter):
                    if any(c.strip() for c in row):
                        return _unique_headers(row)
        except csv.Error as e:
            raise ImportProblem(f"Couldn't read this file as {self.detected_format.upper()}: {e}.") from e
        raise ImportProblem("That file is empty.")

    def names(self) -> list[str]:
        return self._names

    def _iter(self) -> Iterator[dict[str, Any]]:
        names = self._names
        n = len(names)
        with self._open() as f:
            reader = csv.reader(f, delimiter=self.delimiter)
            header_seen = False
            try:
                for row in reader:
                    if not header_seen:
                        if any(c.strip() for c in row):
                            header_seen = True
                        continue
                    if not row or not any(c.strip() for c in row):
                        continue
                    if len(row) < n:
                        row = row + [""] * (n - len(row))
                    yield dict(zip(names, row))
            except csv.Error as e:
                raise ImportProblem(
                    f"Couldn't read this file as {self.detected_format.upper()} near line {reader.line_num}: {e}."
                ) from e

    def rows(self, columns=None, offset=0, limit=None):
        return _window(self._iter(), offset, limit)


class JsonlTable(Table):
    def __init__(self, path: Path):
        super().__init__()
        self.path = path
        self.detected_format = "jsonl"
        self.small = path.stat().st_size <= FULL_PASS_MAX_BYTES
        self.encoding, enc_warn = detect_encoding(path)
        self.base_warnings.extend(enc_warn)
        self._names: list[str] | None = None

    def names(self) -> list[str]:
        if self._names is None:
            seen: dict[str, None] = {}
            for r in self.sample():
                for k in r:
                    seen.setdefault(k, None)
            self._names = list(seen)
        return self._names

    def _iter(self) -> Iterator[dict[str, Any]]:
        bad = 0
        first_bad: int | None = None
        try:
            with open(self.path, "r", encoding=self.encoding, newline="") as f:
                for lineno, line in enumerate(f, 1):
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        obj = json.loads(line)
                    except ValueError:
                        obj = None
                        ok = False
                    else:
                        ok = True
                    if ok and isinstance(obj, str):
                        obj = {"prompt": obj}
                    if not ok or not isinstance(obj, dict):
                        bad += 1
                        if first_bad is None:
                            first_bad = lineno
                        continue
                    yield obj
        finally:
            if bad:
                self.notes["bad_lines"] = (
                    f"{bad:,} {'line was' if bad == 1 else 'lines were'} not valid JSON objects or strings "
                    f"and {'was' if bad == 1 else 'were'} skipped (first: line {first_bad})."
                )
            else:
                self.notes.pop("bad_lines", None)

    def rows(self, columns=None, offset=0, limit=None):
        return _window(self._iter(), offset, limit)


_JSON_LIST_KEYS = ("data", "prompts", "rows", "examples", "items", "train", "records", "questions")


def _json_rows(data: Any) -> list[dict[str, Any]]:
    if isinstance(data, dict):
        lists = {k: v for k, v in data.items() if isinstance(v, list)}
        if not lists:
            raise ImportProblem(
                "Couldn't find a list of prompts in this JSON. Use an array, or an object like {\"data\": [...]}."
            )
        key = next((k for k in _JSON_LIST_KEYS if k in lists), None)
        if key is None:
            key = max(lists, key=lambda k: len(lists[k]))
        data = lists[key]
    if not isinstance(data, list):
        raise ImportProblem("Couldn't find a list of prompts in this JSON.")
    rows: list[dict[str, Any]] = []
    for item in data:
        if isinstance(item, str):
            rows.append({"prompt": item})
        elif isinstance(item, dict):
            rows.append(item)
    if not rows:
        raise ImportProblem("This JSON has no strings or objects to use as prompts.")
    return rows


def open_json(path: Path) -> Table:
    encoding, warn = detect_encoding(path)
    try:
        with open(path, "r", encoding=encoding) as f:
            data = json.load(f)
    except ValueError as e:
        # Several JSON objects, one per line, is JSONL with the wrong extension.
        if "Extra data" in str(e):
            return JsonlTable(path)
        raise ImportProblem(f"Couldn't read this file as JSON: {e}.") from e
    table = MemoryTable(_json_rows(data), detected_format="json")
    table.base_warnings.extend(warn)
    return table


def _arrow_has_binary(t: Any) -> bool:
    import pyarrow as pa

    if pa.types.is_binary(t) or pa.types.is_large_binary(t) or pa.types.is_fixed_size_binary(t):
        return True
    if pa.types.is_struct(t):
        return any(_arrow_has_binary(t.field(i).type) for i in range(t.num_fields))
    if pa.types.is_list(t) or pa.types.is_large_list(t) or pa.types.is_fixed_size_list(t):
        return _arrow_has_binary(t.value_type)
    if pa.types.is_map(t):
        return _arrow_has_binary(t.key_type) or _arrow_has_binary(t.item_type)
    return False


class ParquetTable(Table):
    """One or more Parquet shards read lazily, by row group and batch.

    `openers` return binary file objects (local files, or Hugging Face range-read files).
    """

    def __init__(
        self,
        openers: list[Callable[[], BinaryIO]],
        total_rows: int | None = None,
        detected_format: str = "parquet",
        small: bool = True,
    ):
        super().__init__()
        import pyarrow.parquet as pq

        self.openers = openers
        self.detected_format = detected_format
        self.small = small
        with openers[0]() as f:
            pf = pq.ParquetFile(f)
            schema = pf.schema_arrow
            self._names = [fld.name for fld in schema if not _arrow_has_binary(fld.type)]
            first_rows = pf.metadata.num_rows
        if total_rows is None and len(openers) == 1:
            total_rows = first_rows
        self.total_rows = total_rows

    def names(self) -> list[str]:
        return self._names

    def rows(self, columns=None, offset=0, limit=None):
        return self._iter(columns, offset, limit)

    def _iter(self, columns, offset, limit) -> Iterator[dict[str, Any]]:
        import pyarrow.parquet as pq

        cols = self._names if columns is None else [n for n in self._names if n in set(columns)]
        skip = offset
        remaining = limit
        for opener in self.openers:
            if remaining is not None and remaining <= 0:
                return
            with opener() as f:
                pf = pq.ParquetFile(f)
                meta = pf.metadata
                if skip >= meta.num_rows:
                    skip -= meta.num_rows
                    continue
                groups: list[int] = []
                first_skip = 0
                for g in range(pf.num_row_groups):
                    n = meta.row_group(g).num_rows
                    if not groups and skip >= n:
                        skip -= n
                        continue
                    if not groups:
                        first_skip, skip = skip, 0
                    groups.append(g)
                batch_size = 1000 if remaining is None else max(1, min(1000, remaining + first_skip))
                batches = pf.iter_batches(batch_size=batch_size, row_groups=groups, columns=cols)
                try:
                    for batch in batches:
                        n = batch.num_rows
                        if first_skip:
                            if first_skip >= n:
                                first_skip -= n
                                continue
                            batch = batch.slice(first_skip)
                            first_skip = 0
                        if remaining is not None and batch.num_rows > remaining:
                            batch = batch.slice(0, remaining)
                        yield from batch.to_pylist()
                        if remaining is not None:
                            remaining -= batch.num_rows
                            if remaining <= 0:
                                return
                finally:
                    close = getattr(batches, "close", None)
                    if close:
                        close()


def open_parquet(path: Path) -> Table:
    try:
        return ParquetTable(
            [lambda: open(path, "rb")], small=path.stat().st_size <= FULL_PASS_MAX_BYTES
        )
    except Exception as e:  # pyarrow raises ArrowInvalid and friends
        raise ImportProblem(f"Couldn't read this file as Parquet: {e}") from e


# --------------------------------------------------------------------------- text documents


@dataclass
class TextDoc:
    text: str
    kind: str  # "text" | "markdown" | "paste"
    base_warnings: list[str] = field(default_factory=list)

    @property
    def detected_format(self) -> str:
        return self.kind

    @property
    def markdown(self) -> bool | None:
        return {"markdown": True, "text": False, "paste": None}[self.kind]

    def close(self) -> None:
        pass


def open_path(path: Path, fmt: str = "auto", filename: str | None = None) -> Table | TextDoc:
    if fmt == "auto":
        fmt = detect_format(path, filename)
    if fmt in ("text", "markdown"):
        enc, warn = detect_encoding(path)
        return TextDoc(path.read_text(encoding=enc), fmt, warn)
    try:
        if fmt in ("csv", "tsv"):
            return CsvTable(path, fmt)
        if fmt == "jsonl":
            return JsonlTable(path)
        if fmt == "json":
            return open_json(path)
        if fmt == "parquet":
            return open_parquet(path)
    except ImportProblem:
        raise
    except (OSError, ValueError) as e:
        raise ImportProblem(f"Couldn't read this file as {fmt}: {e}") from e
    raise ImportProblem(f"Unknown format '{fmt}'.")


def paste_doc(text: str) -> TextDoc:
    return TextDoc(text, "paste")


# --------------------------------------------------------------------------- prompts


@dataclass
class Stats:
    rows: int = 0
    empty: int = 0
    duplicates: int = 0

    def warnings(self) -> list[str]:
        out = []
        if self.empty:
            out.append(
                f"{self.empty:,} {'row' if self.empty == 1 else 'rows'} had an empty prompt and "
                f"{'was' if self.empty == 1 else 'were'} skipped."
            )
        if self.duplicates:
            out.append(
                f"{self.duplicates:,} duplicate {'prompt was' if self.duplicates == 1 else 'prompts were'} dropped."
            )
        return out


@dataclass
class Plan:
    prompt_field: str
    system_field: str | None
    input_field: str | None  # Alpaca: "instruction" + "input"
    meta_fields: list[str]

    def read_columns(self) -> set[str]:
        cols = {self.prompt_field, *self.meta_fields}
        if self.system_field:
            cols.add(self.system_field)
        if self.input_field:
            cols.add(self.input_field)
        return cols


def resolve_plan(schema: Schema, sample: list[dict[str, Any]], mapping: ImportMapping) -> tuple[Plan | None, str | None, str | None]:
    """(plan or None when no column fits, prompt_field, system_field). Raises on a bad explicit field."""
    names = schema.names()
    # system_field: None = detect automatically, "" = no system column.
    no_system = mapping.system_field == ""
    for label, f in (("prompt", mapping.prompt_field), ("system", mapping.system_field)):
        if f and f not in names:
            shown = ", ".join(names[:12]) + (", …" if len(names) > 12 else "")
            raise ImportProblem(f"There is no column called '{f}' to use as the {label}. Columns are: {shown}.")
    prompt_field = mapping.prompt_field or auto_prompt_field(schema, sample)
    system_field = None if no_system else (mapping.system_field or auto_system_field(schema, prompt_field))
    if system_field == prompt_field:
        system_field = None
    if prompt_field is None:
        return None, None, system_field
    input_field = None
    if prompt_field.lower() == "instruction":
        for c in schema.columns:
            if c.name.lower() == "input" and c.kind == "text":
                input_field = c.name
    used = {prompt_field, system_field, input_field}
    meta_fields = [c.name for c in schema.columns if c.name in schema.scalar and c.name not in used]
    return Plan(prompt_field, system_field, input_field, meta_fields), prompt_field, system_field


def _meta_value(v: Any) -> Any:
    if v is None or isinstance(v, (bytes, bytearray, memoryview)):
        return None
    if isinstance(v, bool):
        return v
    if isinstance(v, str):
        v = v.strip()
        return v[:MAX_META_CHARS] if v else None
    if isinstance(v, int):
        return v
    if isinstance(v, float):
        return v if math.isfinite(v) else None
    if isinstance(v, Decimal):
        f = float(v)
        return f if math.isfinite(f) else None
    if isinstance(v, (datetime, date)):
        return v.isoformat()
    return None


def build_prompt(row: dict[str, Any], plan: Plan) -> tuple[list[dict[str, str]], dict[str, Any]] | None:
    val = row.get(plan.prompt_field)
    if val is None:
        return None
    if isinstance(val, list):
        messages = normalize_messages(val)
        if not messages:
            return None
    else:
        if isinstance(val, (dict, bytes, bytearray)):
            return None
        text = str(val).strip()
        if plan.input_field is not None:
            extra = row.get(plan.input_field)
            if extra is not None and str(extra).strip():
                text = f"{text}\n\n{str(extra).strip()}" if text else str(extra).strip()
        if not text:
            return None
        messages = [{"role": "user", "content": text}]
    if plan.system_field is not None:
        sys_val = row.get(plan.system_field)
        if isinstance(sys_val, str) and sys_val.strip():
            if messages[0]["role"] == "system":
                messages[0] = {"role": "system", "content": sys_val.strip()}
            else:
                messages.insert(0, {"role": "system", "content": sys_val.strip()})
    meta: dict[str, Any] = {}
    for name in plan.meta_fields:
        v = _meta_value(row.get(name))
        if v is not None:
            meta[name] = v
    return messages, meta


def iter_prompts(
    rows: Iterable[dict[str, Any]], plan: Plan, stats: Stats, dedupe: bool = True
) -> Iterator[tuple[list[dict[str, str]], dict[str, Any]]]:
    seen: set[str] = set()
    for row in rows:
        stats.rows += 1
        built = build_prompt(row, plan)
        if built is None:
            stats.empty += 1
            continue
        if dedupe:
            key = built[0][-1]["content"]
            if key in seen:
                stats.duplicates += 1
                continue
            seen.add(key)
        yield built


def iter_text_prompts(
    pieces: Iterable[str], stats: Stats, dedupe: bool = True
) -> Iterator[tuple[list[dict[str, str]], dict[str, Any]]]:
    seen: set[str] = set()
    for piece in pieces:
        stats.rows += 1
        if dedupe:
            if piece in seen:
                stats.duplicates += 1
                continue
            seen.add(piece)
        yield [{"role": "user", "content": piece}], {}


# --------------------------------------------------------------------------- streams and previews


@dataclass
class Preview:
    detected_format: str
    columns: list[ColumnInfo]
    prompt_field: str | None
    system_field: str | None
    split_mode: str | None
    total_rows: int | None
    estimated_prompts: int | None
    prompts: list[list[dict[str, str]]]
    warnings: list[str]


class PromptStream:
    """Prompts ready to store, produced in batches. Check `warnings` after iterating."""

    def __init__(
        self,
        source: Table | TextDoc,
        mapping: ImportMapping,
        plan: Plan | None = None,
        pieces: list[str] | None = None,
        split_mode: str | None = None,
    ):
        self.source = source
        self.mapping = mapping
        self.plan = plan
        self.pieces = pieces
        self.split_mode = split_mode
        self.stats = Stats()
        self.count = 0

    @property
    def prompt_field(self) -> str | None:
        return self.plan.prompt_field if self.plan else None

    @property
    def system_field(self) -> str | None:
        return self.plan.system_field if self.plan else None

    @property
    def warnings(self) -> list[str]:
        return [*self.source.base_warnings, *getattr(self.source, "notes", {}).values(), *self.stats.warnings()]

    def _iter(self) -> Iterator[tuple[list[dict[str, str]], dict[str, Any]]]:
        m = self.mapping
        if isinstance(self.source, TextDoc):
            assert self.pieces is not None
            window = self.pieces[m.offset : None if m.limit is None else m.offset + m.limit]
            yield from iter_text_prompts(window, self.stats, m.dedupe)
            return
        assert self.plan is not None
        rows = self.source.rows(self.plan.read_columns(), m.offset, m.limit)
        with contextlib.closing(rows):
            yield from iter_prompts(rows, self.plan, self.stats, m.dedupe)

    def __iter__(self):
        return self._iter()

    def iter_batches(self, size: int = 5000) -> Iterator[list[tuple[list[dict[str, str]], dict[str, Any]]]]:
        it = self._iter()
        try:
            while True:
                batch = list(itertools.islice(it, size))
                if not batch:
                    return
                self.count += len(batch)
                yield batch
        finally:
            it.close()

    def close(self) -> None:
        self.source.close()


def open_stream(source: Table | TextDoc, mapping: ImportMapping) -> PromptStream:
    """Resolve fields and splitting. Raises ImportProblem when the source can't make prompts."""
    if isinstance(source, TextDoc):
        pieces, mode = split_text(source.text, mapping.split_mode, source.markdown)
        return PromptStream(source, mapping, pieces=pieces, split_mode=mode)
    plan, _, _ = resolve_plan(source.schema, source.sample(), mapping)
    if plan is None:
        raise ImportProblem(
            "Couldn't find a text column to use as the prompt. Choose the column that holds the prompts."
        )
    return PromptStream(source, mapping, plan=plan)


def preview_source(
    source: Table | TextDoc,
    mapping: ImportMapping,
    *,
    full_pass: bool | None = None,
    apply_window: bool = True,
) -> Preview:
    """First prompts and detected fields. `full_pass` counts every prompt (default: small sources)."""
    if isinstance(source, TextDoc):
        stream = open_stream(source, mapping)
        prompts = [p for p, _ in itertools.islice(stream, PREVIEW_PROMPTS)]
        full = PromptStream(source, mapping, pieces=stream.pieces, split_mode=stream.split_mode)
        n = sum(1 for _ in full)
        warnings = [*source.base_warnings, *full.stats.warnings()]
        if not prompts:
            warnings.append("No prompts found in this text.")
        return Preview(
            source.detected_format, [], None, None, stream.split_mode, len(stream.pieces or []), n, prompts, warnings
        )

    schema = source.schema
    plan, prompt_field, system_field = resolve_plan(schema, source.sample(), mapping)
    total = source.total_rows
    window_mapping = mapping if apply_window else mapping.model_copy(update={"offset": 0, "limit": None})
    warnings: list[str] = []
    prompts: list[list[dict[str, str]]] = []
    estimated: int | None = None
    if plan is None:
        warnings.append("No column looks like a prompt. Choose the column that holds the prompts.")
    else:
        stream = PromptStream(source, window_mapping, plan=plan)
        prompts = [p for p, _ in itertools.islice(stream, PREVIEW_PROMPTS)]
        if full_pass is None:
            full_pass = source.small
        if full_pass:
            full = PromptStream(source, window_mapping, plan=plan)
            estimated = sum(1 for _ in full)
            if total is None:
                total = full.stats.rows if mapping.offset == 0 and mapping.limit is None else None
            warnings.extend(full.stats.warnings())
            if not apply_window and (mapping.offset or mapping.limit):
                estimated = None
        warnings = [*source.warnings, *warnings]
        if not prompts:
            warnings.append("No prompts could be read with these settings.")
    if estimated is None and total is not None:
        estimated = max(0, min(mapping.limit if mapping.limit is not None else total, total - mapping.offset))
    if plan is None:
        warnings = [*source.warnings, *warnings]
    return Preview(
        source.detected_format, schema.columns, prompt_field, system_field, None, total, estimated, prompts, warnings
    )
