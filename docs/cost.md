# Cost Model and Tracking

Stellarator tracks GPU costs per run based on actual usage reported by Tinker.

## Cost Formula

```
cost_usd = (gpu_seconds / 3600) × rate_per_hour × gpu_count
```

Where:

- `gpu_seconds` — Actual compute time from Tinker telemetry (seconds)
- `rate_per_hour` — Hourly rate for the GPU type (set in backend `.env`)
- `gpu_count` — Number of GPUs used in parallel

## Rates by GPU Type

Configure these in your backend `.env`:

```bash
COST_H100_USD_PER_HR=4.50    # H100 at $4.50/hour
COST_A100_USD_PER_HR=2.20    # A100 at $2.20/hour
```

If you use other GPU types, add them:

```bash
COST_L40S_USD_PER_HR=0.75
COST_RTX4090_USD_PER_HR=0.40
```

The backend uses these rates at run completion to finalize `cost_usd`.

## Example

**Run configuration:**
- GPU type: H100
- GPU count: 8
- Actual training time: 7,200 seconds (2 hours)

**Cost calculation:**

```
cost_usd = (7200 / 3600) × 4.50 × 8
         = 2 × 4.50 × 8
         = $72.00
```

The run's `cost_usd` field will be set to `72.0` when Tinker reports completion.

---

## Cost Tracking API

### Get Run Cost

```bash
curl -X GET http://localhost:8000/v1/runs/{id} \
  -H "Authorization: Bearer AGENT_TOKEN"
```

Response includes:

```json
{
  "id": "run_abc123",
  "gpu_type": "H100",
  "gpu_count": 8,
  "gpu_seconds": 7200.0,
  "cost_usd": 72.0,
  "status": "succeeded",
  "finished_at": "2026-04-30T14:30:00Z"
}
```

### List Runs with Costs

```bash
curl -X GET http://localhost:8000/v1/runs \
  -H "Authorization: Bearer AGENT_TOKEN"
```

Response includes all your runs (or all runs if you're admin):

```json
[
  {
    "id": "run_abc123",
    "agent_id": "claude_code",
    "cost_usd": 72.0,
    "status": "succeeded"
  },
  {
    "id": "run_def456",
    "agent_id": "claude_code",
    "cost_usd": 18.5,
    "status": "running"
  }
]
```

### Total Cost by Agent

```bash
# Via API (TODO: implement aggregation endpoint)
# Workaround: fetch all runs and sum by agent_id
curl -X GET http://localhost:8000/v1/runs?agent_id=claude_code \
  -H "Authorization: Bearer ADMIN_TOKEN" \
  | jq '[.[] | .cost_usd] | add'
```

---

## Budget Management

### Setting a Budget Alert (TODO)

In a future release, agents will be able to set per-agent budget limits:

```bash
# Planned endpoint (not yet implemented)
curl -X POST http://localhost:8000/v1/budgets \
  -H "Authorization: Bearer AGENT_TOKEN" \
  -d '{
    "agent_id": "claude_code",
    "monthly_limit_usd": 10000,
    "alert_at_usd": 8000
  }'
```

When the agent's cumulative cost hits $8,000, the backend will:

1. Log a warning
2. Send a notification (Slack, email, etc.)
3. Optionally pause new runs (configurable)

### Manual Budget Enforcement (for now)

Monitor costs manually by fetching all runs:

```bash
# Get all runs for an agent
curl -X GET http://localhost:8000/v1/runs?agent_id=claude_code \
  -H "Authorization: Bearer ADMIN_TOKEN" \
  | jq 'map(.cost_usd) | add'
```

If costs exceed your threshold, cancel high-cost runs:

```bash
curl -X POST http://localhost:8000/v1/runs/{id}/cancel \
  -H "Authorization: Bearer OWNER_TOKEN"
```

---

## Cost Transparency

Every run is a document of spend. The dashboard (`/runs`) shows:

- Run ID, agent, status
- GPU type, count, duration
- Final cost in USD
- User goal and agent plan (for context on why the spend occurred)

This creates accountability: agents must justify their training runs, and humans can audit spending by agent, date, and methodology.

---

## Next Steps

- See [runs.md](runs.md) for run fields and how to estimate cost before launching
- See [examples/](../examples/) for cost breakdowns of real training scenarios
