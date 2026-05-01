"""Locust load-test scenario for the Stellarator backend.

Usage::

    STELLARATOR_LOAD_TOKEN=<token> locust -f tests/load/locustfile.py \
        --host http://localhost:8000 --users 50 --spawn-rate 5 --run-time 60s

See tests/load/README.md for full instructions and expected throughput baselines.

Threshold-aware exit hook
-------------------------
When run with ``--headless``, the ``quitting`` event reads
``tests/load/thresholds.yaml`` and sets ``environment.process_exit_code = 1``
if any per-endpoint or global threshold is breached.  The CI workflow relies
on this exit code to fail the job.
"""

from __future__ import annotations

import os
import pathlib
from typing import Any

import yaml
from locust import HttpUser, between, events, task
from locust.env import Environment
from locust.runners import MasterRunner, WorkerRunner

_TOKEN: str = os.environ.get("STELLARATOR_LOAD_TOKEN", "")

_VALID_RUN_PAYLOAD: dict = {
    "agent_id": "load-test-agent",
    "model": "gpt-4o-mini",
    "dataset": "load-test-dataset",
    "hyperparameters": {
        "epochs": 1,
        "batch_size": 4,
        "learning_rate": 1e-4,
    },
}

_TEST_KEY_PAYLOAD: dict = {"key": "load-test-dummy-key-not-real"}

# Resolved once at import time so the hook never interpolates user-supplied paths.
_THRESHOLDS_PATH = (
    pathlib.Path(__file__).parent / "thresholds.yaml"
).resolve()


def _load_thresholds() -> dict[str, Any]:
    """Parse thresholds.yaml; return empty dict if file is absent."""
    if not _THRESHOLDS_PATH.exists():
        return {}
    with _THRESHOLDS_PATH.open("r", encoding="utf-8") as fh:
        return yaml.safe_load(fh) or {}


class StellaratorUser(HttpUser):
    """Simulates a typical agent workload against the Stellarator API."""

    wait_time = between(0.5, 2.0)

    def on_start(self) -> None:
        if not _TOKEN:
            raise ValueError(
                "STELLARATOR_LOAD_TOKEN environment variable is not set. "
                "Set it to a valid bearer token before running the load test."
            )
        self.headers = {"Authorization": f"Bearer {_TOKEN}"}

    # ------------------------------------------------------------------
    # Tasks
    # ------------------------------------------------------------------

    @task(10)
    def get_whoami(self) -> None:
        """Lightweight identity check — highest weight (10x)."""
        self.client.get("/v1/whoami", headers=self.headers, name="GET /v1/whoami")

    @task(5)
    def list_runs(self) -> None:
        """List runs for the authenticated agent — medium weight (5x)."""
        self.client.get("/v1/runs", headers=self.headers, name="GET /v1/runs")

    @task(1)
    def create_run(self) -> None:
        """Submit a new training run — lowest weight (1x).

        The backend will reject requests that reference a non-existent dataset
        or agent; that is acceptable in a load-test context — we measure
        throughput and error rates, not successful training outcomes.
        """
        self.client.post(
            "/v1/runs",
            json=_VALID_RUN_PAYLOAD,
            headers=self.headers,
            name="POST /v1/runs",
        )

    @task(1)
    def test_integration_key(self) -> None:
        """Hit the rate-limited key-test endpoint.

        At sufficient concurrency this will return HTTP 429 — that is the
        expected behaviour and confirms the rate limiter is active.
        """
        self.client.post(
            "/v1/integrations/keys/tinker/test",
            json=_TEST_KEY_PAYLOAD,
            headers=self.headers,
            name="POST /v1/integrations/keys/{kind}/test",
            # 429 is expected under load; don't count as a failure.
            catch_response=True,
        ).result()  # type: ignore[attr-defined]


# ---------------------------------------------------------------------------
# Threshold-aware quitting hook
# ---------------------------------------------------------------------------

@events.quitting.add_listener
def _check_thresholds(environment: Environment, **_kwargs: Any) -> None:
    """Evaluate per-endpoint and global thresholds; set exit code 1 on breach.

    Only active in the master process (or single-process headless runs).
    Worker processes do not have aggregated stats and are skipped.
    """
    if isinstance(environment.runner, WorkerRunner):
        return  # stats are aggregated on master only

    thresholds = _load_thresholds()
    if not thresholds:
        print("[thresholds] thresholds.yaml not found — skipping threshold check")
        return

    endpoint_thresholds: dict[str, dict[str, float]] = thresholds.get("endpoints", {})
    global_thresholds: dict[str, float] = thresholds.get("global", {})

    breaches: list[str] = []

    # --- Per-endpoint checks ---
    stats = environment.runner.stats if environment.runner else environment.stats
    for name, limits in endpoint_thresholds.items():
        entry = stats.entries.get((name, "GET")) or stats.entries.get((name, "POST")) or None
        # Try to find by the name string regardless of method.
        if entry is None:
            for (entry_name, _method), entry_obj in stats.entries.items():
                if entry_name == name:
                    entry = entry_obj
                    break

        if entry is None:
            print(f"[thresholds] WARN  no stats for '{name}' — skipping")
            continue

        total_reqs = entry.num_requests + entry.num_failures
        err_rate = entry.num_failures / total_reqs if total_reqs > 0 else 0.0
        # Locust stores response times in milliseconds.
        p95_ms = entry.get_response_time_percentile(0.95)

        max_p95 = limits.get("p95_ms")
        max_err = limits.get("error_rate")

        status_parts: list[str] = []
        breach = False

        if max_p95 is not None:
            ok = p95_ms <= max_p95
            status_parts.append(f"p95={p95_ms:.0f}ms limit={max_p95}ms {'OK' if ok else 'FAIL'}")
            if not ok:
                breach = True

        if max_err is not None:
            ok = err_rate <= max_err
            status_parts.append(
                f"err_rate={err_rate:.3f} limit={max_err:.3f} {'OK' if ok else 'FAIL'}"
            )
            if not ok:
                breach = True

        verdict = "FAIL" if breach else "PASS"
        print(f"[thresholds] {verdict}  {name}: {', '.join(status_parts)}")
        if breach:
            breaches.append(name)

    # --- Global RPS check ---
    min_rps = global_thresholds.get("min_rps")
    if min_rps is not None:
        total_stats = stats.total
        elapsed = total_stats.use_response_times_cache and total_stats.total_response_time
        # Use the aggregated RPS from Locust's stats object.
        actual_rps = total_stats.current_rps if hasattr(total_stats, "current_rps") else 0.0
        # Fall back to total_reqs / elapsed if current_rps unavailable.
        if actual_rps == 0.0 and total_stats.num_requests > 0:
            actual_rps = total_stats.total_rps if hasattr(total_stats, "total_rps") else 0.0

        ok = actual_rps >= min_rps
        print(
            f"[thresholds] {'PASS' if ok else 'FAIL'}  global: "
            f"rps={actual_rps:.1f} limit={min_rps:.1f}"
        )
        if not ok:
            breaches.append("global.min_rps")

    # --- Summary ---
    if breaches:
        print(
            f"\n[thresholds] FAIL — {len(breaches)} threshold(s) breached: "
            + ", ".join(breaches)
        )
        environment.process_exit_code = 1
    else:
        print("\n[thresholds] PASS — all thresholds satisfied")
