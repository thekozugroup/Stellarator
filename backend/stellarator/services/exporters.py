"""Dataset exports: record builders, file writers, filtered DB iteration and Hub upload.

Records are built from done samples (SFT) or preference pairs (DPO/KTO). Writers stream:
jsonl, json and csv yield bytes as rows are read in batches; parquet is written to a temp
file under DATA_DIR/exports in row-group batches, then streamed and deleted.
"""

from __future__ import annotations

import asyncio
import csv
import io
import json
import re
import unicodedata
import uuid
from collections.abc import AsyncIterator
from contextlib import aclosing
from datetime import timezone
from pathlib import Path
from typing import Any

import pyarrow as pa
import pyarrow.parquet as pq
from fastapi import HTTPException
from huggingface_hub import HfApi
from huggingface_hub.errors import HfHubHTTPError, HFValidationError
from sqlalchemy import and_, case, false, func, not_, or_, select, tuple_
from sqlalchemy.ext.asyncio import AsyncSession

from ..config import DATA_DIR
from ..db import SessionLocal
from ..models import (
    SAMPLE_DONE,
    SAMPLE_FAILED,
    SAMPLE_PENDING,
    SAMPLE_RUNNING,
    PreferencePair,
    Prompt,
    Run,
    Sample,
)
from ..schemas import ExportOptions, ExportPreviewOut, HubPushIn, HubPushOut, PrefExportOptions

BATCH = 500
OUTSIDE = ("reasoning_content", "thinking")  # placements that need a field outside the content

MEDIA_TYPES = {
    "jsonl": "application/x-ndjson",
    "json": "application/json",
    "parquet": "application/vnd.apache.parquet",
    "csv": "text/csv",
}

HF_TOKEN_MISSING = "Add a Hugging Face token in Settings to push datasets."


def export_dir() -> Path:
    d = DATA_DIR / "exports"
    d.mkdir(parents=True, exist_ok=True)
    return d


def slugify(name: str) -> str:
    text = unicodedata.normalize("NFKD", name).encode("ascii", "ignore").decode()
    text = re.sub(r"[^a-z0-9._]+", "-", text.lower()).strip("-._")
    return text[:80] or "export"


def filename_for(name: str, fmt: str, file_type: str) -> str:
    return f"{slugify(name)}-{fmt}.{file_type}"


# --------------------------------------------------------------------------- prompt rendering


def render_prompt(messages: list[dict[str, Any]], params: dict[str, Any] | None, include_system: bool = True):
    """The prompt as the model saw it: run system prompt and user template applied."""
    params = params or {}
    msgs = [{"role": m.get("role"), "content": m.get("content") or ""} for m in messages]
    system_prompt = params.get("system_prompt")
    if system_prompt:
        msgs = [m for m in msgs if m["role"] != "system"]
        msgs.insert(0, {"role": "system", "content": system_prompt})
    template = params.get("user_template")
    if template:
        for m in reversed(msgs):
            if m["role"] == "user":
                m["content"] = template.replace("{prompt}", m["content"])
                break
    if not include_system:
        msgs = [m for m in msgs if m["role"] != "system"]
    return msgs


def think_wrap(reasoning: str | None, response: str | None) -> str:
    reasoning = (reasoning or "").strip("\n")
    response = response or ""
    if not (reasoning or "").strip():
        return response
    return f"<think>\n{reasoning}\n</think>\n\n{response}"


def assistant_message(reasoning: str | None, response: str | None, placement: str) -> dict[str, Any]:
    """Assistant turn as a chat message object."""
    reasoning = reasoning if (reasoning or "").strip() else None
    response = response or ""
    if placement == "think_tags":
        return {"role": "assistant", "content": think_wrap(reasoning, response)}
    msg: dict[str, Any] = {"role": "assistant", "content": response}
    if placement in OUTSIDE:
        msg[placement] = reasoning
    return msg


