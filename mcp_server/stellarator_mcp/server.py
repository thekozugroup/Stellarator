"""Stellarator MCP server.

Exposes 10 tools that let Claude Code manage Tinker fine-tuning runs and
look up research papers on the Stellarator backend.

Run via:
    stellarator-mcp          # stdio transport (used by Claude Code)

Required env:
    STELLARATOR_TOKEN        # Bearer token for /v1 endpoints
    STELLARATOR_BASE_URL     # Optional override (default: http://localhost:8000)
"""

from __future__ import annotations

import asyncio
import json
import logging
from typing import Any

from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.types import Tool, TextContent, CallToolResult

from stellarator_mcp.client import StellaratorClient, StellaratorAPIError

logging.basicConfig(level=logging.WARNING)
logger = logging.getLogger(__name__)

app = Server("stellarator")
_client = StellaratorClient()


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _ok(data: Any) -> CallToolResult:
    return CallToolResult(
        content=[TextContent(type="text", text=json.dumps(data, indent=2))],
        isError=False,
    )


def _err(msg: str) -> CallToolResult:
    return CallToolResult(
        content=[TextContent(type="text", text=msg)],
        isError=True,
    )


def _tool_error(exc: Exception) -> CallToolResult:
    if isinstance(exc, StellaratorAPIError):
        return _err(str(exc))
    return _err(f"Unexpected error: {exc!r}")


# ---------------------------------------------------------------------------
# Tool registry
# ---------------------------------------------------------------------------

