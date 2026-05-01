# Load Tests

Locust-based load scenario for the Stellarator backend.

## Prerequisites

```bash
pip install locust
```

## Running

```bash
export STELLARATOR_LOAD_TOKEN=<agent-bearer-token>

# Headless — 50 concurrent users, 5/s ramp, 60 second run
locust -f tests/load/locustfile.py \
    --host http://localhost:8000 \
    --users 50 --spawn-rate 5 --run-time 60s --headless

# Interactive web UI (http://localhost:8089)
locust -f tests/load/locustfile.py --host http://localhost:8000
```

## Task Weights and Expected Behaviour

| Task | Weight | Expected outcome |
|---|---|---|
| `GET /v1/whoami` | 10 | 200 under all load levels |
| `GET /v1/runs` | 5 | 200; slight latency increase at > 100 RPS |
| `POST /v1/runs` | 1 | 201 or 422 (validation); never 5xx |
| `POST /v1/integrations/keys/{kind}/test` | 1 | 429 at scale (rate limiter: 10/min/IP) |

## Throughput Baseline

Measured on a single-core local dev machine (M-series Mac, SQLite on SSD):

| Metric | Expected |
|---|---|
| Peak RPS (50 users) | ≥ 200 req/s |
| `GET /v1/whoami` p99 latency | < 20 ms |
| `GET /v1/runs` p99 latency | < 50 ms |
| `POST /v1/runs` p99 latency | < 200 ms |
| Error rate (excl. 429) | < 0.1 % |

These baselines assume the backend is running with `uvicorn --workers 1` and an empty
SQLite database. Adjust expectations for loaded databases or networked deployments.
