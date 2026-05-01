"""Orchestrator agent toolset (ML Intern pattern).

Tools the *main* orchestrator can call. The research sub-agent has its own
toolset (see :mod:`app.agents.research_subagent`) that is NOT exposed here —
when the orchestrator calls ``research()`` it gets back a single structured
JSON recipe rather than seeing the sub-agent's intermediate steps.

Loop discipline (encoded in tool descriptions so the LLM sees it):
  1. Call ``research(...)`` before proposing hyperparameters.
  2. Call ``sandbox_create(...)`` for a small smoke test.
  3. Call ``read_alerts(sandbox_run_id)`` until the sandbox finishes.
  4. Call ``submit_preflight(...)`` to validate the scale plan.
  5. Call ``run_create(...)`` for the production run.

All non-research tools route through our own /v1 endpoints so server-side
ownership + audit + rate limits apply uniformly. The caller's bearer is
forwarded; it is never logged.

Backwards compatibility: the original tool names (``create_run``, etc.) are
preserved as aliases so existing transcripts still resolve.
"""

from __future__ import annotations

import json
import logging
from typing import Any

import httpx

from app.agents import research_subagent
from app.core.config import settings

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Tool definitions (OpenAI function-calling format)
# ---------------------------------------------------------------------------

_DATASET_MIXTURE_SCHEMA = {
    "type": "array",
    "items": {
        "type": "object",
        "required": ["name", "weight", "source"],
        "properties": {
            "name": {"type": "string"},
            "weight": {"type": "number"},
            "source": {"type": "string"},
        },
    },
}

_HYPERPARAMS_SCHEMA = {"type": "object"}

_PREFLIGHT_SCHEMA = {
    "type": "object",
    "required": [
        "model",
        "method",
        "dataset_mixture",
        "hyperparams",
        "sandbox_run_id",
        "sandbox_summary",
        "projected_cost_usd",
    ],
    "properties": {
        "model": {"type": "string"},
        "method": {"type": "string"},
        "dataset_mixture": _DATASET_MIXTURE_SCHEMA,
        "hyperparams": _HYPERPARAMS_SCHEMA,
        "sandbox_run_id": {"type": "string"},
        "sandbox_summary": {"type": "string"},
        "projected_cost_usd": {"type": "number"},
        "citations": {
            "type": "array",
            "items": {
                "type": "object",
                "required": ["source", "id", "title"],
                "properties": {
                    "source": {"type": "string"},
                    "id": {"type": "string"},
                    "title": {"type": "string"},
                    "note": {"type": "string"},
                },
            },
        },
    },
}


