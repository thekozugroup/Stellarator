# Stellarator MCP Server

MCP server that lets Claude Code drive Tinker fine-tuning runs on the Stellarator backend.

## Install

```bash
cd mcp_server
pip install -e .
# Or with dev deps:
pip install -e ".[dev]"
```

## Required env var

```bash
export STELLARATOR_TOKEN=<your-bearer-token>
# Optional: override backend URL (default: http://localhost:8000)
export STELLARATOR_BASE_URL=http://backend:8000
```

## Add to Claude Code

```bash
claude mcp add stellarator -- stellarator-mcp
```

Or manually in `.claude/mcp.json` / `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "stellarator": {
      "command": "stellarator-mcp",
      "env": {
        "STELLARATOR_TOKEN": "${STELLARATOR_TOKEN}",
        "STELLARATOR_BASE_URL": "http://localhost:8000"
      }
    }
  }
}
```

## Run tests

```bash
cd mcp_server
pytest tests/ -v
```

## Tools exposed

| Tool | Endpoint |
|------|----------|
| `stellarator_create_run` | POST /v1/runs |
| `stellarator_list_runs` | GET /v1/runs |
| `stellarator_get_run` | GET /v1/runs/{id} |
| `stellarator_cancel_run` | POST /v1/runs/{id}/cancel |
| `stellarator_pause_run` | POST /v1/runs/{id}/pause |
| `stellarator_resume_run` | POST /v1/runs/{id}/resume |
| `stellarator_add_note` | POST /v1/runs/{id}/notes |
| `stellarator_search_papers` | GET /v1/research/papers/search |
| `stellarator_get_paper` | GET /v1/research/papers/{source}/{id} |
| `stellarator_cite_paper` | POST /v1/research/runs/{id}/cite |