def assistant_text(reasoning: str | None, response: str | None, placement: str) -> tuple[str, str | None]:
    """Assistant turn for formats without message objects: (text, reasoning to put in a side field)."""
    reasoning_v = reasoning if (reasoning or "").strip() else None
    if placement == "think_tags":
        return think_wrap(reasoning_v, response), None
    if placement in OUTSIDE:
        return response or "", reasoning_v
    return response or "", None


def render_prompt_text(messages: list[dict[str, Any]]) -> str:
    """Plain-text prompt: system + blank line + user, or role-labelled turns for multi-turn."""
    system = "\n\n".join(m["content"] for m in messages if m["role"] == "system")
    rest = [m for m in messages if m["role"] != "system"]
    if len(rest) == 1 and rest[0]["role"] == "user":
        return f"{system}\n\n{rest[0]['content']}" if system else rest[0]["content"]
    labels = {"system": "System", "user": "User", "assistant": "Assistant"}
    return "\n\n".join(f"{labels.get(m['role'], m['role'])}: {m['content']}" for m in messages)


def _iso(dt) -> str | None:
    if dt is None:
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.isoformat()


# --------------------------------------------------------------------------- SFT records


def _history_pairs(turns: list[dict[str, Any]]) -> list[list[str]]:
    pairs: list[list[str]] = []
    pending: str | None = None
    for m in turns:
        if m["role"] == "user":
            if pending is not None:
                pairs.append([pending, ""])
            pending = m["content"]
        elif m["role"] == "assistant":
            pairs.append([pending or "", m["content"]])
            pending = None
    if pending is not None:
        pairs.append([pending, ""])
    return pairs


def sft_record(
    options: ExportOptions,
    run: Run,
    sample: Sample,
    prompt_messages: list[dict[str, Any]],
    prompt_meta: dict[str, Any] | None = None,
) -> dict[str, Any]:
    params = run.params or {}
    prompt = render_prompt(prompt_messages, params, options.include_system)
    fmt = options.format
    placement = options.reasoning
    outside = placement in OUTSIDE

    if fmt == "raw":
        return {
            "id": sample.id,
            "run_id": sample.run_id,
            "prompt_id": sample.prompt_id,
            "prompt_index": sample.prompt_idx,
            "sample_index": sample.sample_idx,
            "model": run.model,
            "endpoint": run.endpoint_name,
            "messages": prompt,
            "reasoning": sample.reasoning or "",
            "response": sample.response or "",
            "finish_reason": sample.finish_reason,
            "usage": {
                "prompt_tokens": sample.prompt_tokens,
                "completion_tokens": sample.completion_tokens,
                "reasoning_tokens": sample.reasoning_tokens,
            },
            "latency_ms": sample.latency_ms,
            "ttft_ms": sample.ttft_ms,
            "params": params,
            "meta": prompt_meta or {},
            "created_at": _iso(sample.completed_at),
        }

    if fmt in ("messages", "prompt_completion"):
        assistant = assistant_message(sample.reasoning, sample.response, placement)
        if fmt == "messages":
            return {"messages": [*prompt, assistant]}
        return {"prompt": prompt, "completion": [assistant]}

    text, side = assistant_text(sample.reasoning, sample.response, placement)
    if fmt == "sharegpt":
        roles = {"system": "system", "user": "human", "assistant": "gpt"}
        conv = [{"from": roles.get(m["role"], m["role"]), "value": m["content"]} for m in prompt]
        conv.append({"from": "gpt", "value": text})
        rec: dict[str, Any] = {"conversations": conv}
    else:  # alpaca
        system = next((m["content"] for m in prompt if m["role"] == "system"), None)
        turns = [m for m in prompt if m["role"] != "system"]
        instruction = turns[-1]["content"] if turns and turns[-1]["role"] == "user" else ""
        earlier = turns[:-1] if turns and turns[-1]["role"] == "user" else turns
        rec = {"instruction": instruction, "input": "", "output": text}
        if system is not None:
            rec["system"] = system
        history = _history_pairs(earlier)
        if history:
            rec["history"] = history
    if outside:
        rec["reasoning"] = side
    return rec


