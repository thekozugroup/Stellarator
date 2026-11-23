"""A tiny response-shape checker.

`check(value, spec)` raises AssertionError naming the path of the first mismatch, so a failing test
says "$.items[3].status: 'queued' is not one of [...]" instead of a bare KeyError.

Specs:
  str / int / float / bool / dict / list   isinstance check (bool is not an int; an int is a float)
  ANY                                      anything, including null
  Opt(spec)                                null or spec
  OneOf("a", "b")                          one of the listed values
  ListOf(spec) / MapOf(spec)               every item / every value matches spec
  Obj({key: spec}, optional={key: spec})   exactly these keys; `optional` ones may be absent
  DT                                       RFC 3339 timestamp that carries a UTC offset

Fields added by the v2 API are listed in `optional` for v1-era tests and moved into the required
keys by the tests marked `v2` (see `Obj.require`).
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import Any


class _Any:
    def __repr__(self) -> str:
        return "ANY"


ANY = _Any()


@dataclass(frozen=True)
class Opt:
    spec: Any


@dataclass(frozen=True)
class OneOf:
    values: tuple

    def __init__(self, *values: Any):
        object.__setattr__(self, "values", tuple(values))


@dataclass(frozen=True)
class ListOf:
    spec: Any


@dataclass(frozen=True)
class MapOf:
    spec: Any


class _DT:
    def __repr__(self) -> str:
        return "DT"


DT = _DT()


@dataclass(frozen=True)
class Obj:
    required: dict
    optional: dict | None = None

    def require(self) -> "Obj":
        """The same object with its optional (v2) keys required."""
        return Obj({**self.required, **(self.optional or {})}, None)


def parse_dt(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def check(value: Any, spec: Any, path: str = "$") -> None:
    if spec is ANY:
        return
    if isinstance(spec, Opt):
        if value is not None:
            check(value, spec.spec, path)
        return
    if isinstance(spec, OneOf):
        assert value in spec.values, f"{path}: {value!r} is not one of {list(spec.values)}"
        return
    if spec is DT:
        assert isinstance(value, str), f"{path}: expected an RFC 3339 string, got {value!r}"
        try:
            parsed = parse_dt(value)
        except ValueError:
            raise AssertionError(f"{path}: {value!r} is not an RFC 3339 timestamp") from None
        assert parsed.tzinfo is not None, f"{path}: {value!r} has no UTC offset"
        return
    if isinstance(spec, ListOf):
        assert isinstance(value, list), f"{path}: expected a list, got {type(value).__name__}"
        for i, item in enumerate(value):
            check(item, spec.spec, f"{path}[{i}]")
        return
    if isinstance(spec, MapOf):
        assert isinstance(value, dict), f"{path}: expected an object, got {type(value).__name__}"
        for k, item in value.items():
            check(item, spec.spec, f"{path}.{k}")
        return
    if isinstance(spec, Obj):
        assert isinstance(value, dict), f"{path}: expected an object, got {type(value).__name__}"
        optional = spec.optional or {}
        missing = set(spec.required) - set(value)
        extra = set(value) - set(spec.required) - set(optional)
        assert not missing, f"{path}: missing keys {sorted(missing)} (has {sorted(value)})"
        assert not extra, f"{path}: unexpected keys {sorted(extra)}"
        for key, sub in {**spec.required, **optional}.items():
            if key in value:
                check(value[key], sub, f"{path}.{key}")
        return
    if spec is bool:
        assert isinstance(value, bool), f"{path}: expected bool, got {value!r}"
        return
    if spec is int:
        assert isinstance(value, int) and not isinstance(value, bool), f"{path}: expected int, got {value!r}"
        return
    if spec is float:
        assert isinstance(value, (int, float)) and not isinstance(value, bool), (
            f"{path}: expected a number, got {value!r}"
        )
        return
    if isinstance(spec, type):
        assert isinstance(value, spec), f"{path}: expected {spec.__name__}, got {value!r}"
        return
    raise TypeError(f"bad spec {spec!r}")


# --------------------------------------------------------------------------- shared shapes

MESSAGE = Obj({"role": OneOf("system", "user", "assistant"), "content": str})

MCP_TRANSPORT = OneOf("stdio", "http")

RUN_TOOLS = Obj(
    {
        "servers": ListOf(Obj({"server_id": int, "tools": Opt(ListOf(str))})),
        "max_rounds": int,
        "tool_timeout_s": float,
        "max_result_chars": int,
        "tool_choice": OneOf("auto", "required"),
    }
)

TOOL_DEFINITION = Obj(
    {"type": OneOf("function"), "function": Obj({"name": str, "parameters": dict}, optional={"description": str})}
)

TOOL_SNAPSHOT = Obj(
    {
        "servers": ListOf(
            Obj(
                {
                    "server_id": int,
                    "name": str,
                    "transport": MCP_TRANSPORT,
                    "tools": ListOf(Obj({"name": str, "server_tool": str})),
                }
            )
        ),
        "definitions": ListOf(TOOL_DEFINITION),
    }
)

TOOL_CALL = Obj({"id": str, "name": str, "arguments": str})

TRACE_ASSISTANT = Obj(
    {
        "role": OneOf("assistant"),
        "reasoning": Opt(str),
        "content": str,
        "tool_calls": ListOf(TOOL_CALL),
        "finish_reason": Opt(str),
        "prompt_tokens": Opt(int),
        "completion_tokens": Opt(int),
        "reasoning_tokens": Opt(int),
        "latency_ms": int,
    }
)

TRACE_TOOL = Obj(
    {
        "role": OneOf("tool"),
        "tool_call_id": str,
        "name": str,
        "server": str,
        "content": str,
        "is_error": bool,
        "truncated": bool,
        "latency_ms": int,
    }
)


def check_trace(value, path: str = "$") -> None:
    assert isinstance(value, list), f"{path}: expected a list, got {value!r}"
    for i, m in enumerate(value):
        assert isinstance(m, dict) and m.get("role") in ("assistant", "tool"), f"{path}[{i}]: {m!r}"
        check(m, TRACE_ASSISTANT if m["role"] == "assistant" else TRACE_TOOL, f"{path}[{i}]")


def page(item: Any) -> Obj:
    return Obj({"items": ListOf(item), "total": int, "offset": int, "limit": int})


HEALTH = Obj({"ok": bool, "version": str})

ENDPOINT = Obj(
    {
        "id": int,
        "name": str,
        "base_url": str,
        "has_api_key": bool,
        "api_key_hint": Opt(str),
        "location": OneOf("local", "cloud"),
        "default_concurrency": int,
        "headers": MapOf(str),
        "models": ListOf(str),
        "last_checked_at": Opt(DT),
        "last_ok": Opt(bool),
        "last_error": Opt(str),
        "created_at": DT,
        "run_count": int,
    }
)

ENDPOINT_TEST = Obj({"ok": bool, "models": ListOf(str), "latency_ms": Opt(int), "error": Opt(str)})

ENDPOINT_PRESET = Obj(
    {
        "id": str,
        "name": str,
        "base_url": str,
        "location": OneOf("local", "cloud"),
        "default_concurrency": int,
        "needs_api_key": bool,
        "api_key_url": Opt(str),
        "headers": MapOf(str),
    }
)

PROMPT_SET = Obj(
    {
        "id": int,
        "name": str,
        "source_kind": OneOf("huggingface", "file", "paste"),
        "source": dict,
        "prompt_count": int,
        "created_at": DT,
        "run_count": int,
    },
    optional={
        "status": OneOf("importing", "ready", "failed"),
        "status_detail": Opt(str),
        "rows_read": int,
        "rows_total": Opt(int),
    },
)

PROMPT = Obj({"id": int, "idx": int, "messages": ListOf(MESSAGE), "meta": dict})

UPLOAD = Obj({"upload_id": str, "filename": str, "size": int, "detected_format": str})

COLUMN = Obj(
    {"name": str, "kind": OneOf("text", "messages", "number", "other"), "example": Opt(str)},
    optional={"looks_like_output": bool},
)

HF_SPLIT = Obj({"config": str, "split": str, "num_rows": Opt(int)})

IMPORT_PREVIEW = Obj(
    {
        "detected_format": str,
        "columns": ListOf(COLUMN),
        "prompt_field": Opt(str),
        "system_field": Opt(str),
        "split_mode": Opt(OneOf("auto", "line", "blank_line", "separator", "heading", "list_item", "whole")),
        "total_rows": Opt(int),
        "estimated_prompts": Opt(int),
        "prompts": ListOf(ListOf(MESSAGE)),
        "warnings": ListOf(str),
        "hf_splits": ListOf(HF_SPLIT),
        "hf_config": Opt(str),
        "hf_split": Opt(str),
    },
    optional={"has_conversations": bool, "dropped": ListOf(str)},
)

RUN_PARAMS = Obj(
    {
        "samples_per_prompt": int,
        "temperature": Opt(float),
        "top_p": Opt(float),
        "max_tokens": Opt(int),
        "max_tokens_field": OneOf("max_tokens", "max_completion_tokens"),
        "seed": Opt(int),
        "system_prompt": Opt(str),
        "user_template": Opt(str),
        "extra_body": dict,
        "stream": bool,
        "think_open": str,
        "think_close": str,
        "max_attempts": int,
        "request_timeout_s": float,
        "pause_after_failures": int,
        "tools": Opt(RUN_TOOLS),
    }
)

RUN = Obj(
    {
        "id": int,
        "name": str,
        "endpoint_id": Opt(int),
        "endpoint_name": str,
        "model": str,
        "prompt_set_id": int,
        "prompt_set_name": str,
        "params": RUN_PARAMS,
        "concurrency": int,
        "status": OneOf("running", "pausing", "paused", "completed"),
        "status_reason": Opt(str),
        "total": int,
        "done": int,
        "failed": int,
        "truncated": int,
        "pending": int,
        "prompt_tokens": int,
        "completion_tokens": int,
        "reasoning_tokens": int,
        "active_seconds": float,
        "created_at": DT,
        "started_at": Opt(DT),
        "finished_at": Opt(DT),
        "tools": Opt(TOOL_SNAPSHOT),
        "tool_calls": int,
    }
)

LIVE_SEQUENCE = Obj(
    {
        "sample_id": int,
        "prompt_idx": int,
        "sample_idx": int,
        "prompt_preview": str,
        "phase": OneOf("waiting", "thinking", "answering", "tool"),
        "tokens": int,
        "tail": str,
        "elapsed_ms": int,
        "attempt": int,
        "round": int,
        "tools": ListOf(str),
    }
)

METRIC_POINT = Obj(
    {
        "t": DT,
        "tokens_per_s": float,
        "samples_per_min": float,
        "active": int,
        "concurrency": int,
        "done": int,
        "failed": int,
    }
)

RUN_LIVE = Obj(
    {
        "run": RUN,
        "in_flight": int,
        "sequences": ListOf(LIVE_SEQUENCE),
        "tokens_per_s": float,
        "samples_per_min": float,
        "eta_seconds": Opt(float),
        "metrics": ListOf(METRIC_POINT),
    }
)

SAMPLE_STATUS = OneOf("pending", "running", "done", "failed")

SAMPLE = Obj(
    {
        "id": int,
        "run_id": int,
        "prompt_id": int,
        "prompt_idx": int,
        "sample_idx": int,
        "status": SAMPLE_STATUS,
        "reasoning": Opt(str),
        "response": Opt(str),
        "finish_reason": Opt(str),
        "prompt_tokens": Opt(int),
        "completion_tokens": Opt(int),
        "reasoning_tokens": Opt(int),
        "latency_ms": Opt(int),
        "ttft_ms": Opt(int),
        "attempts": int,
        "error": Opt(str),
        "completed_at": Opt(DT),
        "messages": ListOf(MESSAGE),
        "trace": Opt(list),
        "tool_calls": int,
    }
)

SAMPLE_SUMMARY = Obj(
    {
        "id": int,
        "prompt_idx": int,
        "sample_idx": int,
        "status": SAMPLE_STATUS,
        "prompt_preview": str,
        "response_preview": Opt(str),
        "has_reasoning": bool,
        "finish_reason": Opt(str),
        "completion_tokens": Opt(int),
        "latency_ms": Opt(int),
        "error": Opt(str),
        "tool_calls": int,
    }
)

EXPORT_PREVIEW = Obj(
    {"count": int, "skipped": MapOf(int), "example": Opt(dict), "filename": str}
)

HUB_PUSH = Obj({"url": str, "count": int})

RUN_REF = Obj({"id": int, "name": str, "model": str})

PREFERENCE_SET = Obj(
    {
        "id": int,
        "name": str,
        "prompt_set_id": int,
        "prompt_set_name": str,
        "run_ids": ListOf(int),
        "runs": ListOf(RUN_REF),
        "prompt_count": int,
        "decided": int,
        "created_at": DT,
    }
)

CANDIDATE = Obj(
    {
        "sample_id": int,
        "run_id": int,
        "run_name": str,
        "model": str,
        "sample_idx": int,
        "reasoning": Opt(str),
        "response": Opt(str),
        "finish_reason": Opt(str),
        "completion_tokens": Opt(int),
        "reasoning_tokens": Opt(int),
        "tool_calls": int,
        "trace": Opt(list),
    }
)

PREFERENCE_ITEM = Obj(
    {
        "prompt_id": int,
        "prompt_idx": int,
        "messages": ListOf(MESSAGE),
        "candidates": ListOf(CANDIDATE),
        "chosen_sample_id": Opt(int),
        "rejected_sample_id": Opt(int),
        "source": Opt(OneOf("manual", "rule")),
    }
)

AUTO_PAIR = Obj({"created": int, "skipped": int})

SETTINGS = Obj(
    {
        "hf_token_set": bool,
        "hf_token_hint": Opt(str),
        "hf_token_from_env": bool,
        "data_dir": str,
        "version": str,
    },
    optional={"mode": OneOf("docker", "desktop", "native"), "stdio_mcp_allowed": bool},
)

SECRET_HINT = Obj({"name": str, "hint": Opt(str)})

MCP_TOOL = Obj({"name": str, "description": Opt(str), "input_schema": dict})

MCP_SERVER = Obj(
    {
        "id": int,
        "name": str,
        "transport": MCP_TRANSPORT,
        "command": Opt(str),
        "args": ListOf(str),
        "env": ListOf(SECRET_HINT),
        "url": Opt(str),
        "headers": ListOf(SECRET_HINT),
        "enabled": bool,
        "status": OneOf("unchecked", "ok", "error"),
        "error": Opt(str),
        "checked_at": Opt(DT),
        "server_info": Opt(str),
        "tools": ListOf(MCP_TOOL),
        "blocked": Opt(str),
        "run_count": int,
        "created_at": DT,
        "updated_at": DT,
    }
)

MCP_IMPORT_NOTE = Obj({"entry": str, "message": str})

MCP_IMPORT = Obj(
    {"created": ListOf(MCP_SERVER), "errors": ListOf(MCP_IMPORT_NOTE), "notes": ListOf(MCP_IMPORT_NOTE)}
)
