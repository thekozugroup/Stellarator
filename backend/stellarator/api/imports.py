"""Import prompts from a file, pasted text or a Hugging Face dataset."""

from __future__ import annotations

import asyncio
import re
import shutil
import uuid
from itertools import islice
from pathlib import Path
from typing import Any

from fastapi import APIRouter, Depends, File, HTTPException, Request, UploadFile
from sqlalchemy.ext.asyncio import AsyncSession

from ..config import UPLOAD_DIR
from ..db import get_session
from ..models import Prompt, PromptSet
from ..schemas import (
    FileSource,
    HFSource,
    ImportCommitIn,
    ImportMapping,
    ImportPreviewIn,
    ImportPreviewOut,
    PasteSource,
    PromptSetOut,
    UploadOut,
)
from ..services import hf, importers
from ..services.importers import ImportProblem
from ..services.settings import hf_token
from .prompt_sets import to_out

router = APIRouter(prefix="/api/imports", tags=["imports"])

MAX_UPLOAD_BYTES = 1024**3
_ID_RE = re.compile(r"^[0-9a-f]{32}$")
INSERT_BATCH = 5000


def _problem(e: ImportProblem) -> HTTPException:
    return HTTPException(e.status, e.message)


# --------------------------------------------------------------------------- upload


def _upload_file(upload_id: str) -> Path:
    if not _ID_RE.match(upload_id):
        raise HTTPException(404, "That upload was not found. Upload the file again.")
    folder = UPLOAD_DIR / upload_id
    files = [p for p in folder.iterdir() if p.is_file()] if folder.is_dir() else []
    if not files:
        raise HTTPException(404, "That upload was not found. It may have expired; upload the file again.")
    return files[0]


def _delete_upload(upload_id: str) -> None:
    if _ID_RE.match(upload_id):
        shutil.rmtree(UPLOAD_DIR / upload_id, ignore_errors=True)


@router.post("/upload", response_model=UploadOut)
async def upload(request: Request, file: UploadFile = File(...)):
    too_big = HTTPException(413, "That file is over the 1 GB upload limit. Split it or import a smaller file.")
    declared = request.headers.get("content-length")
    if declared and declared.isdigit() and int(declared) > MAX_UPLOAD_BYTES + (1 << 20):
        raise too_big
    await asyncio.to_thread(importers.cleanup_old_uploads, UPLOAD_DIR)
    upload_id = uuid.uuid4().hex
    folder = UPLOAD_DIR / upload_id
    folder.mkdir(parents=True)
    name = importers.sanitize_filename(file.filename)
    dest = folder / name
    size = 0
    try:
        with open(dest, "wb") as out:
            while chunk := await file.read(1 << 20):
                size += len(chunk)
                if size > MAX_UPLOAD_BYTES:
                    raise too_big
                out.write(chunk)
        if size == 0:
            raise HTTPException(422, "That file is empty.")
        fmt = await asyncio.to_thread(importers.detect_format, dest, name)
    except BaseException:
        shutil.rmtree(folder, ignore_errors=True)
        raise
    return UploadOut(upload_id=upload_id, filename=name, size=size, detected_format=fmt)


# --------------------------------------------------------------------------- preview


def _file_preview(path: Path, source: FileSource, mapping: ImportMapping) -> importers.Preview:
    src = importers.open_path(path, source.format, path.name)
    try:
        return importers.preview_source(src, mapping)
    finally:
        src.close()


async def _hf_preview(source: HFSource, mapping: ImportMapping) -> ImportPreviewOut:
    data = await hf.fetch_preview(source.dataset, source.config, source.split)
    table = data.table
    apply_window = True
    if mapping.offset > 0:
        # first-rows only has the start of the split; read the requested window instead.
        export = await hf.fetch_parquet(data.dataset, data.config, data.split)
        token = await hf_token()

        def read() -> list[dict[str, Any]]:
            t = hf.open_export(export, token)
            return list(islice(t.rows(None, mapping.offset, 100), 100))

        try:
            rows = await asyncio.to_thread(read)
        except ImportProblem:
            raise
        except Exception as e:
            raise ImportProblem(f"Couldn't read rows from Hugging Face: {str(e)[:200]}", 502) from e
        table = importers.MemoryTable(rows, table.names(), "huggingface", total_rows=data.num_rows)
        # keep the columns from the dataset start so kinds stay stable
        apply_window = False
    pv = await asyncio.to_thread(importers.preview_source, table, mapping, full_pass=False, apply_window=apply_window)
    out = _to_out(pv)
    out.hf_splits = data.splits
    out.hf_config = data.config
    out.hf_split = data.split
    return out


