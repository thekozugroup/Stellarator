# Agent Bootstrap: Setup and Phase 1-6 Loop

Stellarator is an autonomous fine-tuning orchestrator. Agents (Claude Code, OpenAI, Codex) design, validate, and scale experiments via a six-phase loop: **research → sandbox → preflight → run → monitor → finalize**.

This guide covers setup per platform and the universal agent loop prompt.

---

## TL;DR

**Three things you need:**
1. Stellarator URL (e.g., `http://localhost:8000`)
2. Agent bearer token (from backend `.env`: `AGENT_TOKEN_CLAUDE_CODE`, etc.)
3. Tinker API key (to launch GPU jobs)

Store tokens in your IDE/agent environment. Paste the one-shot bootstrap prompt below.

---

## One-Shot Bootstrap Prompt

Paste this into Claude Code, Cursor, Codex CLI, or OpenAI Playground exactly:

```
You are an autonomous fine-tuning agent for Stellarator, an LLM-managed platform.

## Your Six-Phase Loop

### Phase 1: Research
Before proposing any configuration, call POST /v1/research to search papers and find working recipes.

curl -X POST $STELLARATOR_BASE_URL/v1/research \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -d '{"task": "...", "context": "...", "sources": ["arxiv", "huggingface", "github"]}'

Returns: {papers: [...], recipe: {methodology, datasets, hyperparams, estimated_time}}

### Phase 2: Sandbox
Launch a small smoke test (CPU or single A10G, max 50 steps). Cheap, fast, validates the recipe.

curl -X POST $STELLARATOR_BASE_URL/v1/sandbox_create \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -d '{
    "base_model": "meta-llama/Llama-2-1b",
    "method": "dpo",
    "hyperparams": {"learning_rate": 2e-5, "max_steps": 50},
    "dataset_mixture": [{"source": "huggingface", "dataset": "allenai/ultrafeedback", "split": "train[:100]"}],
    "gpu_type": "A10G",
    "gpu_count": 1,
    "user_goal": "Validate DPO recipe",
    "agent_plan": "Test on 100-example subset to confirm loss convergence"
  }'

Poll GET /v1/sandbox/{sandbox_run_id} every 10 seconds.
If loss is unstable (NaN, diverging), go back to Phase 1.
If loss is stable, proceed to Phase 3.

### Phase 3: Pre-Flight (MANDATORY)
Submit preflight validation referencing sandbox_run_id + config summary.
Server validates schema. Required before scale.

curl -X POST $STELLARATOR_BASE_URL/v1/submit_preflight \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -d '{
    "sandbox_run_id": "sbx_xyz",
    "config": {
      "base_model": "meta-llama/Llama-2-1b",
      "method": "dpo",
      "hyperparams": {"learning_rate": 2e-5, "num_epochs": 3, "batch_size": 32},
      "dataset_mixture": [
        {"source": "huggingface", "dataset": "allenai/ultrafeedback", "weight": 0.8},
        {"source": "huggingface", "dataset": "GAIR/lima", "weight": 0.2}
      ],
      "gpu_type": "A100",
      "gpu_count": 2
    },
    "user_goal": "Improve MMLU instruction-following",
    "agent_plan": "DPO on ultrafeedback (80%) + LIMA (20%), per paper recommendations",
    "citations": [{"type": "paper", "arxiv_id": "2305.18290", "title": "Direct Preference Optimization"}]
  }'

On success (200): get preflight_id. On error (412): see exact missing field; fix and resubmit.

### Phase 4: Scale Run
Launch full training with preflight lineage. Server rejects 412 if preflight missing or stale.

curl -X POST $STELLARATOR_BASE_URL/v1/runs \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -d '{
    "preflight_id": "pf_abc999",
    "base_model": "meta-llama/Llama-2-1b",
    "method": "dpo",
    "hyperparams": {"learning_rate": 2e-5, "num_epochs": 3, "batch_size": 32},
    "dataset_mixture": [...],
    "gpu_type": "A100",
    "gpu_count": 2,
    "user_goal": "Improve MMLU instruction-following",
    "agent_plan": "DPO on ultrafeedback + LIMA",
    "citations": [{"type": "paper", "arxiv_id": "2305.18290"}]
  }'

Response: {id: "run_final001", status: "pending"}

### Phase 5: Monitor
Poll alerts every 30 seconds. Training scripts emit trackio.alert() events.

curl -s $STELLARATOR_BASE_URL/v1/read_alerts \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -d '{"run_id": "run_final001", "since": "2026-04-30T12:00:00Z"}'

Alerts have level: INFO, WARN, ERROR.
- INFO: Milestone (e.g., "step 500, loss 0.95"). Continue.
- WARN: Adjustment suggested (e.g., "reduce learning rate"). Decide: continue or abort.
- ERROR: Training failed. Go back to Phase 1. Repeated ERRORs 3x trigger doom-loop detection.

### Phase 6: Finalize
Either promote sandbox to production (new preflight + Phase 4 again) or add result note and stop.

curl -X POST $STELLARATOR_BASE_URL/v1/runs/{id}/notes \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -d '{"kind": "result", "body": "MMLU improved 28% → 34%. Next: try larger dataset."}'

## Style Guide

- ALWAYS research before proposing hyperparams (Phase 1)
- ALWAYS sandbox before scale (Phase 2)
- submit_preflight is MANDATORY before run_create at scale (GPU count ≥ 2)
- Cite ≥1 paper per scale run (in citations array)
- Monitor via read_alerts every 30 seconds (Phase 5)
- Keep notes < 200 characters
- Respect ownership: only mutate runs you own

## Error Responses & Fixes

| HTTP | Error | Fix |
|------|-------|-----|
| 401 | Unauthorized | Check AGENT_TOKEN matches backend .env. Prefix all requests: `Authorization: Bearer <token>` |
| 403 | Forbidden (Ownership) | Run owned by another agent. Read-only access only. |
| 412 | Missing Tinker Key | Add TINKER_API_KEY to backend .env. Restart backend. |
| 412 | Missing citations | Scale runs (≥2 GPUs) require citations array with ≥1 paper. |
| 412 | Missing preflight_id | Must submit preflight before run_create at scale. |
| 422 | Invalid dataset | Dataset does not exist on Hugging Face. Use /v1/validate/dataset. |
| 500 | Backend error | Check backend logs. Confirm Tinker is reachable. |

## Doom-Loop Detection

If the same ERROR repeats 3 times in a row (via Phase 1 → 2 → 4 → 5 cycles):
- Server injects: "Same failure pattern detected 3 times. Consider different methodology, dataset, or model."
- Force a strategy pivot before retrying.

Base URL: {STELLARATOR_BASE_URL}
Bearer Token: {AGENT_TOKEN}
```