# --------------------------------------------------------------------------- preference records


def pref_records(
    options: PrefExportOptions,
    chosen: Sample,
    rejected: Sample,
    prompt_messages: list[dict[str, Any]],
    params: dict[str, Any] | None,
) -> list[dict[str, Any]]:
    prompt = render_prompt(prompt_messages, params, options.include_system)
    placement = options.reasoning
    outside = placement in OUTSIDE

    if options.conversational:
        c = [assistant_message(chosen.reasoning, chosen.response, placement)]
        r = [assistant_message(rejected.reasoning, rejected.response, placement)]
        if options.format == "dpo":
            return [{"prompt": prompt, "chosen": c, "rejected": r}]
        return [
            {"prompt": prompt, "completion": c, "label": True},
            {"prompt": prompt, "completion": r, "label": False},
        ]

    ptext = render_prompt_text(prompt)
    ctext, cside = assistant_text(chosen.reasoning, chosen.response, placement)
    rtext, rside = assistant_text(rejected.reasoning, rejected.response, placement)
    if options.format == "dpo":
        rec: dict[str, Any] = {"prompt": ptext, "chosen": ctext, "rejected": rtext}
        if outside:
            rec["chosen_reasoning"] = cside
            rec["rejected_reasoning"] = rside
        return [rec]
    good: dict[str, Any] = {"prompt": ptext, "completion": ctext, "label": True}
    bad: dict[str, Any] = {"prompt": ptext, "completion": rtext, "label": False}
    if outside:
        good["reasoning"] = cside
        bad["reasoning"] = rside
    return [good, bad]


# --------------------------------------------------------------------------- schemas (parquet / csv columns)


def _msg_type(extra: str | None) -> pa.DataType:
    fields = [("role", pa.string()), ("content", pa.string())]
    if extra:
        fields.append((extra, pa.string()))
    return pa.struct(fields)


def _extra(placement: str) -> str | None:
    return placement if placement in OUTSIDE else None


def sft_schema(options: ExportOptions) -> pa.Schema:
    msg = _msg_type(_extra(options.reasoning))
    outside = options.reasoning in OUTSIDE
    s = pa.string()
    i = pa.int64()
    fmt = options.format
    if fmt == "messages":
        fields = [("messages", pa.list_(msg))]
    elif fmt == "prompt_completion":
        fields = [("prompt", pa.list_(msg)), ("completion", pa.list_(msg))]
    elif fmt == "sharegpt":
        fields = [("conversations", pa.list_(pa.struct([("from", s), ("value", s)])))]
        if outside:
            fields.append(("reasoning", s))
    elif fmt == "alpaca":
        fields = [
            ("instruction", s),
            ("input", s),
            ("output", s),
            ("system", s),
            ("history", pa.list_(pa.list_(s))),
        ]
        if outside:
            fields.append(("reasoning", s))
    else:  # raw
        fields = [
            ("id", i),
            ("run_id", i),
            ("prompt_id", i),
            ("prompt_index", i),
            ("sample_index", i),
            ("model", s),
            ("endpoint", s),
            ("messages", pa.list_(_msg_type(None))),
            ("reasoning", s),
            ("response", s),
            ("finish_reason", s),
            (
                "usage",
                pa.struct([("prompt_tokens", i), ("completion_tokens", i), ("reasoning_tokens", i)]),
            ),
            ("latency_ms", i),
            ("ttft_ms", i),
            ("params", s),  # JSON-encoded in parquet and csv
            ("meta", s),
            ("created_at", s),
        ]
    return pa.schema(fields)


