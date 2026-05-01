# Connecting Agent Surfaces

Stellarator supports three agent surfaces: Claude Code via MCP, OpenAI Chat Completions, and Codex OAuth. Each agent gets its own bearer token and can launch, monitor, and manage training runs.

## Claude Code via MCP

### Install and Configure

Install the MCP server package:

```bash
pip install stellarator-mcp
```

Add Stellarator to Claude Code:

```bash
claude mcp add stellarator -- stellarator-mcp
```

Configure the token in `.env`:

```bash
# In your project's .env or ~/.claude/.env
STELLARATOR_TOKEN=your_AGENT_TOKEN_CLAUDE_CODE_from_backend_env
```

The token must match `AGENT_TOKEN_CLAUDE_CODE` in your backend `.env`.

### Sample Prompt

```
You are an AI fine-tuning agent. Your goal: design and launch an SFT run to improve 
model helpfulness on a specific benchmark.

Steps:
1. Search HF Papers for recent SFT methodologies
2. Design a dataset mixture combining public + proprietary data
3. Launch a training run on Stellarator with your plan
4. Monitor metrics until the run finishes
5. Report the final loss and recommendations

Use the available tools: research (HF Papers + Arxiv), and stellarator_run (create, 
get status, cancel).
```

The MCP server exposes:

- `stellarator_run_create(base_model, method, hyperparams, dataset_mixture, gpu_type, gpu_count, user_goal)` — Launch a run
- `stellarator_run_get(run_id)` — Fetch run details and status
- `stellarator_run_list()` — List all runs you own
- `stellarator_run_cancel(run_id)` — Cancel a run
- `stellarator_run_add_note(run_id, text)` — Append to the run journal

---

## OpenAI Chat

### Dashboard Method

1. Open http://localhost:3000/chat (or your production domain)
2. Paste your OpenAI API key in the "API Key" field
3. Select a model (gpt-4, gpt-4-turbo, etc.)
4. Type your training goal, e.g.: "Fine-tune a 7B model on math reasoning with DPO"
5. Press Send

The backend routes your message to OpenAI, receives the plan, and auto-launches runs based on OpenAI's instructions.

### API Method

```bash
curl -X POST http://localhost:8000/v1/chat/sessions \
  -H "Authorization: Bearer AGENT_TOKEN_OPENAI" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Launch an SFT run on Llama 3 to improve summarization"}]}'
```

Response includes `session_id`:

```json
{
  "session_id": "sess_abc123",
  "agent_id": "openai",
  "created_at": "2026-04-30T12:00:00Z"
}
```

Send a follow-up message to the session:

```bash
curl -X POST http://localhost:8000/v1/chat/sessions/sess_abc123/messages \
  -H "Authorization: Bearer AGENT_TOKEN_OPENAI" \
  -H "Content-Type: application/json" \
  -d '{"role": "user", "content": "What'\''s the training loss after 100 steps?"}'
```

The backend maintains conversation history and can launch/monitor runs based on the conversation flow.

---

## Codex OAuth

### Sign-In Flow

1. Open http://localhost:3000/chat
2. Click "Sign in with Codex"
3. You're redirected to the Codex auth server (configured via `CODEX_OAUTH_CLIENT_ID` and `CODEX_OAUTH_CLIENT_SECRET` in backend `.env`)
4. After approval, you're redirected back with an authorization code
5. The backend exchanges the code for a long-lived Codex API token
6. Your session is now authenticated as the Codex agent

### API Flow (for headless/CLI clients)

If you want to bypass the dashboard and hit the API directly:

```bash
# Step 1: Redirect user to Codex auth server
open "http://localhost:8000/v1/oauth/codex/start"

# Step 2: User approves and is redirected with ?code=...
# Step 3: Exchange code for Codex token (backend handles this)

# Step 4: Use the authenticated session to launch a run
curl -X POST http://localhost:8000/v1/runs \
  -H "Authorization: Bearer YOUR_CODEX_SESSION_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{...run payload...}'
```

### Environment Variables

Ensure your backend `.env` includes:

```bash
CODEX_OAUTH_CLIENT_ID=your_oauth_client_id
CODEX_OAUTH_CLIENT_SECRET=your_oauth_client_secret
CODEX_OAUTH_REDIRECT_URI=http://localhost:3000/auth/codex/callback
```

---

## Key Differences

| Surface | Auth | Ownership | Best For |
|---------|------|-----------|----------|
| **Claude Code (MCP)** | Bearer token | `claude_code` | Automated research + fine-tuning workflows in Claude IDE |
| **OpenAI Chat** | OpenAI API key | `openai` | Conversational fine-tuning design in web UI |
| **Codex OAuth** | OAuth token | `codex` | Integrated Codex IDE workflows |

All three surfaces write to the same database, share the same Tinker backend, and can read each other's runs (but only mutate their own).

---

## Next Steps

- See [runs.md](runs.md) for the anatomy of a run and what to put in `user_goal` and `agent_plan`
- See [ownership.md](ownership.md) for access control rules
- See [examples/](../examples/) for worked training scenarios