TOOLS: list[dict[str, Any]] = [
    {
        "type": "function",
        "function": {
            "name": "research",
            "description": (
                "Spawn the research sub-agent to extract methodology + "
                "hyperparams + working code from papers and reference repos. "
                "ALWAYS call this before proposing hyperparameters or a base "
                "model. Returns a structured JSON recipe."
            ),
            "parameters": {
                "type": "object",
                "required": ["task"],
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Concrete research question (e.g. 'best learning rate for GRPO on Llama-3-8B').",
                    },
                    "context": {
                        "type": "string",
                        "description": "Relevant orchestrator context to pass through.",
                        "default": "",
                    },
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "sandbox_create",
            "description": (
                "Create a small CPU/single-small-GPU smoke-test run "
                "(is_sandbox=True, bypasses preflight gate). Use this BEFORE "
                "any scale run to validate the recipe."
            ),
            "parameters": {
                "type": "object",
                "required": ["name", "base_model", "method", "dataset_mixture"],
                "properties": {
                    "name": {"type": "string"},
                    "base_model": {"type": "string"},
                    "method": {"type": "string", "enum": ["sft", "dpo", "grpo", "ppo", "rm"]},
                    "hyperparams": _HYPERPARAMS_SCHEMA,
                    "dataset_mixture": _DATASET_MIXTURE_SCHEMA,
                    "max_steps": {"type": "integer", "default": 50},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "read_alerts",
            "description": (
                "Read alerts emitted by a run's training script (trackio.alert). "
                "Poll this each loop iteration; route ERROR -> re-research, "
                "WARN -> tweak hyperparams, INFO -> milestone."
            ),
            "parameters": {
                "type": "object",
                "required": ["run_id"],
                "properties": {
                    "run_id": {"type": "string"},
                    "since": {"type": "string", "description": "ISO-8601 lower bound"},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "read_run",
            "description": "Get a run's status, last metrics, and notes.",
            "parameters": {
                "type": "object",
                "required": ["run_id"],
                "properties": {"run_id": {"type": "string"}},
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "submit_preflight",
            "description": (
                "Validate a preflight plan WITHOUT creating a run. Required "
                "before run_create() at scale. Returns ok or specific schema "
                "errors."
            ),
            "parameters": {
                "type": "object",
                "required": ["preflight_json"],
                "properties": {
                    "run_id_planned_name": {"type": "string"},
                    "preflight_json": _PREFLIGHT_SCHEMA,
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "run_create",
            "description": (
                "Create a full scale run. Server re-validates preflight + "
                "sandbox lineage; rejects with 412 if absent or stale."
            ),
            "parameters": {
                "type": "object",
                "required": [
                    "name",
                    "base_model",
                    "method",
                    "dataset_mixture",
                    "sandbox_run_id",
                    "preflight_json",
                ],
                "properties": {
                    "name": {"type": "string"},
                    "base_model": {"type": "string"},
                    "method": {"type": "string", "enum": ["sft", "dpo", "grpo", "ppo", "rm"]},
                    "hyperparams": _HYPERPARAMS_SCHEMA,
                    "dataset_mixture": _DATASET_MIXTURE_SCHEMA,
                    "gpu_type": {"type": "string"},
                    "gpu_count": {"type": "integer"},
                    "sandbox_run_id": {"type": "string"},
                    "preflight_json": _PREFLIGHT_SCHEMA,
                    "user_goal": {"type": "string"},
                    "agent_plan": {"type": "string"},
                    "citations": {"type": "array", "items": {"type": "object"}},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "cancel_run",
            "description": "Cancel a queued or running run.",
            "parameters": {
                "type": "object",
                "required": ["run_id"],
                "properties": {"run_id": {"type": "string"}},
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "pause_run",
            "description": "Pause a running run.",
            "parameters": {
                "type": "object",
                "required": ["run_id"],
                "properties": {"run_id": {"type": "string"}},
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "resume_run",
            "description": "Resume a paused run.",
            "parameters": {
                "type": "object",
                "required": ["run_id"],
                "properties": {"run_id": {"type": "string"}},
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "add_note",
            "description": "Append a note to a run (plan|progress|warning|result).",
            "parameters": {
                "type": "object",
                "required": ["run_id", "kind", "body"],
                "properties": {
                    "run_id": {"type": "string"},
                    "kind": {
                        "type": "string",
                        "enum": ["plan", "progress", "warning", "result"],
                    },
                    "body": {"type": "string"},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "cite_paper",
            "description": "Attach a paper citation to a run.",
            "parameters": {
                "type": "object",
                "required": ["run_id", "source", "paper_id"],
                "properties": {
                    "run_id": {"type": "string"},
                    "source": {"type": "string", "enum": ["hf", "arxiv"]},
                    "paper_id": {"type": "string"},
                    "note": {"type": "string"},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "get_checkpoint",
            "description": (
                "Get the trained weights URL for a completed run. Returns null until "
                "the run reaches succeeded status. Use this to pass weights to a "
                "downstream eval script or to a promotion run."
            ),
            "parameters": {
                "type": "object",
                "required": ["run_id"],
                "properties": {
                    "run_id": {"type": "string"},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "pick_environment",
            "description": (
                "Look up a recipe scaffold for a known RL/eval task. Returns the "
                "recommended method, dataset mixture, hyperparams, and eval metric. "
                "The agent can use this as a starting point for sandbox_create. "
                "Known IDs: gsm8k, math, humaneval, mbpp, mt-bench, alpaca-eval, "
                "truthfulqa, hellaswag."
            ),
            "parameters": {
                "type": "object",
                "required": ["env_id"],
                "properties": {
                    "env_id": {
                        "type": "string",
                        "description": (
                            "Environment identifier, e.g. 'gsm8k', 'humaneval', "
                            "'mt-bench'."
                        ),
                    },
                },
            },
        },
    },
]


# ---------------------------------------------------------------------------
# Backwards-compat aliases (transcripts pre-refactor still resolve)
# ---------------------------------------------------------------------------

_LEGACY_ALIASES: dict[str, str] = {
    "create_run": "run_create",
    "stellarator_create_run": "run_create",
    "stellarator_cancel_run": "cancel_run",
    "stellarator_add_note": "add_note",
    "search_papers": "research",
    "get_run": "read_run",
    "list_runs": "read_run",
}


# ---------------------------------------------------------------------------
# Dispatcher
# ---------------------------------------------------------------------------


def _internal_base() -> str:
    return settings.stellarator_internal_base_url


def _resolve_name(name: str) -> str:
    return _LEGACY_ALIASES.get(name, name)


async def execute(
    name: str,
    args: dict[str, Any],
    agent_token: str,
    *,
    agent: str | None = None,
) -> str:
    """Dispatch a tool call. Always returns a JSON string."""
    name = _resolve_name(name)
    headers = {"Authorization": f"Bearer {agent_token}"}

    if name == "research":
        try:
            recipe = await research_subagent.research(
                task=args.get("task", ""),
                context=args.get("context", ""),
                agent=agent or "unknown",
            )
        except (httpx.HTTPError, ValueError, KeyError, RuntimeError) as exc:
            logger.exception("research sub-agent failed")
            return json.dumps({"error": f"{type(exc).__name__}: {exc}"})
        return json.dumps(recipe)

    async with httpx.AsyncClient(
        base_url=_internal_base(), headers=headers, timeout=60.0
    ) as client:
        if name == "sandbox_create":
            r = await client.post("/v1/runs/", json=_sandbox_payload(args))
            return _result(r)

        if name == "read_alerts":
            params: dict[str, Any] = {}
            if args.get("since"):
                params["since"] = args["since"]
            r = await client.get(f"/v1/runs/{args['run_id']}/alerts", params=params)
            return _result(r)

        if name == "read_run":
            r = await client.get(f"/v1/runs/{args['run_id']}")
            return _result(r)

        if name == "submit_preflight":
            payload = {
                "preflight_json": args.get("preflight_json"),
                "planned_name": args.get("run_id_planned_name", ""),
            }
            r = await client.post("/v1/runs/preflight/validate", json=payload)
            return _result(r)

        if name == "run_create":
            r = await client.post("/v1/runs/", json=_scale_payload(args))
            return _result(r)

        if name == "cancel_run":
            r = await client.post(f"/v1/runs/{args['run_id']}/cancel")
            return _result(r)

        if name == "pause_run":
            r = await client.post(f"/v1/runs/{args['run_id']}/pause")
            return _result(r)

        if name == "resume_run":
            r = await client.post(f"/v1/runs/{args['run_id']}/resume")
            return _result(r)

        if name == "add_note":
            r = await client.post(
                f"/v1/runs/{args['run_id']}/notes",
                json={"kind": args["kind"], "body": args["body"]},
            )
            return _result(r)

        if name == "cite_paper":
            run_id = args["run_id"]
            r = await client.post(
                f"/v1/research/runs/{run_id}/cite",
                json={
                    "source": args.get("source"),
                    "paper_id": args.get("paper_id"),
                    "note": args.get("note", ""),
                },
            )
            return _result(r)

        if name == "get_checkpoint":
            r = await client.get(f"/v1/runs/{args['run_id']}")
            if r.status_code >= 400:
                return _result(r)
            data = r.json()
            run_status = data.get("status", "")
            ready = run_status == "succeeded"
            url = data.get("checkpoint_url") if ready else None
            return json.dumps({
                "run_id": args["run_id"],
                "status": run_status,
                "checkpoint_url": url,
                "ready": ready,
            })

        if name == "pick_environment":
            from app.services.environments import pick_environment as _pick_env
            env = _pick_env(args.get("env_id", ""))
            if env is None:
                return json.dumps({"error": f"Unknown environment: {args.get('env_id')!r}"})
            return json.dumps(env)

    return json.dumps({"error": f"Unknown tool: {name}"})


def _result(r: httpx.Response) -> str:
    if r.status_code >= 400:
        try:
            return json.dumps({"status": r.status_code, "error": r.json()})
        except ValueError:
            return json.dumps({"status": r.status_code, "error": r.text[:500]})
    return r.text


def _sandbox_payload(args: dict[str, Any]) -> dict[str, Any]:
    hp = dict(args.get("hyperparams") or {})
    hp.setdefault("max_steps", args.get("max_steps", 50))
    return {
        "name": args["name"],
        "base_model": args["base_model"],
        "method": args["method"],
        "hyperparams": hp,
        "dataset_mixture": args.get("dataset_mixture") or [],
        "gpu_type": "cpu",
        "gpu_count": 1,
        "is_sandbox": True,
        "user_goal": args.get("user_goal", ""),
        "agent_plan": args.get("agent_plan", "sandbox smoke test"),
    }


def _scale_payload(args: dict[str, Any]) -> dict[str, Any]:
    return {
        "name": args["name"],
        "base_model": args["base_model"],
        "method": args["method"],
        "hyperparams": args.get("hyperparams") or {},
        "dataset_mixture": args.get("dataset_mixture") or [],
        "gpu_type": args.get("gpu_type", "H100"),
        "gpu_count": args.get("gpu_count", 1),
        "is_sandbox": False,
        "preflight_json": args.get("preflight_json"),
        "sandbox_run_id": args.get("sandbox_run_id"),
        "user_goal": args.get("user_goal", ""),
        "agent_plan": args.get("agent_plan", ""),
        "citations": args.get("citations") or [],
    }


__all__ = ["TOOLS", "execute"]