def _to_out(pv: importers.Preview) -> ImportPreviewOut:
    return ImportPreviewOut(
        detected_format=pv.detected_format,
        columns=pv.columns,
        prompt_field=pv.prompt_field,
        system_field=pv.system_field,
        split_mode=pv.split_mode,  # type: ignore[arg-type]
        total_rows=pv.total_rows,
        estimated_prompts=pv.estimated_prompts,
        prompts=pv.prompts,
        warnings=pv.warnings,
    )


@router.post("/preview", response_model=ImportPreviewOut)
async def preview(body: ImportPreviewIn):
    source = body.source
    try:
        if isinstance(source, HFSource):
            return await _hf_preview(source, body.mapping)
        if isinstance(source, PasteSource):
            if not source.text.strip():
                raise ImportProblem("Paste some text to import.")
            doc = importers.paste_doc(source.text)
            pv = await asyncio.to_thread(importers.preview_source, doc, body.mapping)
            return _to_out(pv)
        path = _upload_file(source.upload_id)
        pv = await asyncio.to_thread(_file_preview, path, source, body.mapping)
        return _to_out(pv)
    except ImportProblem as e:
        raise _problem(e) from e


# --------------------------------------------------------------------------- commit


async def _open_stream(source: HFSource | FileSource | PasteSource, mapping: ImportMapping):
    """(stream, source_kind, source dict without prompt count)."""
    if isinstance(source, PasteSource):
        if not source.text.strip():
            raise ImportProblem("Paste some text to import.")
        doc = importers.paste_doc(source.text)
        stream = await asyncio.to_thread(importers.open_stream, doc, mapping)
        return stream, "paste", {"format": "paste", "split_mode": stream.split_mode}
    if isinstance(source, FileSource):
        path = _upload_file(source.upload_id)

        def go():
            src = importers.open_path(path, source.format, path.name)
            try:
                return importers.open_stream(src, mapping)
            except BaseException:
                src.close()
                raise

        stream = await asyncio.to_thread(go)
        info: dict[str, Any] = {"filename": path.name, "format": stream.source.detected_format}
        if stream.plan is None:
            info["split_mode"] = stream.split_mode
        else:
            info["prompt_field"] = stream.prompt_field
            if stream.system_field:
                info["system_field"] = stream.system_field
        return stream, "file", info
    export = await hf.fetch_parquet(source.dataset, source.config, source.split)
    token = await hf_token()

    def go_hf():
        try:
            table = hf.open_export(export, token)
            return importers.open_stream(table, mapping)
        except ImportProblem:
            raise
        except Exception as e:
            raise ImportProblem(f"Couldn't read rows from Hugging Face: {str(e)[:200]}", 502) from e

    stream = await asyncio.to_thread(go_hf)
    info = {
        "dataset": export.dataset,
        "config": export.config,
        "split": export.split,
        "prompt_field": stream.prompt_field,
    }
    if stream.system_field:
        info["system_field"] = stream.system_field
    return stream, "huggingface", info


@router.post("", response_model=PromptSetOut, status_code=201)
async def commit(body: ImportCommitIn, session: AsyncSession = Depends(get_session)):
    try:
        stream, kind, info = await _open_stream(body.source, body.mapping)
    except ImportProblem as e:
        raise _problem(e) from e
    ps = PromptSet(name=body.name.strip() or body.name, source_kind=kind, source=info, prompt_count=0)
    try:
        session.add(ps)
        await session.flush()
        batches = stream.iter_batches(INSERT_BATCH)
        total = 0
        while True:
            try:
                batch = await asyncio.to_thread(next, batches, None)
            except ImportProblem as e:
                raise _problem(e) from e
            except Exception as e:
                raise HTTPException(502, f"Couldn't read the source while importing: {str(e)[:200]}") from e
            if batch is None:
                break
            rows = [
                {"prompt_set_id": ps.id, "idx": total + i, "messages": m, "meta": meta}
                for i, (m, meta) in enumerate(batch)
            ]
            await session.execute(Prompt.__table__.insert(), rows)
            total += len(rows)
        if total == 0:
            raise HTTPException(
                422, "No prompts were found with these settings. Check the column or split mode and try again."
            )
        ps.prompt_count = total
        ps.source = {**info, "rows": total}
        await session.commit()
    except BaseException:
        await session.rollback()
        raise
    finally:
        await asyncio.to_thread(stream.close)
    if isinstance(body.source, FileSource):
        await asyncio.to_thread(_delete_upload, body.source.upload_id)
    return to_out(ps, 0)