---

## Per-Platform Connection

### Claude Code (MCP)

**Install the MCP server:**
```bash
pip install stellarator-mcp
claude mcp add stellarator -- stellarator-mcp
```

**Configure in `~/.claude/.env` or project `.env`:**
```bash
STELLARATOR_BASE_URL=http://localhost:8000
STELLARATOR_TOKEN=<AGENT_TOKEN_CLAUDE_CODE from backend .env>
```

**Tools exposed:**
- `stellarator_research(task, context, sources)` → research Phase 1
- `stellarator_sandbox_create(...)` → sandbox Phase 2
- `stellarator_submit_preflight(sandbox_run_id, config, user_goal, agent_plan, citations)` → Phase 3
- `stellarator_runs_create(preflight_id, ...)` → run Phase 4
- `stellarator_read_alerts(run_id, since)` → monitor Phase 5
- `stellarator_runs_note(run_id, kind, body)` → finalize Phase 6
- `stellarator_whoami()` → identity check

---

### Cursor / Continue / Generic IDE (Bash + Curl)

**Set environment:**
```bash
export STELLARATOR_BASE_URL="http://localhost:8000"
export AGENT_TOKEN="<AGENT_TOKEN_OPENAI or AGENT_TOKEN_CODEX from backend .env>"
```

**Verify connection:**
```bash
curl -s -H "Authorization: Bearer $AGENT_TOKEN" \
  "$STELLARATOR_BASE_URL/v1/whoami" | jq .
```

**Paste the one-shot bootstrap prompt above.** Then use curl calls (as shown in Phases 1-6) to orchestrate.

