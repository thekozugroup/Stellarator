"""Research sub-agent (HF ML Intern pattern).

The orchestrator agent calls ``research(task, context)`` as a single tool;
internally this spins a parallel mini-loop that has access to a *separate*
toolset (HF papers + arXiv + GitHub Code Search + raw GitHub file read)
and returns a structured JSON recipe to the orchestrator.

The orchestrator never sees the sub-agent's intermediate tool calls — those
are persisted to ``research_transcripts`` for audit and surfaced in the
chat UI as a single collapsed "Research" card.

Driver selection is read from ``CALLING_DRIVER`` ContextVar so each
orchestrator path (openai / codex / openrouter) can route the sub-agent
through the same upstream credential it is already authenticated for.
"""

from __future__ import annotations

import asyncio
import contextvars
import json
import logging
from datetime import datetime
from typing import Any, Awaitable, Callable, Protocol

import httpx

from app.core.config import settings
from app.core.db import SessionLocal
from app.models.alert import ResearchTranscript
from app.services.github import github
from app.services.research import ArxivClient, HFPapersClient

logger = logging.getLogger(__name__)

# Maximum mini-loop iterations. Hugging Face's ML Intern caps at ~6.
MAX_SUBAGENT_ITER = 6


class _LLMCallable(Protocol):
    """Minimal interface the sub-agent needs from a driver."""

    async def __call__(
        self, messages: list[dict[str, Any]], tools: list[dict[str, Any]]
    ) -> dict[str, Any]:
        ...


# ContextVar lets each in-flight orchestrator request inject its own driver
# without thread-locals. Defaults to None so unit tests can patch in a stub.
CALLING_DRIVER: contextvars.ContextVar[_LLMCallable | None] = contextvars.ContextVar(
    "research_subagent_driver", default=None
)


_SYSTEM_PROMPT = """\
You are a research sub-agent for a fine-tuning orchestrator.
Goal: extract methodology + hyperparameters + working code snippets from
papers and reference repos for the task you are given.

You have these tools (NOT exposed to the parent):
  hf_papers_search(query, limit)
  hf_paper_read(arxiv_id)
  hf_paper_citation_graph(arxiv_id, depth)
  arxiv_search(query, limit)
  github_find_examples(query, lang)
  github_read_file(repo, path, ref)

You have at most {max_iter} tool-call iterations. Be parsimonious.

When you are done, return ONLY a JSON object (no prose) matching:
{{
  "summary": "<2-3 sentences>",
  "datasets": [{{"name": "...", "weight": 1.0, "source": "hf|other"}}],
  "hyperparams": {{"lr": 0.0, "batch_size": 0, ...}},
  "code_snippets": [{{"lang": "python", "source": "<repo:path>", "body": "..."}}],
  "citations": [{{"source": "hf|arxiv|github|web", "id": "...", "title": "...",
                  "url": "...", "relevance_score": 0.0}}]
}}
"""


# ---------------------------------------------------------------------------
# Sub-agent tool surface (NOT exposed to the orchestrator)
# ---------------------------------------------------------------------------

_hf_client = HFPapersClient()
_arxiv_client = ArxivClient()


async def _hf_citation_graph(arxiv_id: str, depth: int = 1) -> dict[str, Any]:
    """Walk the HF papers citation graph one or two hops.

    HF exposes ``/api/papers/{id}`` whose payload includes ``citing`` /
    ``cited_by`` arrays. We surface the immediate neighborhood; for
    depth > 1 we recurse but cap at 12 nodes to bound cost.
    """
    seen: dict[str, dict[str, Any]] = {}
    frontier: list[str] = [arxiv_id]
    timeout = httpx.Timeout(connect=5.0, read=10.0, write=5.0, pool=5.0)

    async with httpx.AsyncClient(timeout=timeout) as client:
        for hop in range(max(1, min(depth, 2))):
            next_frontier: list[str] = []
            for node in frontier:
                if node in seen or len(seen) >= 12:
                    continue
                try:
                    r = await client.get(f"https://huggingface.co/api/papers/{node}")
                except httpx.HTTPError as exc:
                    logger.debug("hf citation graph transport: %s", exc)
                    continue
                if r.status_code >= 400:
                    continue
                try:
                    payload = r.json()
                except ValueError:
                    continue
                seen[node] = {
                    "id": node,
                    "title": payload.get("title", ""),
                    "citing": [c.get("id") for c in payload.get("citing", []) if c.get("id")],
                    "cited_by": [
                        c.get("id") for c in payload.get("cited_by", []) if c.get("id")
                    ],
                }
                next_frontier.extend(seen[node]["citing"])
                next_frontier.extend(seen[node]["cited_by"])
            frontier = next_frontier
            if hop + 1 >= depth:
                break

    return {"root": arxiv_id, "nodes": list(seen.values())}