def pref_schema(options: PrefExportOptions) -> pa.Schema:
    s = pa.string()
    outside = options.reasoning in OUTSIDE
    if options.conversational:
        msgs = pa.list_(_msg_type(_extra(options.reasoning)))
        if options.format == "dpo":
            return pa.schema([("prompt", msgs), ("chosen", msgs), ("rejected", msgs)])
        return pa.schema([("prompt", msgs), ("completion", msgs), ("label", pa.bool_())])
    if options.format == "dpo":
        fields = [("prompt", s), ("chosen", s), ("rejected", s)]
        if outside:
            fields += [("chosen_reasoning", s), ("rejected_reasoning", s)]
    else:
        fields = [("prompt", s), ("completion", s), ("label", pa.bool_())]
        if outside:
            fields.append(("reasoning", s))
    return pa.schema(fields)


def _coerce(record: dict[str, Any], schema: pa.Schema) -> dict[str, Any]:
    """Make a record fit the schema: JSON-encode dicts bound for string columns."""
    out = {}
    for field in schema:
        v = record.get(field.name)
        if pa.types.is_string(field.type) and isinstance(v, (dict, list)):
            v = json.dumps(v, ensure_ascii=False)
        out[field.name] = v
    return out


def _csv_cell(v: Any) -> Any:
    if v is None:
        return ""
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (dict, list)):
        return json.dumps(v, ensure_ascii=False)
    return v


# --------------------------------------------------------------------------- file writers


def _dumps(record: dict[str, Any]) -> str:
    return json.dumps(record, ensure_ascii=False)


async def _write_parquet(path: Path, records: AsyncIterator[dict[str, Any]], schema: pa.Schema) -> None:
    writer = pq.ParquetWriter(str(path), schema, compression="zstd")

    def flush(rows: list[dict[str, Any]]) -> None:
        writer.write_table(pa.Table.from_pylist([_coerce(r, schema) for r in rows], schema=schema))

    try:
        buf: list[dict[str, Any]] = []
        async for rec in records:
            buf.append(rec)
            if len(buf) >= BATCH:
                await asyncio.to_thread(flush, buf)
                buf = []
        if buf:
            await asyncio.to_thread(flush, buf)
    finally:
        writer.close()


async def write_parquet_file(records: AsyncIterator[dict[str, Any]], schema: pa.Schema) -> Path:
    path = export_dir() / f"{uuid.uuid4().hex}.parquet"
    try:
        await _write_parquet(path, records, schema)
    except BaseException:
        path.unlink(missing_ok=True)
        raise
    return path


async def stream_file(
    file_type: str, records: AsyncIterator[dict[str, Any]], schema: pa.Schema
) -> AsyncIterator[bytes]:
    """Serialise records to the chosen file type as a stream of bytes."""
    if file_type == "jsonl":
        buf: list[str] = []
        async for rec in records:
            buf.append(_dumps(rec))
            if len(buf) >= 100:
                yield ("\n".join(buf) + "\n").encode()
                buf = []
        if buf:
            yield ("\n".join(buf) + "\n").encode()
    elif file_type == "json":
        yield b"["
        first = True
        buf = []
        async for rec in records:
            buf.append(_dumps(rec))
            if len(buf) >= 100:
                yield (("" if first else ",") + "\n" + ",\n".join(buf)).encode()
                first = False
                buf = []
        if buf:
            yield (("" if first else ",") + "\n" + ",\n".join(buf)).encode()
        yield b"\n]\n"
    elif file_type == "csv":
        names = [f.name for f in schema]
        sio = io.StringIO()
        w = csv.writer(sio, lineterminator="\n")
        w.writerow(names)
        yield sio.getvalue().encode()
        sio = io.StringIO()
        w = csv.writer(sio, lineterminator="\n")
        n = 0
        async for rec in records:
            coerced = _coerce_csv(rec, names)
            w.writerow(coerced)
            n += 1
            if n >= 100:
                yield sio.getvalue().encode()
                sio = io.StringIO()
                w = csv.writer(sio, lineterminator="\n")
                n = 0
        if n:
            yield sio.getvalue().encode()
    else:  # parquet
        path = await write_parquet_file(records, schema)
        try:
            with open(path, "rb") as fh:
                while chunk := await asyncio.to_thread(fh.read, 1 << 20):
                    yield chunk
        finally:
            path.unlink(missing_ok=True)


