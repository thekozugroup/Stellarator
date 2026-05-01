Stellarator is a control plane for fine-tuning and reinforcement learning workloads on [Tinker](https://thinkingmachines.ai/tinker/). Operators launch sandbox runs, validate them through a structured pre-flight gate, then promote to scale — with cost projections, budgets, and live alert streams threaded through every step.

## Screenshots

![Stellarator dashboard](./docs/screenshot.png)

## How it works

Each run is owned by the identity that created it. Other identities can read the metrics, alerts, and notes but cannot mutate the run — pause, cancel, edit hyperparameters all require ownership. This makes the platform safe for multi-operator workspaces where several pipelines may share a Tinker organization.

Runs cannot reach scale GPUs without first passing through a sandbox. The sandbox is a small, cheap smoke test (CPU or single A10G, capped step count). Its result feeds a structured pre-flight payload — model, method, dataset mixture, hyperparameters, projected cost, citations — which the server validates before accepting any production submission. Stale lineage (over 24 hours) or missing fields are rejected with HTTP 412 and an actionable hint.

Training scripts emit trackio-style alerts (`error`, `warn`, `info`) that surface in real time on the run detail page and trigger toasts via a server-sent event stream. A high-concurrency Rust supervisor handles per-job polling, metric ingestion, cost computation, and websocket fan-out — keeping the orchestration layer free from per-tick work.

A research subsystem fetches Hugging Face papers, arXiv records, and GitHub code examples on demand so methodology choices can be cited per run, with a global research log auditing every lookup.

## Stack

- FastAPI + SQLAlchemy + SQLite (WAL) with Alembic migrations
- Rust supervisor: axum, tokio, sqlx, subtle (constant-time auth)
- Next.js 15 + Tailwind v4 + shadcn/ui (App Router, dark mode default)
- MultiFernet encryption for OAuth tokens and per-identity provider keys
- OpenTelemetry instrumentation, Prometheus + Grafana dashboards
- Locust load-regression gating in CI
- Tinker REST surface for training job orchestration

## Status

Active
