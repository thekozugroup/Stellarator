# Research Sub-Agent: Architecture and Audit

## Why Research Is Hidden

The research sub-agent operates in parallel with its own focused system prompt. Its findings flow into the orchestrator's context via structured responses—not as raw browsing or chat.

This design matches the Hugging Face ML Intern UX: **research is a tool, not a destination**. Humans do not browse papers; they review the agent's choices via run citations and the `agent_plan` field.

---

## Research Sub-Agent Contract

**Input:**
```python
{
  "task": str,        # e.g., "improve instruction-following on MMLU"
  "context": str,     # e.g., "1B model, 4-hour budget, LoRA preferred"
  "sources": list     # ["arxiv", "huggingface", "github"]
}
```

**Output:**
```python
{
  "research_id": str,
  "papers": [
    {
      "arxiv_id": str,
      "title": str,
      "authors": list[str],
      "year": int,
      "relevance_score": float,  # 0.0 - 1.0
      "summary": str             # 1-2 sentences on why it's relevant
    }
  ],
  "recipe": {
    "methodology": str,           # "DPO", "SFT+GRPO", etc.
    "dataset_candidates": [
      {
        "name": str,
        "size": int,
        "quality": str,           # "high", "medium", "low"
        "url": str
      }
    ],
    "hyperparams": dict,
    "estimated_training_time": str
  }
}
```

The sub-agent:
1. Searches Hugging Face Papers, arXiv, and GitHub for working code + publications
2. Filters for recency, citation count, and task relevance
3. Extracts hyperparameters from papers and successful open-source implementations
4. Returns a **structured recipe**, not prose

---

## Audit Log: `/research`

Every research query is logged at the backend's `/research` endpoint (read-only for humans):

```bash
GET http://localhost:8000/research
```

**Response:**
```json
[
  {
    "timestamp": "2026-04-30T12:00:00Z",
    "agent_id": "claude_code",
    "task": "improve instruction-following on MMLU",
    "context": "1B model, 4-hour budget",
    "research_id": "res_abc123",
    "papers_found": 15,
    "papers_returned": 3,
    "recipe_generated": true
  }
]
```

Humans read this audit log to:
- Verify the agent researched before proposing
- See which papers were considered (with relevance scores)
- Understand why certain methodologies were chosen

---

## Why Not Show Papers Inline?

1. **Signal-to-noise ratio**: A 50-paper search would flood context; a curated 3-paper recipe is actionable.
2. **Agent accountability**: The orchestrator's `agent_plan` explicitly cites papers by arxiv_id; humans can verify by ID.
3. **Parallel execution**: Sub-agent runs independently while orchestrator works; no blocking waits.
4. **UX match**: ML Intern users don't want to read 20 papers; they want a TL;DR and a 10-minute sandbox.

---

## Integration with Agent Loop

**Phase 1 (Research)** calls the sub-agent:

```python
# Orchestrator code
response = await research_agent.research(
    task="Replicate DPO baseline from arXiv 2305.18290",
    context="1B model, 4-hour budget",
    sources=["arxiv", "github"]
)

# response.papers[0] = {arxiv_id: "2305.18290", ...}
# response.recipe.methodology = "DPO"
# response.recipe.hyperparams = {"lr": 5e-5, "num_epochs": 3}
```

Orchestrator uses this structured output to design the sandbox (Phase 2).

---

## System Prompt: Research Sub-Agent

```
You are a research assistant specialized in fine-tuning methodologies.

When given a task and context, your job is to:
1. Search Hugging Face Papers, arXiv.org, and GitHub for papers and code
2. Identify the most relevant 3-5 papers based on recency and citation count
3. Extract hyperparameters and dataset recommendations
4. Return a structured JSON recipe (methodology, datasets, hyperparams, estimated time)

DO NOT return prose, markdown, or links. ONLY return valid JSON.
DO NOT speculate. If no papers match the query, return empty arrays.
ALWAYS include arxiv_id or URL for every source.

Prefer recent papers (2022+) and papers with >10 citations.
Prioritize open-source implementations over closed-source baselines.
```

---

## Next Steps

- See [docs/agent-loop.md](agent-loop.md) for Phase 1-6 orchestration
- See [docs/bootstrap.md](bootstrap.md) for how agents call the research endpoint