_SUBAGENT_TOOLS: list[dict[str, Any]] = [
    {
        "type": "function",
        "function": {
            "name": "hf_papers_search",
            "description": "Search Hugging Face papers for a query.",
            "parameters": {
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "default": 5},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "hf_paper_read",
            "description": "Fetch a single Hugging Face paper (full abstract).",
            "parameters": {
                "type": "object",
                "required": ["arxiv_id"],
                "properties": {"arxiv_id": {"type": "string"}},
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "hf_paper_citation_graph",
            "description": "Traverse the HF papers citation graph from a seed paper.",
            "parameters": {
                "type": "object",
                "required": ["arxiv_id"],
                "properties": {
                    "arxiv_id": {"type": "string"},
                    "depth": {"type": "integer", "default": 1, "minimum": 1, "maximum": 2},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "arxiv_search",
            "description": "Search arXiv directly.",
            "parameters": {
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "default": 5},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "github_find_examples",
            "description": "Search public GitHub code for working training examples.",
            "parameters": {
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {"type": "string"},
                    "lang": {"type": "string", "default": "python"},
                    "limit": {"type": "integer", "default": 5},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "github_read_file",
            "description": "Read a single file from a public GitHub repo.",
            "parameters": {
                "type": "object",
                "required": ["repo", "path"],
                "properties": {
                    "repo": {"type": "string"},
                    "path": {"type": "string"},
                    "ref": {"type": "string", "default": "main"},
                },
            },
        },
    },
]


_SubagentTool = Callable[..., Awaitable[Any]]


def _subagent_dispatch_table() -> dict[str, _SubagentTool]:
    return {
        "hf_papers_search": lambda **kw: _hf_client.search(
            kw["query"], kw.get("limit", 5)
        ),
        "hf_paper_read": lambda **kw: _hf_client.fetch(kw["arxiv_id"]),
        "hf_paper_citation_graph": lambda **kw: _hf_citation_graph(
            kw["arxiv_id"], kw.get("depth", 1)
        ),
        "arxiv_search": lambda **kw: _arxiv_client.search(
            kw["query"], kw.get("limit", 5)
        ),
        "github_find_examples": lambda **kw: github.find_examples(
            kw["query"], kw.get("lang", "python"), kw.get("limit", 5)
        ),
        "github_read_file": lambda **kw: github.read_file(
            kw["repo"], kw["path"], kw.get("ref", "main")
        ),
    }


# ---------------------------------------------------------------------------
# Result schema enforcement
# ---------------------------------------------------------------------------

_REQUIRED_KEYS = ("summary", "datasets", "hyperparams", "code_snippets", "citations")


def _coerce_result(raw: Any) -> dict[str, Any]:
    """Best-effort coercion of the LLM's final message into the schema."""
    if isinstance(raw, str):
        try:
            raw = json.loads(raw)
        except json.JSONDecodeError:
            raw = {"summary": raw[:1000]}
    if not isinstance(raw, dict):
        raw = {"summary": str(raw)[:1000]}

    out: dict[str, Any] = {
        "summary": str(raw.get("summary", ""))[:4000],
        "datasets": list(raw.get("datasets") or []),
        "hyperparams": dict(raw.get("hyperparams") or {}),
        "code_snippets": list(raw.get("code_snippets") or []),
        "citations": list(raw.get("citations") or []),
    }
    return out


# ---------------------------------------------------------------------------
# Public entry point
# ---------------------------------------------------------------------------


async def research(
    task: str,
    context: str,
    *,
    agent: str,
    run_id: str | None = None,
    driver: _LLMCallable | None = None,
) -> dict[str, Any]:
    """Run the research sub-agent and return the structured JSON recipe.

    On any unrecoverable error returns a minimal valid result rather than
    raising — the orchestrator should be able to keep planning.
    """
    started_at = datetime.utcnow()
    transcript: list[dict[str, Any]] = []

    llm = driver or CALLING_DRIVER.get()
    if llm is None:
        result = _coerce_result(
            {"summary": "research sub-agent unavailable: no driver configured"}
        )
        await _persist(run_id, agent, task, context, result, started_at, transcript)
        return result

    system_prompt = _SYSTEM_PROMPT.format(max_iter=MAX_SUBAGENT_ITER)
    messages: list[dict[str, Any]] = [
        {"role": "system", "content": system_prompt},
        {
            "role": "user",
            "content": f"Task: {task}\n\nOrchestrator context:\n{context}",
        },
    ]
    transcript.append({"role": "user", "task": task, "context": context[:500]})

    dispatch = _subagent_dispatch_table()
    final_payload: Any = ""

    for _ in range(MAX_SUBAGENT_ITER):
        try:
            response = await llm(messages=messages, tools=_SUBAGENT_TOOLS)
        except (httpx.HTTPError, RuntimeError) as exc:
            logger.warning("subagent driver failure: %s", exc)
            final_payload = {"summary": f"sub-agent driver error: {type(exc).__name__}"}
            break

        choice = response.get("choices", [{}])[0]
        msg = choice.get("message", {})
        tool_calls = msg.get("tool_calls") or []
        content = msg.get("content")

        # No tool calls -> final answer.
        if not tool_calls:
            final_payload = content or ""
            transcript.append({"role": "assistant", "content": str(content)[:500]})
            break

        messages.append(
            {
                "role": "assistant",
                "content": content,
                "tool_calls": tool_calls,
            }
        )

        # Independent tool calls -> dispatch in parallel.
        async def _run_call(tc: dict[str, Any]) -> tuple[dict[str, Any], str]:
            fn = tc.get("function", {})
            name = fn.get("name", "")
            try:
                args = json.loads(fn.get("arguments") or "{}")
            except json.JSONDecodeError:
                args = {}
            handler = dispatch.get(name)
            if handler is None:
                return tc, json.dumps({"error": f"unknown tool {name}"})
            try:
                result = await handler(**args)
            except (httpx.HTTPError, ValueError, KeyError, TypeError) as exc:
                return tc, json.dumps({"error": f"{type(exc).__name__}: {exc}"})
            try:
                serialized = json.dumps(result, default=str)
            except (TypeError, ValueError):
                serialized = json.dumps({"error": "unserializable_result"})
            return tc, serialized

        results = await asyncio.gather(
            *(_run_call(tc) for tc in tool_calls), return_exceptions=False
        )
        for tc, serialized in results:
            transcript.append(
                {
                    "role": "tool",
                    "name": tc.get("function", {}).get("name", ""),
                    "result_preview": serialized[:300],
                }
            )
            messages.append(
                {
                    "role": "tool",
                    "tool_call_id": tc.get("id", ""),
                    "content": serialized,
                }
            )

    result = _coerce_result(final_payload)
    await _persist(run_id, agent, task, context, result, started_at, transcript)
    return result


async def _persist(
    run_id: str | None,
    agent: str,
    task: str,
    context: str,
    result: dict[str, Any],
    started_at: datetime,
    transcript: list[dict[str, Any]],
) -> None:
    """Store the transcript for audit. Best-effort; never raises."""
    try:
        async with SessionLocal() as session:
            row = ResearchTranscript(
                run_id=run_id,
                calling_agent=agent[:64],
                task=task[:8000],
                context=context[:8000],
                result_json={"result": result, "transcript": transcript[:100]},
                started_at=started_at,
                completed_at=datetime.utcnow(),
            )
            session.add(row)
            await session.commit()
    except Exception:  # noqa: BLE001  -- audit-only, never block the agent
        logger.exception("research transcript persist failed (run_id=%s)", run_id)


__all__ = [
    "CALLING_DRIVER",
    "MAX_SUBAGENT_ITER",
    "research",
    "_REQUIRED_KEYS",
]