def _coerce_csv(rec: dict[str, Any], names: list[str]) -> list[Any]:
    return [_csv_cell(rec.get(n)) for n in names]


# --------------------------------------------------------------------------- SFT: filters and iteration


def _blank(col):
    return or_(col.is_(None), func.trim(col, " \t\r\n") == "")


def _sft_conditions(options: ExportOptions) -> dict[str, Any]:
    done = Sample.status == SAMPLE_DONE
    empty = and_(_blank(Sample.response), _blank(Sample.reasoning))
    extra = Sample.sample_idx > 0 if options.first_sample_only else false()
    trunc = func.coalesce(Sample.finish_reason, "") == "length" if options.exclude_truncated else false()
    noreas = _blank(Sample.reasoning) if options.require_reasoning else false()
    return {
        "done": done,
        "empty": empty,
        "extra": extra,
        "trunc": trunc,
        "noreas": noreas,
        "keep": and_(done, not_(empty), not_(extra), not_(trunc), not_(noreas)),
    }


async def sft_counts(session: AsyncSession, run_id: int, options: ExportOptions) -> tuple[int, dict[str, int]]:
    c = _sft_conditions(options)
    done, empty, extra, trunc, noreas = c["done"], c["empty"], c["extra"], c["trunc"], c["noreas"]

    def n(cond):
        return func.coalesce(func.sum(case((cond, 1), else_=0)), 0)

    row = (
        await session.execute(
            select(
                n(c["keep"]),
                n(Sample.status == SAMPLE_FAILED),
                n(Sample.status.in_((SAMPLE_PENDING, SAMPLE_RUNNING))),
                n(and_(done, empty)),
                n(and_(done, not_(empty), extra)),
                n(and_(done, not_(empty), not_(extra), trunc)),
                n(and_(done, not_(empty), not_(extra), not_(trunc), noreas)),
            ).where(Sample.run_id == run_id)
        )
    ).one()
    keep, failed, pending, empty_n, extra_n, trunc_n, noreas_n = (int(v) for v in row)
    skipped = {
        "failed": failed,
        "pending": pending,
        "truncated": trunc_n,
        "no_reasoning": noreas_n,
        "extra_samples": extra_n,
        "empty_response": empty_n,
    }
    return keep, {k: v for k, v in skipped.items() if v}


async def iter_sft_records(run: Run, options: ExportOptions, batch: int = BATCH) -> AsyncIterator[dict[str, Any]]:
    keep = _sft_conditions(options)["keep"]
    cursor: tuple[int, int] | None = None
    async with SessionLocal() as session:
        while True:
            q = (
                select(Sample, Prompt.messages, Prompt.meta)
                .join(Prompt, Prompt.id == Sample.prompt_id)
                .where(Sample.run_id == run.id, keep)
            )
            if cursor is not None:
                q = q.where(tuple_(Sample.prompt_idx, Sample.sample_idx) > cursor)
            q = q.order_by(Sample.prompt_idx, Sample.sample_idx).limit(batch)
            rows = (await session.execute(q)).all()
            await session.commit()  # end the read transaction between batches
            if not rows:
                return
            for sample, messages, meta in rows:
                yield sft_record(options, run, sample, messages, meta)
            last = rows[-1][0]
            cursor = (last.prompt_idx, last.sample_idx)
            if len(rows) < batch:
                return


async def sft_preview(session: AsyncSession, run: Run, options: ExportOptions) -> ExportPreviewOut:
    count, skipped = await sft_counts(session, run.id, options)
    example = None
    async with aclosing(iter_sft_records(run, options, batch=1)) as it:
        async for rec in it:
            example = rec
            break
    return ExportPreviewOut(
        count=count,
        skipped=skipped,
        example=example,
        filename=filename_for(run.name, options.format, options.file_type),
    )


