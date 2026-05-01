# Anatomy of a Run

A "run" is a complete training job: metadata, configuration, execution state, and cost tracking. Every run is owned by the agent that created it.

## Run Model Fields

```python
# Identity
id: str                          # UUID, immutable
agent_id: str                    # "claude_code", "openai", or "codex"
tinker_job_id: str | None        # Tinker's job ID (set after creation)

# Goals & Reasoning
user_goal: str                   # High-level intent: what should the model get better at?
agent_plan: str                  # Agent's reasoning: which papers, datasets, and why?

# Training Configuration
base_model: str                  # e.g., "meta-llama/Llama-2-7b-hf"
method: str                      # "sft", "dpo", "grpo", "ppo"
hyperparameters: dict            # Learning rate, batch size, warmup steps, etc.
dataset_mixture: list[dict]      # Each item: {name, split, weight, citation_key}

# Compute
gpu_type: str                    # "H100", "A100", etc.
gpu_count: int                   # Number of GPUs
gpu_seconds: float               # Actual GPU time used (from Tinker telemetry)
cost_usd: float                  # Total cost (computed after run finishes)

# Tracking
notes: list[str]                 # Journal of updates from the agent
citations: list[dict]            # References to papers/datasets
created_at: datetime
started_at: datetime | None
finished_at: datetime | None
status: str                      # "pending", "running", "succeeded", "failed", "cancelled"
```

---

## Writing a Good user_goal

The `user_goal` is read by humans (and future agents) to understand why this run exists. Be specific about the problem and the desired outcome.

**Good examples:**

- "Improve model performance on mathematical reasoning benchmarks (MATH, GSM8K) using domain-specific SFT"
- "Reduce hallucinations in factual summarization using DPO with preference pairs from fact-checking"
- "Generalize code-generation to less common programming languages via multi-language SFT"

**Avoid:**

- "Train the model" (too vague)
- "Experiment" (what are you testing?)
- "Improve helpfulness" (which metrics? which domain?)

---

## Writing a Good agent_plan

The `agent_plan` is the agent's reasoning for *how* to achieve the `user_goal`. Include:

1. **Methodology choice** — Why SFT vs. DPO vs. GRPO? Cite papers.
2. **Dataset selection** — Which datasets? Why this mixture?
3. **Hyperparameters** — Learning rate, batch size, num epochs. Justify based on prior work.
4. **Evaluation strategy** — How will you know if it worked?

**Example:**

```
Goal: Improve math reasoning on GSM8K (grade-school math word problems).

Plan:
- Methodology: SFT with chain-of-thought (CoT) supervision.
  Reference: Wei et al. (2022) "Chain-of-Thought Prompting" shows CoT improves reasoning by 10-20%.
  
- Datasets:
  1. GSM8K train split (7.5K examples) - weight 0.5 - target domain
  2. MATH dataset (7.5K examples) - weight 0.3 - harder examples
  3. Internal math tutoring logs (5K examples) - weight 0.2 - company-specific notation

- Hyperparameters:
  Learning rate: 2e-5 (following LLaMA fine-tuning guidelines)
  Batch size: 128
  Num epochs: 3
  Warmup steps: 500
  Max tokens: 512

- Evaluation:
  Track validation loss every 100 steps.
  After training, evaluate on GSM8K test (1,319 examples) using majority voting with 3 samples.
```

---

## Citations

Each run can reference papers and datasets. The `citations` field is a list of dicts:

```python
citations = [
    {
        "type": "paper",
        "title": "Chain-of-Thought Prompting Elicits Reasoning in Large Language Models",
        "authors": ["Jason Wei", "Xuezhi Wang", ...],
        "year": 2022,
        "arxiv_id": "2201.11903",
        "url": "https://arxiv.org/abs/2201.11903"
    },
    {
        "type": "dataset",
        "name": "GSM8K",
        "source": "OpenAI",
        "url": "https://huggingface.co/datasets/openai/gsm8k",
        "license": "CC-BY-4.0"
    }
]
```

### How Agents Add Citations

**Via MCP (Claude Code):**

```python
# Claude has research tools: search_arxiv(), search_hf_papers()
papers = search_arxiv("chain of thought reasoning")
citations = [p.to_citation_dict() for p in papers[:3]]

run = client.stellarator_run_create(
    base_model="meta-llama/Llama-2-7b",
    method="sft",
    hyperparams={"lr": 2e-5, "epochs": 3},
    dataset_mixture=[...],
    user_goal="Improve math reasoning",
    agent_plan="...",
    citations=citations  # Pass citations when creating
)
```

**Via API (OpenAI, Codex):**

```bash
curl -X POST http://localhost:8000/v1/runs \
  -H "Authorization: Bearer AGENT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "base_model": "meta-llama/Llama-2-7b",
    "method": "sft",
    "hyperparameters": {"lr": 2e-5, "epochs": 3},
    "dataset_mixture": [...],
    "user_goal": "Improve math reasoning",
    "agent_plan": "...",
    "citations": [
      {
        "type": "paper",
        "title": "Chain-of-Thought Prompting...",
        "arxiv_id": "2201.11903"
      }
    ]
  }'
```

---

## Run Lifecycle

1. **Created** (status: `pending`)
   - Agent provides `user_goal`, `agent_plan`, config, citations
   - Run is stored in DB, but no job launched yet

2. **Submitted to Tinker** (status: `pending` → `running`)
   - Backend calls `TinkerClient.create_job()`
   - `tinker_job_id` is set
   - GPU time starts accruing

3. **Running**
   - Agent can poll `/v1/runs/{id}` to fetch current metrics
   - Agent can add notes via `PUT /v1/runs/{id}/notes`
   - Only the owner can pause/cancel/edit

4. **Finished** (status: `succeeded`, `failed`, or `cancelled`)
   - Tinker reports final metrics
   - `gpu_seconds` and `cost_usd` are finalized
   - Run is immutable (read-only for all agents)

---

## Next Steps

- See [cost.md](cost.md) for cost tracking details
- See [ownership.md](ownership.md) for access control rules
- See [examples/](../examples/) for concrete run payloads
