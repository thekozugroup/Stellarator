# Run Ownership and Access Control

Each run is owned by the agent that created it. Ownership enforces a simple rule: read anywhere, mutate by owner only.

## Ownership Model

When you launch a run via any surface (Claude Code, OpenAI Chat, Codex), the backend:

1. Identifies your agent ID from your authentication token
2. Tags the run with `agent_id: "claude_code"` (or `"openai"`, `"codex"`)
3. Stores the run in the database

From that moment on:

- **Any agent** can read the run (GET `/v1/runs/{id}`, list all runs)
- **Only the owner** can mutate (POST to pause/cancel, PUT to edit notes, etc.)

---

## Access Control Rules

| Action | Any Agent | Owner Only |
|--------|-----------|-----------|
| Get run details | ✓ | ✓ |
| List all runs | ✓ | ✓ |
| Add notes | | ✓ |
| Pause/cancel | | ✓ |
| Edit hyperparams (before start) | | ✓ |
| Delete run | | ✓ |

---

## Why Ownership Matters

1. **Prevent accidents** — You can't accidentally cancel someone else's $5,000 training job
2. **Audit trail** — Each run says which agent designed and launched it
3. **Multi-agent workflows** — One agent can request another to launch a run, but the requester can't sabotage it
4. **Resource isolation** — Budget limits can be enforced per-agent

---

## Transfer Ownership

Currently, ownership transfer is **manual database only**. This is intentional: we want the decision to be deliberate and auditable.

### Transfer Procedure (for now)

```sql
-- In your Stellarator SQLite database:
UPDATE runs SET agent_id = 'openai' WHERE id = 'run_xyz123';
```

Then verify:

```sql
SELECT id, agent_id, user_goal, status FROM runs WHERE id = 'run_xyz123';
```

**When would you transfer ownership?**

- An agent that launched a run is being decommissioned
- An agent authored a run but you want another agent to manage it going forward
- Recovering from an agent token compromise (change `agent_id` to a fresh agent)

### Future: API-Driven Transfer

In a future release, we'll add:

```bash
curl -X POST http://localhost:8000/v1/runs/{id}/transfer \
  -H "Authorization: Bearer OWNER_TOKEN" \
  -d '{"new_agent_id": "openai"}' \
```

This will log the transfer and notify both agents.

---

## Multi-Agent Workflows

### Scenario: Claude Code Researches, OpenAI Trains

1. Claude Code uses `search_arxiv()` and `search_hf_papers()` tools to design a training plan
2. Claude Code creates a run with `user_goal` and `agent_plan`
3. OpenAI agent sees the run in `/v1/runs` (readable by all)
4. OpenAI **cannot** mutate it (not the owner)
5. OpenAI can launch its own run based on the same methodology

### Scenario: Humans Audit Agent Runs

1. All agents are transparent: dashboard shows `agent_id` on every run
2. A human user (e.g., a manager) can see what each agent has launched
3. If an agent misbehaves, you can disable its token or transfer its runs to a supervisor agent

---

## Authentication & Tokens

Each agent has a distinct bearer token:

- **Claude Code**: `AGENT_TOKEN_CLAUDE_CODE` from backend `.env`
- **OpenAI**: `AGENT_TOKEN_OPENAI` from backend `.env`
- **Codex**: `AGENT_TOKEN_CODEX` from backend `.env` (or via OAuth)

You specify the token in your request:

```bash
curl -X POST http://localhost:8000/v1/runs \
  -H "Authorization: Bearer $AGENT_TOKEN_OPENAI" \
  -d '{...}'
```

The backend extracts the agent ID from the token. If a token is compromised:

1. Rotate the token in `.env`
2. Restart the backend
3. Optionally transfer existing runs to a fresh agent ID

---

## Next Steps

- See [cost.md](cost.md) for per-agent budget limits (future feature)
- See [agents.md](agents.md) for how each surface authenticates
