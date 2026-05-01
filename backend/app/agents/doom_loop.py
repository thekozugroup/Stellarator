"""Doom-loop detector for agent tool-call histories.

If the same tool is called with identical arguments three or more times
inside the recent window, the loop is unproductive (the result obviously
has not changed the agent's plan). The driver injects a corrective system
note nudging a different approach.

The detector is purely structural — it does not look at tool *results*,
because for some tools (read_alerts polling an empty stream) "no change"
is the actual signal we care about catching.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any, Iterable

DEFAULT_WINDOW = 8
DEFAULT_THRESHOLD = 3


@dataclass(frozen=True)
class ToolCall:
    name: str
    args: dict[str, Any]


# Suggestion table — keyed by tool name. Order matches the spec.
_SUGGESTIONS: dict[str, str] = {
    "research": (
        "drill deeper; call hf_paper_citation_graph for the most relevant paper "
        "or pivot the query to a related sub-topic."
    ),
    "read_alerts": (
        "no new alerts arrived; either the job is healthy or stalled — call "
        "read_run to inspect status/last metric instead of polling alerts."
    ),
    "run_create": (
        "the same scale run keeps getting rejected; fix the specific preflight "
        "field the server cited rather than re-submitting the same payload."
    ),
    "submit_preflight": (
        "preflight keeps failing; re-read the validation error and modify "
        "exactly that field — do not retry the same shape."
    ),
    "sandbox_create": (
        "sandbox already exists; call read_run on the existing sandbox_run_id "
        "and proceed to submit_preflight when it succeeds."
    ),
}

_DEFAULT_SUGGESTION = (
    "you are repeating the same call with identical arguments; either change "
    "arguments, switch tool, or stop and produce a final answer."
)


def _canonical_args(args: dict[str, Any]) -> str:
    """Stable JSON for equality comparison."""
    try:
        return json.dumps(args, sort_keys=True, default=str)
    except (TypeError, ValueError):
        return repr(sorted(args.items()))


def detect(
    history: Iterable[ToolCall],
    *,
    window: int = DEFAULT_WINDOW,
    threshold: int = DEFAULT_THRESHOLD,
) -> str | None:
    """Return a corrective system message if a doom loop is detected, else None.

    ``history`` is the raw tool-call list (oldest -> newest). Only the last
    ``window`` entries are inspected.
    """
    recent = list(history)[-window:]
    if len(recent) < threshold:
        return None

    counts: dict[tuple[str, str], int] = {}
    for call in recent:
        key = (call.name, _canonical_args(call.args))
        counts[key] = counts.get(key, 0) + 1
        if counts[key] >= threshold:
            suggestion = _SUGGESTIONS.get(call.name, _DEFAULT_SUGGESTION)
            return (
                f"DOOM LOOP DETECTED: you have called `{call.name}` with the "
                f"same arguments {counts[key]} times in the last {window} tool "
                f"calls. Try a different approach: {suggestion}"
            )
    return None


def extract_history_from_messages(messages: list[dict[str, Any]]) -> list[ToolCall]:
    """Walk an OpenAI-format message list, returning every tool call observed."""
    out: list[ToolCall] = []
    for msg in messages:
        if msg.get("role") != "assistant":
            continue
        for tc in msg.get("tool_calls") or []:
            fn = tc.get("function", {})
            name = fn.get("name") or ""
            raw_args = fn.get("arguments") or "{}"
            try:
                args = json.loads(raw_args) if isinstance(raw_args, str) else raw_args
            except json.JSONDecodeError:
                args = {"_raw": str(raw_args)}
            if not isinstance(args, dict):
                args = {"_value": args}
            out.append(ToolCall(name=name, args=args))
    return out