# --------------------------------------------------------------------------- preference: iteration


async def pref_count(session: AsyncSession, set_id: int, options: PrefExportOptions) -> int:
    n = (
        await session.execute(
            select(func.count()).select_from(PreferencePair).where(PreferencePair.preference_set_id == set_id)
        )
    ).scalar_one()
    return n * (2 if options.format == "kto" else 1)


async def iter_pref_records(
    set_id: int, options: PrefExportOptions, batch: int = BATCH
) -> AsyncIterator[dict[str, Any]]:
    cursor: int | None = None
    run_params: dict[int, dict[str, Any]] = {}
    async with SessionLocal() as session:
        while True:
            q = (
                select(PreferencePair, Prompt.idx, Prompt.messages)
                .join(Prompt, Prompt.id == PreferencePair.prompt_id)
                .where(PreferencePair.preference_set_id == set_id)
            )
            if cursor is not None:
                q = q.where(Prompt.idx > cursor)
            rows = (await session.execute(q.order_by(Prompt.idx).limit(batch))).all()
            if not rows:
                await session.commit()
                return
            ids = {p.chosen_sample_id for p, _, _ in rows} | {p.rejected_sample_id for p, _, _ in rows}
            samples = {
                s.id: s for s in (await session.execute(select(Sample).where(Sample.id.in_(ids)))).scalars()
            }
            missing = {s.run_id for s in samples.values()} - run_params.keys()
            if missing:
                for rid, params in (await session.execute(select(Run.id, Run.params).where(Run.id.in_(missing)))).all():
                    run_params[rid] = params or {}
            await session.commit()
            for pair, _idx, messages in rows:
                chosen, rejected = samples.get(pair.chosen_sample_id), samples.get(pair.rejected_sample_id)
                if chosen is None or rejected is None:
                    continue
                for rec in pref_records(options, chosen, rejected, messages, run_params.get(chosen.run_id)):
                    yield rec
            cursor = rows[-1][1]
            if len(rows) < batch:
                return


async def pref_preview(session: AsyncSession, pset_name: str, set_id: int, options: PrefExportOptions) -> ExportPreviewOut:
    count = await pref_count(session, set_id, options)
    example = None
    async with aclosing(iter_pref_records(set_id, options, batch=1)) as it:
        async for rec in it:
            example = rec
            break
    return ExportPreviewOut(
        count=count,
        skipped={},
        example=example,
        filename=filename_for(pset_name, options.format, options.file_type),
    )


# --------------------------------------------------------------------------- Hugging Face


def _param_line(params: dict[str, Any]) -> str:
    keys = ("samples_per_prompt", "temperature", "top_p", "max_tokens", "seed")
    parts = [f"{k}={params[k]}" for k in keys if params.get(k) is not None]
    if params.get("system_prompt"):
        parts.append("system prompt set")
    if params.get("user_template"):
        parts.append("user template set")
    return ", ".join(parts) or "defaults"


def _card(title: str, split: str, lines: list[str]) -> str:
    front = (
        "---\n"
        "configs:\n"
        "- config_name: default\n"
        "  data_files:\n"
        f"  - split: {split}\n"
        f"    path: data/{split}-*\n"
        "---\n\n"
    )
    return front + f"# {title}\n\n" + "\n".join(f"- {line}" for line in lines) + "\n"


def run_card(run: Run, prompt_set_name: str, count: int, options: ExportOptions, split: str) -> str:
    return _card(
        run.name,
        split,
        [
            f"Model: {run.model}",
            f"Endpoint: {run.endpoint_name}",
            f"Prompt set: {prompt_set_name}",
            f"Records: {count}",
            f"Format: {options.format} (reasoning: {options.reasoning})",
            f"Generation parameters: {_param_line(run.params or {})}",
            "Generated with Stellarator.",
        ],
    )


