# RL Tool Gap Analysis: Hermes vs Stellarator
_Generated: 2026-04-30_

---

## Part 1: Hermes Agent RL Tool Surface

### Complete `rl` Toolset (10 tools)

| Tool | What it does | Inputs / Outputs | RL Phase |
|------|-------------|-----------------|----------|
| `rl_list_environments` | Scans `tinker-atropos/tinker_atropos/environments/` via AST to find `BaseEnv` subclasses | out: `[{name, path, description}]` | Data prep / env discovery |
| `rl_select_environment` | Loads an environment and its default config | in: `env_name`; out: config object | Data prep |
| `rl_get_current_config` | Returns editable config fields: `group_size`, `max_token_length`, `total_steps`, `steps_per_eval`, `use_wandb`, `wandb_name`, `max_num_workers` | out: `{configurable: {...}, locked: {...}}` | Training launch |
| `rl_edit_config` | Modifies one configurable field (locked infra fields like `lora_rank`, `learning_rate` are immutable via this tool) | in: `{field, value}` | Training launch |
| `rl_start_training` | Spawns 3 processes: Atropos API server, Tinker trainer, environment runner | in: current config; out: `run_id` | Training launch |
| `rl_check_status` | Rate-limited (30-min minimum between checks); returns WandB metrics: `step`, `state`, `reward_mean`, `loss`, `percent_correct` | in: `run_id`; out: metrics dict | Monitoring |
| `rl_stop_training` | Terminates a running training job | in: `run_id` | Training launch |
| `rl_get_results` | Returns final metrics and path to trained weights | in: `run_id`; out: `{metrics, weights_path}` | Post-training |
| `rl_list_runs` | Lists all active and completed runs | out: `[{run_id, status, env, started_at}]` | Monitoring |
| `rl_test_inference` | Quick inference test via OpenRouter against current model | in: prompt; out: response + latency | Eval / debugging |

### Underlying Architecture Tools (not directly agent-callable, but part of the RL system)

| Component | Role | RL Phase |
|-----------|------|----------|
| **Atropos** (trajectory API server) | Coordinates environment interactions, manages rollout groups, computes advantages (GRPO) | Training launch / rollout collection |
| **Tinker** (training service) | Handles model weights, LoRA training, sampling/inference, optimizer steps | Training launch |
| **BaseEnv** (`load_dataset`, `get_next_item`, `score_answer`, `collect_trajectories`) | Custom Python classes defining tasks, scoring, and reward functions | Data prep / reward modeling |
| **WandB** | Metrics tracking (`reward_mean`, `loss`, `percent_correct`, `step`) | Monitoring |
| **OpenRouter** | Inference backend for `rl_test_inference` | Eval during training |
| **Log files** (`~/.hermes/logs/rl_training/`) | Per-run API/trainer/env logs | Debugging |

### Python SDK Surface Imports (Hermes side)
- `tinker-atropos` submodule (Python package)
- `TINKER_API_KEY` → Tinker REST API (`/jobs`, `/jobs/{id}`, `/jobs/{id}/cancel`, etc.)
- `WANDB_API_KEY` → W&B run metrics
- `ast` module for environment discovery
- OpenRouter API for inference tests

---

## Part 2: Stellarator Agent Tool Surface

### Orchestrator Tools (in `backend/app/agents/tools.py`, OpenAI function-call format)

| Tool | What it does | Inputs | RL Phase |
|------|-------------|--------|----------|
| `research` | Spawns research sub-agent to extract methodology + hyperparams + working code from papers/repos; returns structured JSON recipe | `task: str`, `context: str`; out: JSON recipe | Data prep / hyperparameter selection |
| `sandbox_create` | Creates a small CPU/GPU smoke-test run (`is_sandbox=True`, bypasses preflight) | `name`, `base_model`, `method` (`sft/dpo/grpo/ppo/rm`), `hyperparams`, `dataset_mixture`, `max_steps` | Training launch (validation) |
| `submit_preflight` | Validates a production plan without creating a run; enforces budget and recipe completeness | `_PREFLIGHT_SCHEMA`: `{model, method, dataset_mixture, hyperparams, sandbox_run_id, sandbox_summary, projected_cost_usd, citations[]}` | Training launch (gate) |
| `run_create` | Creates a production fine-tuning run after preflight passes | Same as preflight schema | Training launch |
| `read_alerts` | Polls `trackio.alert` stream for a run (`ERROR→re-research, WARN→tweak, INFO→milestone`) | `run_id`, `since: ISO-8601`; out: `[{level, title, body, source, created_at}]` | Monitoring |
| `read_run` | Gets run status, last metrics, and notes | `run_id`; out: `RunDetail` | Monitoring |

**Backwards-compat aliases:** `create_run` → `run_create`

