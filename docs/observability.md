# Observability

Stellarator exposes metrics via Prometheus, distributed traces via OpenTelemetry, and
structured logs with trace correlation.

---

## Metrics Catalog

All metrics are exported at `GET /metrics` (Prometheus text format, port 8000).
The `prometheus-fastapi-instrumentator` library instruments all HTTP handlers automatically.
Additional application metrics are registered manually.

| Metric | Type | Labels | Description |
|---|---|---|---|
| `http_requests_total` | Counter | `method`, `handler`, `status` | Total HTTP requests, auto-instrumented by fastapi-instrumentator |
| `http_request_duration_seconds` | Histogram | `method`, `handler` | Full request latency including DB I/O |
| `tinker_request_duration_seconds` | Histogram | `agent_id`, `operation` | Latency of outbound Tinker API calls |
| `run_active_count` | Gauge | `agent_id` | Number of runs currently in non-terminal state |
| `run_cost_usd_total` | Counter | `agent_id`, `model` | Cumulative USD cost of completed runs |
| `ws_active_connections` | Gauge | — | Active WebSocket connections (chat + run streaming) |
| `oauth_state_consume_total` | Counter | `provider`, `result` (ok / replay / expired / agent_mismatch) | OAuth state consumption outcomes |

Supervisor metrics are exported at `supervisor:8001/metrics` and scraped by the same
Prometheus instance (see `docker/prometheus.yml`).

---

## Distributed Tracing

`backend/app/core/tracing.py` initialises the OpenTelemetry SDK on startup when
`OTEL_EXPORTER_OTLP_ENDPOINT` is set. The module is imported via the existing defensive
`try/except ImportError` pattern in `app/main.py`, so the backend starts normally when
the OTel SDK is not installed.

**Instrumented layers**:
- FastAPI (all HTTP spans via `opentelemetry-instrumentation-fastapi`)
- SQLAlchemy (all query spans via `opentelemetry-instrumentation-sqlalchemy`)
- HTTPX (all outbound HTTP spans via `opentelemetry-instrumentation-httpx`)

**Supervisor integration**: the Rust supervisor attaches `traceparent` and `tracestate` W3C
headers to its `POST /v1/runs/{id}/track` callback so that supervisor work is a child span of
the originating run span.

**SSE chunk tagging**: each SSE event carries a `trace_id` field in its JSON envelope when
tracing is active, enabling correlation between client-visible stream events and backend spans.

### OTLP Endpoint Configuration

```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4318/v1/traces
```

If the variable is unset, tracing is silently disabled. The default value targets the
`otel-collector` service defined in `docker-compose.yml`.

---

## Log Correlation

All log records emitted while handling a traced request are enriched with:

- `trace_id` — the current OpenTelemetry trace ID (hex string)
- `span_id` — the current span ID
- `run_id` — present when the log is emitted within a run context

This allows joining log lines to traces in any log aggregation system that ingests structured
JSON logs.

---

## Grafana Dashboards

A starter dashboard is provisioned at `docker/grafana/dashboards/stellarator.json`.
It is auto-loaded by Grafana's provisioning mechanism (configured in
`docker/grafana/provisioning/datasources/prometheus.yml`).

**Panels**:
1. **HTTP Request Rate** — `rate(http_requests_total[1m])` by handler and status code.
2. **Tinker Request Latency p99** — `histogram_quantile(0.99, rate(tinker_request_duration_seconds_bucket[5m]))` by agent.
3. **Active Runs** — current gauge of `run_active_count` by agent, displayed as a stat panel.
4. **Cost Burndown by Agent** — `increase(run_cost_usd_total[1h])` per agent as a time-series.

---

## Proposed SLOs

| SLO | Target | Measurement |
|---|---|---|
| Availability — `POST /v1/runs` | 99.9% success rate over 30 days | `rate(http_requests_total{handler="/v1/runs",status=~"5.."}[30d]) / rate(http_requests_total{handler="/v1/runs"}[30d]) < 0.001` |
| Latency — `POST /v1/runs` p99 | ≤ 500 ms | `histogram_quantile(0.99, rate(http_request_duration_seconds_bucket{handler="/v1/runs"}[5m])) < 0.5` |
| Cost accuracy | Reported cost within ±5% of projected | Evaluated per run via `run_cost_usd_total`; alert on deviation > 5% over trailing 7 days |