def pref_card(
    set_name: str, prompt_set_name: str, runs: list[Run], count: int, options: PrefExportOptions, split: str
) -> str:
    models = ", ".join(sorted({r.model for r in runs})) or "unknown"
    endpoints = ", ".join(sorted({r.endpoint_name for r in runs})) or "unknown"
    params = "; ".join(f"{r.name}: {_param_line(r.params or {})}" for r in runs) or "unknown"
    kind = "conversational" if options.conversational else "text"
    return _card(
        set_name,
        split,
        [
            f"Models: {models}",
            f"Endpoints: {endpoints}",
            f"Prompt set: {prompt_set_name}",
            f"Records: {count}",
            f"Format: {options.format}, {kind} (reasoning: {options.reasoning})",
            f"Generation parameters: {params}",
            "Generated with Stellarator.",
        ],
    )


def hub_http_error(exc: BaseException, repo_id: str) -> HTTPException:
    if isinstance(exc, HFValidationError):
        return HTTPException(400, "The repo id must look like owner/name, using letters, numbers, dashes and dots.")
    if isinstance(exc, HfHubHTTPError):
        status = getattr(getattr(exc, "response", None), "status_code", None)
        if status == 401:
            return HTTPException(400, "Hugging Face rejected the token. Check it in Settings.")
        if status == 403:
            return HTTPException(
                400, f"The token cannot write to {repo_id}. Use a token with write access to that account."
            )
        if status == 404:
            return HTTPException(
                400, f"Hugging Face could not find {repo_id}. Check the owner name in the repo id."
            )
        if status == 409:
            return HTTPException(409, f"{repo_id} already exists in a way that conflicts. Pick another repo id.")
        return HTTPException(502, f"Hugging Face returned an error ({status}). Try again in a moment.")
    return HTTPException(502, "Could not reach Hugging Face. Check the network connection and try again.")


def _upload(token: str, target: HubPushIn, parquet_path: Path, card: str) -> None:
    api = HfApi(token=token)
    api.create_repo(repo_id=target.repo_id, repo_type="dataset", private=target.private, exist_ok=True)
    api.upload_file(
        path_or_fileobj=str(parquet_path),
        path_in_repo=f"data/{target.split}-00000-of-00001.parquet",
        repo_id=target.repo_id,
        repo_type="dataset",
        commit_message="Add data from Stellarator",
    )
    api.upload_file(
        path_or_fileobj=card.encode("utf-8"),
        path_in_repo="README.md",
        repo_id=target.repo_id,
        repo_type="dataset",
        commit_message="Add dataset card",
    )


async def push_to_hub(
    *,
    token: str | None,
    target: HubPushIn,
    records: AsyncIterator[dict[str, Any]],
    schema: pa.Schema,
    count: int,
    card: str,
) -> HubPushOut:
    if not token:
        raise HTTPException(400, HF_TOKEN_MISSING)
    if not re.fullmatch(r"[A-Za-z0-9_.-]+", target.split):
        raise HTTPException(400, "The split name can only use letters, numbers, dots, dashes and underscores.")
    if count == 0:
        raise HTTPException(400, "There are no records to push with these options.")
    path = await write_parquet_file(records, schema)
    try:
        await asyncio.to_thread(_upload, token, target, path, card)
    except HTTPException:
        raise
    except Exception as exc:  # noqa: BLE001 - mapped to a one-sentence error
        raise hub_http_error(exc, target.repo_id) from exc
    finally:
        path.unlink(missing_ok=True)
    return HubPushOut(url=f"https://huggingface.co/datasets/{target.repo_id}", count=count)


def strip_none(v: Any) -> Any:
    """Drop None values recursively (parquet round trips add nulls for absent keys)."""
    if isinstance(v, dict):
        return {k: strip_none(x) for k, x in v.items() if x is not None}
    if isinstance(v, list):
        return [strip_none(x) for x in v]
    return v