### Research Sub-Agent Tools (in `backend/app/agents/research_subagent.py`, NOT visible to orchestrator)

| Tool | What it does | RL Phase |
|------|-------------|----------|
| `hf_papers_search` | Search HuggingFace Papers API | Data prep |
| `hf_paper_read` | Fetch full abstract for a paper by arxiv_id | Data prep |
| `hf_paper_citation_graph` | Traverse HF citation graph from seed paper (depth 1-2) | Data prep |
| `arxiv_search` | Search arXiv directly | Data prep |
| `github_find_examples` | GitHub Code Search for working training examples | Data prep |
| `github_read_file` | Read a single file from a public GitHub repo | Data prep |

### MCP Server Tools (in `mcp_server/stellarator_mcp/server.py`, 10 tools exposed to Claude Code)

| Tool | What it does | RL Phase |
|------|-------------|----------|
| `stellarator_create_run` | Create a Tinker fine-tuning run (lora/qlora/full/dpo/orpo/sft) | Training launch |
| `stellarator_list_runs` | List runs filtered by owner/status | Monitoring |
| `stellarator_get_run` | Get full run details + recent metrics (loss, eval scores, throughput) | Monitoring |
| `stellarator_pause_run` | Pause a running run | Training launch |
| `stellarator_resume_run` | Resume a paused run | Training launch |
| `stellarator_cancel_run` | Cancel a run | Training launch |
| `stellarator_search_papers` | Search HF Papers + arXiv (or both) | Data prep |
| `stellarator_get_paper` | Fetch full metadata + abstract for a paper | Data prep |
| `stellarator_cite_paper` | Attach a paper citation to a run | Post-training |
| `stellarator_note_run` | Add a categorized markdown note to a run | Post-training |

### Backend Services

| Service | Capabilities |
|---------|-------------|
| `TinkerClient` (`services/tinker.py`) | `create_job`, `get_job`, `list_jobs`, `cancel_job`, `pause_job`, `resume_job`, `stream_metrics` (SSE) |
| `AlertService` (`services/alerts.py`) | `record_alert`, `list_alerts` — ingests `trackio.alert()` POSTs |
| `NotificationService` (`services/notifications.py`) | SSE fan-out per agent: `notify_run_update`, `notify_alert_error` |
| `CostService` (`services/cost.py`) | `monthly_spend`, `check_budget` — monthly budget gate |
| `GitHubService` (`services/github.py`) | `find_examples`, `read_file` — Code Search + file read with LRU cache |
| `HFPapersClient` + `ArxivClient` (`services/research.py`) | Paper search, fetch, citation graph |
| `ReconcileService` (`services/reconcile.py`) | Background loop syncing Tinker job status → DB |
| `CryptoService` (`services/crypto.py`) | Fernet encryption for per-agent API keys |

### REST API Endpoints Available via Tools

- `POST /v1/runs/` — create run
- `GET /v1/runs/` — list runs
- `GET /v1/runs/{id}` — get run
- `POST /v1/runs/{id}/cancel`
- `POST /v1/runs/{id}/pause`
- `POST /v1/runs/{id}/resume`
- `POST /v1/runs/{sandbox_id}/promote`
- `POST /v1/runs/{id}/alerts` — ingest alert
- `GET /v1/runs/{id}/alerts` — read alerts
- `POST /v1/runs/{id}/notes`

---

## Part 3: Side-by-Side Comparison

