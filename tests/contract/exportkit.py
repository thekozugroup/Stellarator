"""Parse export downloads of every file type back into records."""

from __future__ import annotations

import csv
import io
import json
from typing import Any

import httpx
import pyarrow.parquet as pq

MEDIA_TYPES = {
    "jsonl": "application/x-ndjson",
    "json": "application/json",
    "parquet": "application/vnd.apache.parquet",
    "csv": "text/csv",
}
FILE_TYPES = list(MEDIA_TYPES)


def strip_none(v: Any) -> Any:
    if isinstance(v, dict):
        return {k: strip_none(x) for k, x in v.items() if x is not None}
    if isinstance(v, list):
        return [strip_none(x) for x in v]
    return v


def assert_download_headers(resp: httpx.Response, file_type: str, filename: str) -> None:
    assert resp.status_code == 200, resp.text[:300]
    assert resp.headers["content-type"].split(";")[0].strip() == MEDIA_TYPES[file_type]
    assert resp.headers["content-disposition"] == f'attachment; filename="{filename}"'


def parse_download(file_type: str, body: bytes) -> tuple[list[dict], list[str] | None]:
    """(records, column names). jsonl and json have no fixed columns, so None."""
    if file_type == "jsonl":
        text = body.decode("utf-8")
        assert text == "" or text.endswith("\n"), "jsonl ends with a newline"
        return [json.loads(line) for line in text.split("\n") if line], None
    if file_type == "json":
        data = json.loads(body.decode("utf-8"))
        assert isinstance(data, list)
        return data, None
    if file_type == "csv":
        text = body.decode("utf-8")
        reader = csv.reader(io.StringIO(text, newline=""))
        rows = list(reader)
        assert rows, "csv has a header row"
        header = rows[0]
        assert len(set(header)) == len(header)
        return [dict(zip(header, r, strict=True)) for r in rows[1:]], header
    assert body[:4] == b"PAR1" and body[-4:] == b"PAR1", "parquet magic bytes"
    table = pq.read_table(io.BytesIO(body))
    return table.to_pylist(), table.column_names


def assert_same_records(jsonl_records: list[dict], file_type: str, parsed: list[dict], columns: list[str] | None) -> None:
    """The records of another file type carry the same data as the jsonl records."""
    assert len(parsed) == len(jsonl_records), f"{file_type}: {len(parsed)} records != {len(jsonl_records)}"
    if file_type in ("jsonl", "json"):
        assert parsed == jsonl_records
        return
    assert columns is not None
    for rec, row in zip(jsonl_records, parsed, strict=True):
        assert set(rec) <= set(columns), f"{file_type}: columns {columns} miss keys of {sorted(rec)}"
        if file_type == "parquet":
            row = {k: (json.loads(v) if k in ("params", "meta") and isinstance(v, str) else v) for k, v in row.items()}
            assert strip_none(row) == strip_none(rec)
            continue
        for key in columns:
            want, cell = rec.get(key), row[key]
            if isinstance(want, (dict, list)):
                assert json.loads(cell) == want, (key, cell)
            elif want is None:
                assert cell == "", (key, cell)
            elif isinstance(want, bool):
                assert cell == ("true" if want else "false"), (key, cell)
            else:
                assert cell == str(want), (key, cell, want)