TOOLS: list[Tool] = [
    Tool(
        name="stellarator_create_run",
        description=(
            "Create a new Tinker fine-tuning run on Stellarator. "
            "Supply at minimum `name`, `base_model`, `method`, and `dataset_mixture`. "
            "The caller agent identity (claude-code) is attached automatically via the "
            "bearer token — do NOT pass an `owner` field. "
            "Returns the full run object including the assigned `id` you will need for "
            "subsequent tool calls."
        ),
        inputSchema={
            "type": "object",
            "required": ["name", "base_model", "method", "dataset_mixture"],
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Human-readable name for this run, e.g. 'llama3-lora-v2'.",
                },
                "base_model": {
                    "type": "string",
                    "description": (
                        "HuggingFace model ID or local path, "
                        "e.g. 'meta-llama/Meta-Llama-3-8B'."
                    ),
                },
                "method": {
                    "type": "string",
                    "enum": ["lora", "qlora", "full", "dpo", "orpo", "sft"],
                    "description": "Fine-tuning method to use.",
                },
                "hyperparams": {
                    "type": "object",
                    "description": (
                        "Key/value hyperparameters passed to the training script, "
                        "e.g. {\"lr\": 2e-4, \"epochs\": 3, \"batch_size\": 4}."
                    ),
                },
                "dataset_mixture": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": (
                        "List of dataset identifiers to mix, "
                        "e.g. [\"tatsu-lab/alpaca\", \"HuggingFaceH4/ultrachat_200k\"]."
                    ),
                },
                "gpu_type": {
                    "type": "string",
                    "description": "GPU SKU, e.g. 'A100-80GB' or 'H100-SXM'.",
                },
                "gpu_count": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Number of GPUs to allocate.",
                },
                "user_goal": {
                    "type": "string",
                    "description": "High-level goal the end-user stated, verbatim.",
                },
                "user_context": {
                    "type": "string",
                    "description": "Additional context the end-user provided.",
                },
                "agent_plan": {
                    "type": "string",
                    "description": (
                        "The agent's reasoning and plan for this run, "
                        "stored for audit purposes."
                    ),
                },
                "citations": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Paper IDs or URLs that informed this run configuration.",
                },
            },
        },
    ),
    Tool(
        name="stellarator_list_runs",
        description=(
            "List fine-tuning runs visible to the current agent. "
            "Filter by owner (agent identity string), status, or both. "
            "Use `owner='claude-code'` to see only runs created by this agent. "
            "Returns an array of run summary objects sorted newest-first."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "owner": {
                    "type": "string",
                    "description": (
                        "Filter by agent identity, e.g. 'claude-code'. "
                        "Omit to list runs from all agents."
                    ),
                },
                "status": {
                    "type": "string",
                    "enum": [
                        "queued", "running", "paused",
                        "completed", "failed", "cancelled",
                    ],
                    "description": "Filter by run status.",
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 200,
                    "description": "Maximum number of runs to return (default: 20).",
                },
            },
        },
    ),
    Tool(
        name="stellarator_get_run",
        description=(
            "Fetch full details for a single run including its notes and recent "
            "training metrics (loss, eval scores, throughput). "
            "Use this to monitor progress or inspect a completed run before citing it."
        ),
        inputSchema={
            "type": "object",
            "required": ["run_id"],
            "properties": {
                "run_id": {
                    "type": "string",
                    "description": "Run UUID returned by stellarator_create_run or stellarator_list_runs.",
                },
            },
        },
    ),
    Tool(
        name="stellarator_cancel_run",
        description=(
            "Cancel a queued or running fine-tuning job immediately. "
            "Only the owning agent (the one whose token created the run) can cancel it; "
            "a 403 means another agent owns the run. "
            "Cancelled runs cannot be resumed — use stellarator_pause_run if you may want "
            "to continue later."
        ),
        inputSchema={
            "type": "object",
            "required": ["run_id"],
            "properties": {
                "run_id": {"type": "string", "description": "Run UUID to cancel."},
            },
        },
    ),
    Tool(
        name="stellarator_pause_run",
        description=(
            "Pause a currently running fine-tuning job, checkpointing it so it can "
            "be resumed later with stellarator_resume_run. "
            "Only the owning agent can pause a run (403 = owned by another agent). "
            "Pausing is idempotent — calling it on an already-paused run is safe."
        ),
        inputSchema={
            "type": "object",
            "required": ["run_id"],
            "properties": {
                "run_id": {"type": "string", "description": "Run UUID to pause."},
            },
        },
    ),
    Tool(
        name="stellarator_resume_run",
        description=(
            "Resume a previously paused fine-tuning run from its last checkpoint. "
            "Only the owning agent can resume a run (403 = owned by another agent). "
            "The run must be in 'paused' status; resuming a completed or failed run "
            "returns an error."
        ),
        inputSchema={
            "type": "object",
            "required": ["run_id"],
            "properties": {
                "run_id": {"type": "string", "description": "Run UUID to resume."},
            },
        },
    ),
    Tool(
        name="stellarator_add_note",
        description=(
            "Attach a structured note to a run. Use this to record observations, "
            "hypotheses, intermediate findings, or decisions during an experiment. "
            "Notes are stored in chronological order and returned by stellarator_get_run. "
            "`kind` controls how the UI renders the note."
        ),
        inputSchema={
            "type": "object",
            "required": ["run_id", "kind", "body"],
            "properties": {
                "run_id": {
                    "type": "string",
                    "description": "Run UUID to attach the note to.",
                },
                "kind": {
                    "type": "string",
                    "enum": ["observation", "hypothesis", "decision", "warning", "info"],
                    "description": "Semantic category of the note.",
                },
                "body": {
                    "type": "string",
                    "description": "Markdown-formatted note content.",
                },
            },
        },
    ),
    Tool(
        name="stellarator_search_papers",
        description=(
            "Search for ML research papers relevant to the current experiment. "
            "Queries HuggingFace Papers, arXiv, or both simultaneously. "
            "Returns paper metadata (title, abstract excerpt, authors, date, source ID). "
            "Use the returned `source` + `paper_id` fields with stellarator_get_paper "
            "or stellarator_cite_paper."
        ),
        inputSchema={
            "type": "object",
            "required": ["q"],
            "properties": {
                "q": {
                    "type": "string",
                    "description": (
                        "Full-text search query, e.g. "
                        "'LoRA fine-tuning rank selection' or 'DPO reward hacking'."
                    ),
                },
                "source": {
                    "type": "string",
                    "enum": ["hf", "arxiv", "both"],
                    "description": (
                        "Which paper index to search: 'hf' (HuggingFace Papers), "
                        "'arxiv', or 'both' (default)."
                    ),
                },
            },
        },
    ),
    Tool(
        name="stellarator_get_paper",
        description=(
            "Fetch full metadata and abstract for a specific research paper. "
            "`source` and `paper_id` come from the search results returned by "
            "stellarator_search_papers. "
            "Use this to read the abstract before deciding whether to cite a paper."
        ),
        inputSchema={
            "type": "object",
            "required": ["source", "paper_id"],
            "properties": {
                "source": {
                    "type": "string",
                    "enum": ["hf", "arxiv"],
                    "description": "Paper index the ID belongs to.",
                },
                "paper_id": {
                    "type": "string",
                    "description": (
                        "Paper identifier from the source index, "
                        "e.g. '2106.09685' for arXiv or a HF Papers slug."
                    ),
                },
            },
        },
    ),
    Tool(
        name="stellarator_cite_paper",
        description=(
            "Attach a research paper citation to a fine-tuning run, recording why "
            "this paper informed the run's design. Citations appear in the run's audit "
            "trail. Cite papers before or after creating the run — citations can be "
            "added at any time while the run exists. "
            "A free-text `note` explains the paper's relevance to this specific run."
        ),
        inputSchema={
            "type": "object",
            "required": ["run_id", "source", "paper_id", "note"],
            "properties": {
                "run_id": {
                    "type": "string",
                    "description": "Run UUID to attach the citation to.",
                },
                "source": {
                    "type": "string",
                    "enum": ["hf", "arxiv"],
                    "description": "Paper index the ID belongs to.",
                },
                "paper_id": {
                    "type": "string",
                    "description": "Paper identifier from the source index.",
                },
                "note": {
                    "type": "string",
                    "description": (
                        "1–3 sentences explaining why this paper is relevant to the run, "
                        "e.g. which hyperparameter choice or architecture decision it supports."
                    ),
                },
            },
        },
    ),
    Tool(
        name="stellarator_get_checkpoint",
        description=(
            "Get the trained weights URL for a completed run. "
            "Returns null for checkpoint_url until the run reaches succeeded status. "
            "Use this to pass weights to a downstream eval script or to a promotion run. "
            "Poll this tool after run completion before launching an evaluation job."
        ),
        inputSchema={
            "type": "object",
            "required": ["run_id"],
            "properties": {
                "run_id": {
                    "type": "string",
                    "description": "Run UUID to fetch the checkpoint for.",
                },
            },
        },
    ),
    Tool(
        name="stellarator_pick_environment",
        description=(
            "Look up a recipe scaffold for a known RL/eval task. "
            "Returns the recommended method (sft/dpo/grpo/ppo/rm), dataset mixture, "
            "hyperparameters, and eval metric for the requested environment. "
            "Use the returned scaffold as a starting point for sandbox_create. "
            "Known environment IDs: gsm8k, math, humaneval, mbpp, mt-bench, "
            "alpaca-eval, truthfulqa, hellaswag."
        ),
        inputSchema={
            "type": "object",
            "required": ["env_id"],
            "properties": {
                "env_id": {
                    "type": "string",
                    "description": (
                        "Environment identifier, e.g. 'gsm8k', 'humaneval', 'mt-bench'. "
                        "Returns an error if the ID is not in the catalog."
                    ),
                },
            },
        },
    ),
]