| Hermes Capability | Stellarator Equivalent | Gap | Priority |
|-------------------|----------------------|-----|----------|
| `rl_list_environments` — discover BaseEnv subclasses via AST | None — environments are not a first-class concept; methods are an enum (`lora/qlora/sft/dpo`) | **Missing** | High |
| `rl_select_environment` — load env config | None | **Missing** | High |
| `rl_get_current_config` — view editable vs locked params | `submit_preflight` validates schema but no "view current config" introspection tool | **Partial** | Medium |
| `rl_edit_config` — modify individual training params | Hyperparams passed as opaque dict at create time; no mid-planning edit tool | **Missing** | Medium |
| `rl_start_training` — launches Atropos + Tinker + env (3 processes) | `run_create` / `stellarator_create_run` → Tinker job (single service call) | **Partial** — no Atropos/env runner orchestration | High |
| `rl_check_status` — WandB metrics (`reward_mean`, `loss`, `percent_correct`) | `read_run` / `stellarator_get_run` → Tinker job status + basic metrics; `stream_metrics` SSE available | **Partial** — metrics schema is narrower; no WandB integration, no reward_mean/percent_correct | High |
| `rl_stop_training` | `cancel_run` / `stellarator_cancel_run` | **Full** | — |
| `rl_get_results` — final metrics + weights path | `read_run` returns final status; no explicit "weights path" field | **Partial** | Medium |
| `rl_list_runs` | `list_runs` / `stellarator_list_runs` | **Full** | — |
| `rl_test_inference` — quick inference test via OpenRouter | None — no inference-during-training or eval harness tool | **Missing** | High |
| Atropos trajectory API (rollout collection, advantage computation) | None — no trajectory/rollout primitive | **Missing** | High |
| `BaseEnv` interface (`score_answer`, `collect_trajectories`, reward fn) | None — reward modeling is inside Tinker; not agent-accessible | **Missing** | High |
| WandB integration (dashboard, sweeps, artifact versioning) | trackio alert pipeline + Prometheus/Grafana (`prometheus_fastapi_instrumentator`) + SSE notifications | **Partial** — see W&B Assessment below | High |
| Hyperparameter sweep / config search | None | **Missing** | Medium |
| Checkpoint management / weights path retrieval | Not exposed as agent tool; Tinker stores weights internally | **Missing** | Medium |
| Dataset loading from HuggingFace (inside env) | `dataset_mixture` schema references HF datasets by name | **Partial** — no dataset preview/filter/decontamination tool | Medium |
| Dataset filtering / decontamination | None | **Missing** | Low |
| Eval harness (lm-eval, OpenAI evals) | None | **Missing** | Medium |
| Safety / refusal evaluation | None | **Missing** | Low |
| Custom environment authoring | None — no tool to scaffold/write env files | **Missing** | Low |
| Model merging / distillation helpers | None | **Missing** | Low |
| LoRA/QLoRA as training method | Yes — `method` enum includes `lora`, `qlora` | **Full** | — |
| Research sub-agent (papers + GitHub) | `research` tool → sub-agent with HF+arXiv+GitHub tools | **Full** | — |
| Sandbox smoke-test before production run | `sandbox_create` + preflight gate | **Full** | — |
| Budget gate / cost projection | `submit_preflight` requires `projected_cost_usd`; `check_budget` enforces monthly limit | **Full** (unique to Stellarator) | — |
| Per-agent encrypted API key management | `IntegrationKey` + Fernet crypto | **Full** (unique to Stellarator) | — |
| Alert pipeline (`trackio.alert` → agent loop) | `read_alerts` / `post_alert` | **Full** (unique to Stellarator) | — |
| Citation tracking (paper → run linkage) | `stellarator_cite_paper` | **Full** (unique to Stellarator) | — |
| Run notes (structured audit trail) | `stellarator_note_run` | **Full** (unique to Stellarator) | — |
| Pause / resume run | `pause_run` / `resume_run` | **Full** | — |
| Background reconciliation (Tinker ↔ DB sync) | `ReconcileService` background loop | **Full** | — |

---

## W&B Assessment

**Is Weights & Biases optimal for the RL monitoring slot?**

**What Hermes uses W&B for:**
- `reward_mean`, `loss`, `percent_correct`, `step`, `state` per training step
- Named run tracking (`wandb_name` config field)
- Read-back via `rl_check_status` (30-min rate limit; pulls from W&B API)

**What Stellarator currently has:**

| Feature | W&B | Stellarator Today |
|---------|-----|-------------------|
| Per-step scalar charts (loss, reward) | Yes — full dashboard | Partial — `stream_metrics` SSE streams Tinker raw metrics; no chart UI |
| Hyperparameter sweeps (Bayesian, grid) | Yes — W&B Sweeps | Missing entirely |
| Artifact versioning (checkpoints, datasets) | Yes — W&B Artifacts | Missing — weights path is opaque |
| System metrics (GPU utilization, memory) | Yes — automatic | Prometheus via `prometheus_fastapi_instrumentator` — but FastAPI-level only, not GPU-level |
| Team dashboards / experiment comparison | Yes | Missing |
| Alert routing | Basic (W&B Alerts) | Strong — `trackio.alert` → SSE → agent loop → re-research/tweak logic. Better than W&B for agentic response. |
| Prometheus/Grafana | No | Yes — HTTP-level metrics |
| Cost tracking | No | Yes — `monthly_spend`, `check_budget` |

**Verdict:** Stellarator's trackio alert pipeline is _superior_ for agentic response (ERROR → re-research, WARN → tweak hyperparams, INFO → milestone). The Prometheus/Grafana stack covers infrastructure health. However, W&B fills three concrete gaps that Stellarator does not cover:

1. **Per-step training charts** — `reward_mean` and `percent_correct` over steps need a chart backend; the agent currently gets these only by polling `read_run`, not from a persistent time-series store.
2. **Hyperparameter sweeps** — no sweep primitive exists anywhere in Stellarator.
3. **Artifact versioning** — checkpoint lineage is not tracked.