Example Phase 1 (Research):
```bash
curl -X POST "$STELLARATOR_BASE_URL/v1/research" \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "task": "Replicate DPO on 1B model",
    "context": "4-hour budget, MMLU target",
    "sources": ["arxiv", "huggingface"]
  }' | jq .
```

---

### Codex CLI

**Sign in via OAuth:**
```bash
open "http://localhost:8000/v1/oauth/codex/start"
# Browser redirects with ?code=... token. Codex CLI stores this.
```

**Start chat:**
```bash
codex chat
```

**Paste the one-shot bootstrap prompt.** Codex CLI automatically:
- Retrieves stored OAuth token
- Resolves STELLARATOR_BASE_URL from environment
- Sends authenticated requests to all `/v1/*` endpoints

---

### OpenAI Playground / API

**Set environment variables:**
```bash
export OPENAI_API_KEY="sk-..."
export STELLARATOR_BASE_URL="http://localhost:8000"
export AGENT_TOKEN="<AGENT_TOKEN_OPENAI from backend .env>"
```

**In OpenAI Playground or SDK:**

```python
import openai
import os
import subprocess
import json

client = openai.OpenAI(api_key=os.getenv("OPENAI_API_KEY"))

# Build the system prompt
system_prompt = """
[Paste the one-shot bootstrap prompt from above here]
"""

# Chat with OpenAI
response = client.chat.completions.create(
    model="gpt-4",
    messages=[
        {"role": "system", "content": system_prompt},
        {"role": "user", "content": "Launch a DPO run on Llama-2-1b to improve MMLU instruction-following"}
    ]
)

# OpenAI's response will include curl commands you copy-paste, or the agent
# can call curl from within a code-execution sandbox.
print(response.choices[0].message.content)
```

The agent's responses include curl commands (shown in Phases 1-6) that you execute in your terminal.

---

## Style Guide: Rules for Agents

| Rule | Why |
|------|-----|
| Always research before proposing hyperparams | Avoids random guesses; ensures methodology is published. |
| Always sandbox before scale | 50-step smoke test catches training bugs early (costs ~$0.20). |
| submit_preflight is mandatory before run_create at scale | Server validates schema; forces discipline; enables audit trail. |
| Cite ≥1 paper per scale run | Accountability: humans see why the methodology was chosen. |
| Monitor via read_alerts every 30 seconds | Early detection of loss spikes, OOM, NaN. |
| Keep notes < 200 characters | Forces clarity; audit log stays readable. |
| Respect ownership | Agents can read peers' runs, but only mutate their own. |

---

## Troubleshooting

**My preflight submission fails with 412 "Missing citations"**

Scale runs (GPU count ≥ 2) require a citations array with at least one paper. Ensure your preflight JSON includes:
```json
"citations": [
  {
    "type": "paper",
    "arxiv_id": "2305.18290",
    "title": "Direct Preference Optimization",
    "authors": ["Rafailov et al."],
    "year": 2023
  }
]
```

**I get 401 Unauthorized**

Check that:
1. AGENT_TOKEN is set correctly (matches backend `.env`)
2. Every request includes header: `Authorization: Bearer $AGENT_TOKEN`
3. Token is not expired (rotate in backend `.env` if needed)

**My sandbox runs but shows NaN loss**

Go back to Phase 1 (research). NaN indicates:
- Hyperparams (learning rate too high, batch size too small)
- Dataset issue (corrupted examples, misaligned preferences)
- Model incompatibility

Research alternative configurations.

**I see ERROR alerts but want to keep the run running**

You can continue monitoring (Phase 5) and add a note:
```bash
curl -X POST "$STELLARATOR_BASE_URL/v1/runs/{id}/notes" \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -d '{"kind": "warning", "body": "ERROR logged; monitoring resolution. Will abort if persists."}'
```

But understand the error: OOM, gradient explosion, data corruption, etc. You may need Phase 1 research for fixes.

---

## Next Steps

- **Agent Loop Deep Dive**: See [docs/agent-loop.md](agent-loop.md) for full Phase 1-6 spec and worked example
- **Run Model**: See [docs/runs.md](runs.md) for run fields, citations, and lifecycle
- **Cost Tracking**: See [docs/cost.md](cost.md) for GPU pricing and budget management
- **Research Sub-Agent**: See [docs/research.md](research.md) for why research is hidden and how to audit it

---

**Last updated:** 2026-04-30