# ---------------------------------------------------------------------------
# Tool list handler
# ---------------------------------------------------------------------------

@app.list_tools()
async def list_tools() -> list[Tool]:
    return TOOLS


# ---------------------------------------------------------------------------
# Tool call dispatcher
# ---------------------------------------------------------------------------

@app.call_tool()
async def call_tool(name: str, arguments: dict[str, Any]) -> CallToolResult:
    try:
        match name:
            case "stellarator_create_run":
                data = await _client.create_run(arguments)
                return _ok(data)

            case "stellarator_list_runs":
                data = await _client.list_runs(
                    owner=arguments.get("owner"),
                    status=arguments.get("status"),
                    limit=arguments.get("limit"),
                )
                return _ok(data)

            case "stellarator_get_run":
                data = await _client.get_run(arguments["run_id"])
                return _ok(data)

            case "stellarator_cancel_run":
                data = await _client.cancel_run(arguments["run_id"])
                return _ok(data)

            case "stellarator_pause_run":
                data = await _client.pause_run(arguments["run_id"])
                return _ok(data)

            case "stellarator_resume_run":
                data = await _client.resume_run(arguments["run_id"])
                return _ok(data)

            case "stellarator_add_note":
                data = await _client.add_note(
                    arguments["run_id"],
                    arguments["kind"],
                    arguments["body"],
                )
                return _ok(data)

            case "stellarator_search_papers":
                data = await _client.search_papers(
                    arguments["q"],
                    source=arguments.get("source", "both"),
                )
                return _ok(data)

            case "stellarator_get_paper":
                data = await _client.get_paper(
                    arguments["source"], arguments["paper_id"]
                )
                return _ok(data)

            case "stellarator_cite_paper":
                data = await _client.cite_paper(
                    arguments["run_id"],
                    arguments["source"],
                    arguments["paper_id"],
                    arguments["note"],
                )
                return _ok(data)

            case "stellarator_get_checkpoint":
                data = await _client.get_checkpoint(arguments["run_id"])
                return _ok(data)

            case "stellarator_pick_environment":
                data = await _client.pick_environment(arguments["env_id"])
                return _ok(data)

            case _:
                return _err(f"Unknown tool: {name!r}")

    except Exception as exc:
        return _tool_error(exc)


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> None:
    asyncio.run(_serve())


async def _serve() -> None:
    async with stdio_server() as (read_stream, write_stream):
        await app.run(read_stream, write_stream, app.create_initialization_options())


if __name__ == "__main__":
    main()