**Recommendation:** Add W&B as an optional integration (key stored in `IntegrationKey` like Tinker), integrated into `read_run`/`stellarator_get_run` to surface `reward_mean`/`percent_correct` alongside the existing metrics. Do not replace the trackio alert pipeline — it is better for the agent loop.

---

## Prioritized Tool Additions for Stellarator

### 1. `rl_check_status` — reward/eval metrics via Tinker metrics stream (High)
**File:** `backend/app/agents/tools.py` + `backend/app/services/tinker.py`
**What:** Expose `reward_mean`, `loss`, `percent_correct` from Tinker's `/jobs/{id}/metrics/stream` SSE as structured fields on `read_run`. The SSE endpoint already exists in `TinkerClient.stream_metrics()` — wire it into the run model and surface it in the agent tool response.

### 2. `rl_test_inference` — in-loop inference eval tool (High)
**File:** `backend/app/agents/tools.py` + `backend/app/api/runs.py`
**What:** Add `eval_run(run_id, prompt)` tool that calls the Tinker job's inference endpoint (or OpenRouter with the current checkpoint) and returns the response. Required for eval-during-training.

### 3. `rl_list_environments` / `rl_select_environment` — environment registry (High)
**File:** `backend/app/agents/tools.py` + new `backend/app/services/environments.py`
**What:** Introduce a registry of RL environment configs (at minimum: `gsm8k`, `math`, `code`, `custom`). Agent selects an environment to get a default hyperparams + dataset_mixture scaffold. This replaces the current open-ended `hyperparams: {}` schema with structured defaults.

### 4. Atropos/GRPO trajectory tool — `run_create` extension for GRPO method (High)
**File:** `backend/app/api/runs.py` + `backend/app/services/tinker.py`
**What:** Extend the `method` enum with `grpo` (it currently accepts `grpo` in the orchestrator's `sandbox_create` but the MCP tool only exposes `lora/qlora/full/dpo/orpo/sft`). Add Atropos config fields (`group_size`, `max_num_workers`, `steps_per_eval`) to the hyperparams schema so the agent can configure GRPO runs end-to-end.
**File also:** `mcp_server/stellarator_mcp/server.py` — update method enum.

### 5. `submit_preflight` — add config introspection tool (Medium)
**File:** `backend/app/agents/tools.py`
**What:** Add `get_run_config(run_id)` that returns the full hyperparams and dataset_mixture for a run (read-back of what was submitted). Currently the agent has no way to inspect a planned config before submitting — it must reconstruct from memory. Maps to Hermes `rl_get_current_config`.

### 6. W&B integration — artifact + sweep support (Medium)
**File:** `backend/app/services/wandb.py` (new) + `backend/app/agents/tools.py`
**What:** Add `WandBClient` (key stored as `IntegrationKey` kind=`wandb`) with three methods: `log_run_metrics(run_id, step, metrics)`, `get_run_metrics(run_id)`, `create_sweep(config)`. Wire `log_run_metrics` into the reconcile loop. Expose `create_sweep` and `get_run_metrics` as agent tools. This fills the chart + sweep gap without replacing the alert pipeline.

### 7. `eval_harness` tool — lm-eval-harness integration (Medium)
**File:** `backend/app/agents/tools.py` + `backend/app/services/eval.py` (new)
**What:** Add `eval_model(run_id, benchmark)` that submits the finished checkpoint to an lm-evaluation-harness job (can be a Tinker eval job or a self-hosted runner). Returns `{benchmark, score, baseline}`. Maps to Hermes `rl_get_results` + eval phase.

### 8. Checkpoint artifact tool — `get_checkpoint_path` (Medium)
**File:** `backend/app/agents/tools.py` + `backend/app/api/runs.py`
**What:** Add `get_checkpoint(run_id, step=None)` that returns the S3/local path of the model checkpoint at a given step (or final). Currently `read_run` returns status but not the weights path. This enables post-training workflows (merge, distill, upload to HF Hub).

### 9. Dataset preview / decontamination tool (Low)
**File:** `backend/app/agents/tools.py` + `backend/app/services/datasets.py` (new)
**What:** Add `preview_dataset(hf_id, split, n_rows)` and `check_contamination(hf_id, eval_set)` tools wrapping HuggingFace datasets API. Required before mixing custom data into training to detect eval set leakage.

### 10. Hyperparameter sweep tool — `create_sweep` (Medium)
**File:** `backend/app/agents/tools.py` + `backend/app/services/wandb.py`
**What:** Add `create_sweep(base_config, param_ranges, strategy)` that launches a W&B sweep (or simple grid via multiple `sandbox_create` calls). Returns `sweep_id`. The agent can then monitor sweep runs via `list_runs(sweep_id=...)`. This is the single biggest gap vs Hermes for systematic hyperparameter optimization.
