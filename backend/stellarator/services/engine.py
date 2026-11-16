"""Run engine: one task per running run, a dynamic worker pool, and a single batching DB writer.

Public interface:
- engine_registry.active_runs() -> list of RunTask (each has .run_id, .endpoint_id, .prompt_set_id)
- await engine_registry.recover()    on startup: interrupted runs become paused, in-flight samples pending
- await engine_registry.shutdown()   on shutdown: stop workers, samples in flight back to pending
- await engine_registry.start(run_id)            the run row must already say status "running"
- await engine_registry.pause / resume / retry_failed / stop(run_id)
- engine_registry.set_concurrency(run_id, n)     applies live
- engine_registry.live(run_id), in_flight(run_id), unsaved_active_seconds(run_id)

How a run moves:
- Workers take pending samples (in prompt order) from an in-memory buffer refilled from the DB, send
  them, and hand the result to the writer. There are always `concurrency` workers while the run is
  running, so `concurrency` requests are in flight. Raising it spawns workers at once; lowering it
  lets the extra workers finish their current request and exit.
- The writer commits batches (every 250 ms or 200 items) in one transaction: samples marked running
  when dispatched, finished samples, run counters, active seconds and metric points.
- Pause: status "pausing", no new requests, in-flight ones finish, then "paused".
- When nothing is pending or in flight the run is "completed".
"""

from __future__ import annotations

import asyncio
import logging
import time
from collections import deque
from dataclasses import dataclass, field
from itertools import islice
from typing import Any

import httpx
from sqlalchemy import Float, Integer, bindparam, func, select, update
from sqlalchemy.sql import insert

from ..db import SessionLocal
from ..db import engine as db_engine
from ..models import (
    RUN_COMPLETED,
    RUN_PAUSED,
    RUN_PAUSING,
    RUN_RUNNING,
    SAMPLE_DONE,
    SAMPLE_FAILED,
    SAMPLE_PENDING,
    SAMPLE_RUNNING,
    Endpoint,
    Prompt,
    Run,
    RunMetric,
    Sample,
    utcnow,
)
from ..schemas import LiveSequence, RunParams
from .generation import (
    GenerationFailed,
    GenerationResult,
    Progress,
    ServerCaps,
    generate,
    last_user_text,
    prepare_messages,
)
from .openai_compat import api_root, build_headers

log = logging.getLogger("stellarator.engine")

RESTART_REASON = "Paused because Stellarator restarted. Resume to continue."
ENDPOINT_GONE = (
    "The endpoint this run used was deleted, so the run cannot continue. Start a new run on another endpoint."
)
METRIC_INTERVAL_S = 5.0
WRITE_INTERVAL_S = 0.25
WRITE_BATCH = 200
LIVE_SEQUENCES = 64
PREVIEW_CHARS = 160
BUCKET_KEEP_S = 120


class EngineConflict(Exception):
    """The action does not fit the run's current state. The message is one sentence for the user."""


class _Interrupted(Exception):
    """A sample waiting to retry was stopped by a pause; it goes back to pending."""


def preview(text: str | None, limit: int = PREVIEW_CHARS) -> str:
    text = " ".join((text or "").split())
    return text if len(text) <= limit else text[: limit - 1].rstrip() + "…"


# --------------------------------------------------------------------------- writer

_S = Sample.__table__
_R = Run.__table__

_FINISH_STMT = (
    _S.update()
    .where(_S.c.id == bindparam("b_id"))
    .values(
        status=bindparam("b_status"),
        reasoning=bindparam("b_reasoning"),
        response=bindparam("b_response"),
        finish_reason=bindparam("b_finish_reason"),
        prompt_tokens=bindparam("b_prompt_tokens"),
        completion_tokens=bindparam("b_completion_tokens"),
        reasoning_tokens=bindparam("b_reasoning_tokens"),
        latency_ms=bindparam("b_latency_ms"),
        ttft_ms=bindparam("b_ttft_ms"),
        attempts=bindparam("b_attempts"),
        error=bindparam("b_error"),
        completed_at=bindparam("b_completed_at"),
    )
)

_RUN_DELTA_STMT = (
    _R.update()
    .where(_R.c.id == bindparam("b_id"))
    .values(
        done=_R.c.done + bindparam("b_done", type_=Integer),
        failed=_R.c.failed + bindparam("b_failed", type_=Integer),
        truncated=_R.c.truncated + bindparam("b_truncated", type_=Integer),
        prompt_tokens=_R.c.prompt_tokens + bindparam("b_prompt_tokens", type_=Integer),
        completion_tokens=_R.c.completion_tokens + bindparam("b_completion_tokens", type_=Integer),
        reasoning_tokens=_R.c.reasoning_tokens + bindparam("b_reasoning_tokens", type_=Integer),
        active_seconds=_R.c.active_seconds + bindparam("b_active", type_=Float),
    )
)

_DELTA_KEYS = ("done", "failed", "truncated", "prompt_tokens", "completion_tokens", "reasoning_tokens", "active")


@dataclass
class _Op:
    kind: str  # mark | finish | reset | active | metric | flush
    run_id: int = 0
    sample_id: int = 0
    task: RunTask | None = None
    values: dict[str, Any] | None = None
    deltas: dict[str, float] | None = None
    future: asyncio.Future | None = None


class Writer:
    """The single coroutine that writes engine results to the DB, in batches."""

    def __init__(self) -> None:
        self.loop = asyncio.get_running_loop()
        self.queue: asyncio.Queue[_Op] = asyncio.Queue()
        self.task = asyncio.create_task(self._loop(), name="stellarator-writer")

    # ---- producers (sync, never block)

    def mark_running(self, run_id: int, sample_id: int) -> None:
        self.queue.put_nowait(_Op("mark", run_id, sample_id))

    def finish(self, task: RunTask, sample_id: int, values: dict[str, Any], deltas: dict[str, float]) -> None:
        self.queue.put_nowait(_Op("finish", task.run_id, sample_id, task, values, deltas))

    def reset(self, task: RunTask, sample_id: int) -> None:
        self.queue.put_nowait(_Op("reset", task.run_id, sample_id, task))

    def active(self, run_id: int, seconds: float) -> None:
        if seconds > 0:
            self.queue.put_nowait(_Op("active", run_id, deltas={"active": seconds}))

    def metric(self, run_id: int, values: dict[str, Any]) -> None:
        self.queue.put_nowait(_Op("metric", run_id, values=values))

    async def flush(self) -> None:
        """Wait until everything queued so far is committed."""
        if self.task.done():
            return
        fut = self.loop.create_future()
        self.queue.put_nowait(_Op("flush", future=fut))
        await fut

    async def stop(self) -> None:
        try:
            await self.flush()
        finally:
            self.task.cancel()
            try:
                await self.task
            except (asyncio.CancelledError, Exception):
                pass

    # ---- the loop

    async def _loop(self) -> None:
        try:
            await self._run()
        finally:
            # Never leave a flush() waiting on a writer that is gone.
            while not self.queue.empty():
                op = self.queue.get_nowait()
                if op.kind == "flush" and op.future is not None and not op.future.done():
                    op.future.set_result(None)

    async def _run(self) -> None:
        while True:
            first = await self.queue.get()
            batch = [first]
            if first.kind != "flush":
                deadline = time.monotonic() + WRITE_INTERVAL_S
                while len(batch) < WRITE_BATCH and batch[-1].kind != "flush":
                    try:
                        batch.append(self.queue.get_nowait())
                        continue
                    except asyncio.QueueEmpty:
                        pass
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        break
                    try:
                        batch.append(await asyncio.wait_for(self.queue.get(), remaining))
                    except TimeoutError:
                        break
            try:
                await self._commit(batch)
            except Exception:  # pragma: no cover - logged, never kills the writer
                log.exception("Writer failed to commit a batch")
            finally:
                self._after(batch)

    async def _commit(self, batch: list[_Op]) -> None:
        if all(op.kind == "flush" for op in batch):
            return
        try:
            await self._write(batch)
        except Exception:
            log.exception("Writer batch failed; retrying run by run")
            by_run: dict[int, list[_Op]] = {}
            for op in batch:
                if op.kind != "flush":
                    by_run.setdefault(op.run_id, []).append(op)
            for run_id, ops in by_run.items():
                try:
                    await self._write(ops)
                except Exception:
                    log.exception("Dropped %d engine writes for run %s", len(ops), run_id)

    async def _write(self, batch: list[_Op]) -> None:
        marks: list[int] = []
        finishes: list[dict[str, Any]] = []
        resets: list[int] = []
        metrics: list[dict[str, Any]] = []
        deltas: dict[int, dict[str, float]] = {}
        for op in batch:
            if op.kind == "mark":
                marks.append(op.sample_id)
            elif op.kind == "finish":
                finishes.append(op.values or {})
            elif op.kind == "reset":
                resets.append(op.sample_id)
            elif op.kind == "metric":
                metrics.append(op.values or {})
            if op.deltas:
                d = deltas.setdefault(op.run_id, dict.fromkeys(_DELTA_KEYS, 0))
                for k, v in op.deltas.items():
                    d[k] += v
        async with db_engine.begin() as conn:
            for i in range(0, len(marks), 500):
                chunk = marks[i : i + 500]
                await conn.execute(
                    _S.update()
                    .where(_S.c.id.in_(chunk), _S.c.status == SAMPLE_PENDING)
                    .values(status=SAMPLE_RUNNING)
                )
            if finishes:
                await conn.execute(_FINISH_STMT, finishes)
            for i in range(0, len(resets), 500):
                chunk = resets[i : i + 500]
                await conn.execute(
                    _S.update()
                    .where(_S.c.id.in_(chunk), _S.c.status == SAMPLE_RUNNING)
                    .values(status=SAMPLE_PENDING)
                )
            if deltas:
                rows = [
                    {"b_id": run_id, **{f"b_{k}": (v if k == "active" else int(v)) for k, v in d.items()}}
                    for run_id, d in deltas.items()
                ]
                await conn.execute(_RUN_DELTA_STMT, rows)
            if metrics:
                await conn.execute(insert(RunMetric.__table__), metrics)

    def _after(self, batch: list[_Op]) -> None:
        for op in batch:
            if op.kind == "finish" and op.task is not None:
                op.task._flushed(op.sample_id, finished=True)
            elif op.kind == "reset" and op.task is not None:
                op.task._flushed(op.sample_id, finished=False)
            elif op.kind == "flush" and op.future is not None and not op.future.done():
                op.future.set_result(None)


# --------------------------------------------------------------------------- one run


@dataclass
class _Item:
    sample_id: int
    prompt_idx: int
    sample_idx: int
    messages: list[dict[str, Any]]


@dataclass
class _Seq:
    item: _Item
    started: float
    progress: Progress = field(default_factory=Progress)


class RunTask:
    """The task driving one running (or pausing) run."""

    def __init__(self, registry: EngineRegistry, run: Run, endpoint: Endpoint) -> None:
        self.registry = registry
        self.writer = registry.writer
        self.run_id: int = run.id
        self.endpoint_id: int | None = run.endpoint_id
        self.prompt_set_id: int = run.prompt_set_id
        self.model = run.model
        self.params = RunParams.model_validate(run.params or {})
        self.concurrency = max(1, int(run.concurrency or 1))
        self.total = int(run.total or 0)
        self.done = int(run.done or 0)
        self.failed = int(run.failed or 0)
        self.status = RUN_RUNNING
        self.reason: str | None = None
        self.url = api_root(endpoint.base_url) + "/chat/completions"
        self.headers = build_headers(endpoint.api_key, endpoint.headers or {})
        self.caps = ServerCaps()
        self.client: httpx.AsyncClient | None = None

        self.lock = asyncio.Lock()  # state transitions (pause, resume, retry, finalize)
        self.refill_lock = asyncio.Lock()
        self.workers: set[asyncio.Task] = set()
        self.idle = asyncio.Event()
        self.halt = asyncio.Event()  # set while pausing: samples waiting to retry give up their slot
        self.in_flight: dict[int, _Seq] = {}
        self.buffer: deque[_Item] = deque()
        self.claimed: set[int] = set()  # buffered, in flight, or finished but not yet committed
        self.cursor = 0
        self.exhausted = False
        self.finalizing = False
        self.cancelled = False
        self.unflushed = 0  # finished samples not yet committed
        self.dispatched = 0  # samples sent by this task
        self.consecutive_failures = 0
        self.last_error: str | None = None

        self.started = time.monotonic()
        self.active_mark = self.started
        self.buckets: deque[list[float]] = deque()  # [second, tokens, samples]
        self.main_task: asyncio.Task | None = None
        self.metrics_task: asyncio.Task | None = None

    # ---- lifecycle

    def start(self) -> None:
        self.main_task = asyncio.create_task(self._main(), name=f"run-{self.run_id}")

    async def wait(self) -> None:
        if self.main_task is not None and not self.main_task.done():
            await asyncio.wait({self.main_task})

    async def stop(self) -> None:
        """Stop now: cancel in-flight requests (their samples go back to pending)."""
        self.cancelled = True
        self._release_buffer()
        for w in list(self.workers):
            w.cancel()
        await self.wait()

    async def _main(self) -> None:
        final: str | None = None
        self.client = httpx.AsyncClient(
            headers=self.headers,
            timeout=httpx.Timeout(
                self.params.request_timeout_s,
                connect=min(30.0, self.params.request_timeout_s),
                pool=None,
            ),
            limits=httpx.Limits(max_connections=None, max_keepalive_connections=None),
        )
        rescanned_at = -1  # value of self.dispatched when we last rescanned for leftovers
        try:
            self._write_metric()
            self.metrics_task = asyncio.create_task(self._metrics_loop())
            while True:
                self._spawn()
                while self.workers:
                    self.idle.clear()
                    await self.idle.wait()
                if self.cancelled:
                    break
                await self.writer.flush()
                async with self.lock:
                    if self.cancelled:
                        break
                    if self.workers:
                        continue  # resumed or respawned while we flushed
                    if self.status == RUN_PAUSING:
                        self.finalizing = True
                        final = RUN_PAUSED
                        break
                    pending = await self._count_pending()
                    if pending and self.dispatched != rescanned_at:
                        # Pending samples behind the cursor (e.g. returned while pausing): scan again.
                        # Stop when a full pass dispatched nothing, so this can never spin.
                        rescanned_at = self.dispatched
                        self._rescan()
                        continue
                    self.finalizing = True
                    final = RUN_COMPLETED
                    break
        except asyncio.CancelledError:
            self.cancelled = True
            raise
        except Exception as exc:
            log.exception("Run %s stopped on an internal error", self.run_id)
            final = RUN_PAUSED
            self.reason = f"Paused because of an internal error: {exc}. Resume to try again."
        finally:
            self.finalizing = True
            await self._wind_down(final)

    async def _wind_down(self, final: str | None) -> None:
        if self.metrics_task is not None:
            self.metrics_task.cancel()
        if self.workers:
            for w in list(self.workers):
                w.cancel()
            await asyncio.gather(*list(self.workers), return_exceptions=True)
        self._release_buffer()
        if self.client is not None:
            try:
                await self.client.aclose()
            except Exception:  # pragma: no cover
                pass
        self._record_active()
        self._write_metric(active=0)
        try:
            await self.writer.flush()
            if final is not None and not self.cancelled:
                values: dict[str, Any] = {"status": final}
                if final == RUN_COMPLETED:
                    values.update(finished_at=utcnow(), status_reason=None)
                else:
                    values.update(status_reason=self.reason)
                await _update_run(self.run_id, **values)
        except Exception:  # pragma: no cover
            log.exception("Could not save the final state of run %s", self.run_id)
        finally:
            self.registry._remove(self)

    # ---- state changes (call with self.lock held)

    async def _pause_locked(self, reason: str | None) -> None:
        if self.status != RUN_RUNNING or self.finalizing or self.cancelled:
            return
        self.status = RUN_PAUSING
        self.reason = reason
        self.halt.set()
        self._release_buffer()
        await _update_run(self.run_id, status=RUN_PAUSING, status_reason=reason)

    async def _resume_locked(self) -> None:
        if self.finalizing or self.cancelled:
            return
        self.status = RUN_RUNNING
        self.reason = None
        self.halt.clear()
        self.consecutive_failures = 0
        self._rescan()  # samples interrupted while pausing went back to pending behind the cursor
        await _update_run(self.run_id, status=RUN_RUNNING, status_reason=None, finished_at=None)
        self._spawn()

    def set_concurrency(self, n: int) -> None:
        self.concurrency = max(1, int(n))
        if self.status == RUN_RUNNING and not self.finalizing and not self.cancelled:
            self._spawn()
            self._write_metric()

    def _rescan(self) -> None:
        self.cursor = 0
        self.exhausted = False

    def _release_buffer(self) -> None:
        for item in self.buffer:
            self.claimed.discard(item.sample_id)
        self.buffer.clear()

    # ---- workers

    def _spawn(self) -> None:
        if self.status != RUN_RUNNING or self.finalizing or self.cancelled:
            return
        while len(self.workers) < self.concurrency:
            w = asyncio.create_task(self._worker())
            self.workers.add(w)
            self.idle.clear()

    async def _worker(self) -> None:
        me = asyncio.current_task()
        try:
            while True:
                if self.cancelled or self.status != RUN_RUNNING or len(self.workers) > self.concurrency:
                    break
                item = await self._next()
                if item is None:
                    break
                if self.cancelled or self.status != RUN_RUNNING:
                    self.claimed.discard(item.sample_id)
                    break
                await self._process(item)
        except asyncio.CancelledError:
            pass
        except Exception as exc:
            log.exception("Worker for run %s failed", self.run_id)
            async with self.lock:
                await self._pause_locked(f"Paused because of an internal error: {exc}. Resume to try again.")
        finally:
            self.workers.discard(me)  # type: ignore[arg-type]
            if not self.workers:
                self.idle.set()

    async def _next(self) -> _Item | None:
        while True:
            if self.buffer:
                return self.buffer.popleft()
            if self.exhausted or self.cancelled or self.status != RUN_RUNNING:
                return None
            async with self.refill_lock:
                if self.buffer or self.exhausted or self.cancelled or self.status != RUN_RUNNING:
                    continue
                await self._refill()

    async def _refill(self) -> None:
        limit = min(max(256, self.concurrency * 2), 4096)
        async with SessionLocal() as session:
            rows = (
                await session.execute(
                    select(Sample.id, Sample.prompt_idx, Sample.sample_idx, Prompt.messages)
                    .join(Prompt, Prompt.id == Sample.prompt_id)
                    .where(
                        Sample.run_id == self.run_id,
                        Sample.status == SAMPLE_PENDING,
                        Sample.id > self.cursor,
                    )
                    .order_by(Sample.id)  # ids follow (prompt_idx, sample_idx): see api/runs.py
                    .limit(limit)
                )
            ).all()
        if not rows:
            self.exhausted = True
            return
        self.cursor = rows[-1][0]
        for sid, pidx, sidx, messages in rows:
            if sid in self.claimed:
                continue
            self.claimed.add(sid)
            self.buffer.append(_Item(sid, pidx, sidx, messages or []))

    async def _process(self, item: _Item) -> None:
        seq = _Seq(item, time.monotonic())
        self.dispatched += 1
        self.in_flight[item.sample_id] = seq
        self.writer.mark_running(self.run_id, item.sample_id)
        trip: str | None = None
        try:
            messages = prepare_messages(item.messages, self.params)
            result = await generate(
                self.client,  # type: ignore[arg-type]
                self.url,
                self.model,
                messages,
                self.params,
                item.sample_idx,
                caps=self.caps,
                progress=seq.progress,
                on_progress=self._on_progress,
                sleep=self._backoff,
            )
        except GenerationFailed as exc:
            trip = self._finish_failed(item, exc.message, exc.attempts)
        except _Interrupted:
            self.in_flight.pop(item.sample_id, None)
            self.writer.reset(self, item.sample_id)
        except asyncio.CancelledError:
            self.in_flight.pop(item.sample_id, None)
            self.writer.reset(self, item.sample_id)
            raise
        except Exception as exc:
            log.exception("Sample %s failed on an internal error", item.sample_id)
            trip = self._finish_failed(
                item, f"Stellarator hit an internal error on this sample: {exc}.", seq.progress.attempt
            )
        else:
            self._finish_done(item, result)
        finally:
            self.in_flight.pop(item.sample_id, None)
        if trip is not None:
            async with self.lock:
                await self._pause_locked(trip)

    async def _backoff(self, delay: float) -> None:
        """Wait before a retry, unless the run starts pausing: then give the slot up."""
        if self.status != RUN_RUNNING:
            raise _Interrupted
        try:
            await asyncio.wait_for(self.halt.wait(), delay)
        except TimeoutError:
            return
        raise _Interrupted

    def _finish_done(self, item: _Item, r: GenerationResult) -> None:
        truncated = r.finish_reason == "length"
        values = {
            "b_id": item.sample_id,
            "b_status": SAMPLE_DONE,
            "b_reasoning": r.reasoning,
            "b_response": r.response,
            "b_finish_reason": (r.finish_reason or None) and r.finish_reason[:40],
            "b_prompt_tokens": r.prompt_tokens,
            "b_completion_tokens": r.completion_tokens,
            "b_reasoning_tokens": r.reasoning_tokens,
            "b_latency_ms": r.latency_ms,
            "b_ttft_ms": r.ttft_ms,
            "b_attempts": r.attempts,
            "b_error": None,
            "b_completed_at": utcnow(),
        }
        deltas = {
            "done": 1,
            "truncated": 1 if truncated else 0,
            "prompt_tokens": r.prompt_tokens or 0,
            "completion_tokens": r.completion_tokens or 0,
            "reasoning_tokens": r.reasoning_tokens or 0,
        }
        self.unflushed += 1
        self.writer.finish(self, item.sample_id, values, deltas)
        self.done += 1
        self.consecutive_failures = 0
        if r.completion_tokens is not None:
            # Chunks are an estimate while streaming; the server's count is the truth.
            self._add(tokens=r.completion_tokens - r.chunks)
        self._add(samples=1)

    def _finish_failed(self, item: _Item, message: str, attempts: int) -> str | None:
        values = {
            "b_id": item.sample_id,
            "b_status": SAMPLE_FAILED,
            "b_reasoning": None,
            "b_response": None,
            "b_finish_reason": None,
            "b_prompt_tokens": None,
            "b_completion_tokens": None,
            "b_reasoning_tokens": None,
            "b_latency_ms": None,
            "b_ttft_ms": None,
            "b_attempts": attempts,
            "b_error": message,
            "b_completed_at": utcnow(),
        }
        self.unflushed += 1
        self.writer.finish(self, item.sample_id, values, {"failed": 1})
        self.failed += 1
        self.consecutive_failures += 1
        self.last_error = message
        self._add(samples=1)
        limit = self.params.pause_after_failures
        if limit and self.consecutive_failures >= limit and self.status == RUN_RUNNING:
            return f"Paused after {self.consecutive_failures} failures in a row. Last error: {message}"
        return None

    def _flushed(self, sample_id: int, finished: bool) -> None:
        self.claimed.discard(sample_id)
        if finished:
            self.unflushed = max(0, self.unflushed - 1)

    async def _count_pending(self) -> int:
        async with SessionLocal() as session:
            return (
                await session.execute(
                    select(func.count())
                    .select_from(Sample)
                    .where(Sample.run_id == self.run_id, Sample.status == SAMPLE_PENDING)
                )
            ).scalar_one()

    # ---- rates and metrics

    def _on_progress(self, _progress: Progress, n: int) -> None:
        sec = int(time.monotonic())
        b = self.buckets
        if b and b[-1][0] == sec:
            b[-1][1] += n
        else:
            b.append([sec, n, 0])
            while b and b[0][0] < sec - BUCKET_KEEP_S:
                b.popleft()

    def _add(self, tokens: float = 0, samples: int = 0) -> None:
        sec = int(time.monotonic())
        b = self.buckets
        if b and b[-1][0] == sec:
            b[-1][1] += tokens
            b[-1][2] += samples
        else:
            b.append([sec, tokens, samples])
            while b and b[0][0] < sec - BUCKET_KEEP_S:
                b.popleft()

    def _window(self, seconds: float) -> tuple[float, float, float]:
        """(tokens, samples, window length) over the last `seconds`, clipped to the task's age."""
        now = time.monotonic()
        cutoff = int(now) - seconds
        tokens = samples = 0.0
        for sec, t, s in reversed(self.buckets):
            if sec <= cutoff:
                break
            tokens += t
            samples += s
        span = max(1.0, min(seconds, now - self.started))
        return tokens, samples, span

    def tokens_per_s(self, seconds: float = 10) -> float:
        tokens, _, span = self._window(seconds)
        return max(0.0, tokens / span)

    def samples_per_min(self, seconds: float = 60) -> float:
        _, samples, span = self._window(seconds)
        return max(0.0, samples * 60.0 / span)

    def eta_seconds(self) -> float | None:
        if self.status != RUN_RUNNING:
            return None
        rate = self.samples_per_min() / 60.0
        remaining = max(0, self.total - self.done - self.failed)
        if remaining == 0:
            return 0.0
        if rate <= 0:
            return None
        return remaining / rate

    def unsaved_active_seconds(self) -> float:
        return max(0.0, time.monotonic() - self.active_mark)

    def _record_active(self) -> None:
        now = time.monotonic()
        self.writer.active(self.run_id, now - self.active_mark)
        self.active_mark = now

    def _write_metric(self, active: int | None = None) -> None:
        self.writer.metric(
            self.run_id,
            {
                "run_id": self.run_id,
                "t": utcnow(),
                "tokens_per_s": round(self.tokens_per_s(METRIC_INTERVAL_S), 3),
                "samples_per_min": round(self.samples_per_min(60), 3),
                "active": len(self.in_flight) if active is None else active,
                "concurrency": self.concurrency,
                "done": self.done,
                "failed": self.failed,
            },
        )

    async def _metrics_loop(self) -> None:
        while True:
            await asyncio.sleep(METRIC_INTERVAL_S)
            self._record_active()
            self._write_metric()

    def sequences(self, limit: int = LIVE_SEQUENCES) -> list[LiveSequence]:
        now = time.monotonic()
        out = []
        for seq in islice(list(self.in_flight.values()), limit):
            p = seq.progress
            out.append(
                LiveSequence(
                    sample_id=seq.item.sample_id,
                    prompt_idx=seq.item.prompt_idx,
                    sample_idx=seq.item.sample_idx,
                    prompt_preview=preview(last_user_text(seq.item.messages)),
                    phase=p.phase if p.phase in ("waiting", "thinking", "answering") else "waiting",
                    tokens=p.tokens,
                    tail=p.tail,
                    elapsed_ms=int((now - seq.started) * 1000),
                    attempt=p.attempt,
                )
            )
        return out


# --------------------------------------------------------------------------- DB helpers


async def _update_run(run_id: int, **values: Any) -> None:
    async with SessionLocal() as session:
        await session.execute(
            update(Run).where(Run.id == run_id).values(**values).execution_options(synchronize_session=False)
        )
        await session.commit()


def _recount_stmt(run_id: int):
    def agg(expr, *where):
        return (
            select(func.coalesce(expr, 0))
            .select_from(Sample)
            .where(Sample.run_id == run_id, *where)
            .scalar_subquery()
        )

    done = Sample.status == SAMPLE_DONE
    return (
        update(Run)
        .where(Run.id == run_id)
        .values(
            done=agg(func.count(), done),
            failed=agg(func.count(), Sample.status == SAMPLE_FAILED),
            truncated=agg(func.count(), done, Sample.finish_reason == "length"),
            prompt_tokens=agg(func.sum(Sample.prompt_tokens), done),
            completion_tokens=agg(func.sum(Sample.completion_tokens), done),
            reasoning_tokens=agg(func.sum(Sample.reasoning_tokens), done),
        )
        .execution_options(synchronize_session=False)
    )


async def _reset_failed(run_id: int) -> int:
    async with SessionLocal() as session:
        res = await session.execute(
            update(Sample)
            .where(Sample.run_id == run_id, Sample.status == SAMPLE_FAILED)
            .values(
                status=SAMPLE_PENDING,
                reasoning=None,
                response=None,
                finish_reason=None,
                prompt_tokens=None,
                completion_tokens=None,
                reasoning_tokens=None,
                latency_ms=None,
                ttft_ms=None,
                attempts=0,
                error=None,
                completed_at=None,
            )
            .execution_options(synchronize_session=False)
        )
        n = int(res.rowcount or 0)
        if n:
            await session.execute(
                update(Run)
                .where(Run.id == run_id)
                .values(failed=Run.failed - n)
                .execution_options(synchronize_session=False)
            )
        await session.commit()
        return n


# --------------------------------------------------------------------------- registry


class EngineRegistry:
    def __init__(self) -> None:
        self._tasks: dict[int, RunTask] = {}
        self._locks: dict[int, asyncio.Lock] = {}
        self._writer: Writer | None = None

    # ---- plumbing

    @property
    def writer(self) -> Writer:
        w = self._writer
        if w is None or w.task.done() or w.loop is not asyncio.get_running_loop():
            w = self._writer = Writer()
        return w

    def _lock(self, run_id: int) -> asyncio.Lock:
        lock = self._locks.get(run_id)
        if lock is None:
            lock = self._locks[run_id] = asyncio.Lock()
        return lock

    def _remove(self, task: RunTask) -> None:
        if self._tasks.get(task.run_id) is task:
            del self._tasks[task.run_id]

    async def _start_locked(self, run_id: int) -> None:
        if run_id in self._tasks:
            return
        async with SessionLocal() as session:
            run = await session.get(Run, run_id)
            if run is None:
                return
            endpoint = await session.get(Endpoint, run.endpoint_id) if run.endpoint_id else None
            if endpoint is None:
                await _update_run(run_id, status=RUN_PAUSED, status_reason=ENDPOINT_GONE)
                raise EngineConflict(ENDPOINT_GONE)
            # Nothing of this run is in flight, so nothing should say running.
            await session.execute(
                update(Sample)
                .where(Sample.run_id == run_id, Sample.status == SAMPLE_RUNNING)
                .values(status=SAMPLE_PENDING)
                .execution_options(synchronize_session=False)
            )
            await session.commit()
        task = RunTask(self, run, endpoint)
        self._tasks[run_id] = task
        task.start()

    # ---- queries

    def active_runs(self) -> list[RunTask]:
        return list(self._tasks.values())

    def get(self, run_id: int) -> RunTask | None:
        return self._tasks.get(run_id)

    def in_flight(self, run_id: int) -> int:
        """Samples that are neither pending nor counted in the run row yet."""
        task = self._tasks.get(run_id)
        return 0 if task is None else len(task.in_flight) + task.unflushed

    def unsaved_active_seconds(self, run_id: int) -> float:
        task = self._tasks.get(run_id)
        return 0.0 if task is None else task.unsaved_active_seconds()

    # ---- actions

    async def start(self, run_id: int) -> None:
        """Start a run whose row already says status "running"."""
        async with self._lock(run_id):
            await self._start_locked(run_id)

    async def pause(self, run_id: int) -> None:
        async with self._lock(run_id):
            task = self._tasks.get(run_id)
            if task is not None:
                async with task.lock:
                    if not task.finalizing and not task.cancelled:
                        await task._pause_locked(None)
                        return
                await task.wait()
            async with SessionLocal() as session:
                run = await session.get(Run, run_id)
                if run is None:
                    return
                if run.status == RUN_COMPLETED:
                    raise EngineConflict("This run has finished, so there is nothing to pause.")
                if run.status in (RUN_RUNNING, RUN_PAUSING):
                    run.status = RUN_PAUSED
                    await session.commit()

    async def resume(self, run_id: int) -> None:
        async with self._lock(run_id):
            task = self._tasks.get(run_id)
            if task is not None:
                async with task.lock:
                    if not task.finalizing and not task.cancelled:
                        if task.status == RUN_PAUSING:
                            await task._resume_locked()
                        return
                await task.wait()
            async with SessionLocal() as session:
                run = await session.get(Run, run_id)
                if run is None:
                    return
                if run.status == RUN_COMPLETED:
                    pending = (
                        await session.execute(
                            select(func.count())
                            .select_from(Sample)
                            .where(Sample.run_id == run_id, Sample.status.in_((SAMPLE_PENDING, SAMPLE_RUNNING)))
                        )
                    ).scalar_one()
                    if not pending:
                        raise EngineConflict(
                            "This run has finished, so there is nothing to resume. Retry failed samples or start a new run."
                        )
                if run.endpoint_id is None or await session.get(Endpoint, run.endpoint_id) is None:
                    raise EngineConflict(ENDPOINT_GONE)
                run.status = RUN_RUNNING
                run.status_reason = None
                run.finished_at = None
                await session.commit()
            await self._start_locked(run_id)

    async def retry_failed(self, run_id: int) -> int:
        """Failed samples go back to pending and the run runs. Returns how many were reset."""
        async with self._lock(run_id):
            task = self._tasks.get(run_id)
            if task is not None:
                async with task.lock:
                    if not task.finalizing and not task.cancelled:
                        n = await _reset_failed(run_id)
                        task.failed -= n
                        task.consecutive_failures = 0
                        if n:
                            task._rescan()
                            if task.status == RUN_PAUSING:
                                await task._resume_locked()
                            else:
                                task._spawn()
                        return n
                await task.wait()
            async with SessionLocal() as session:
                run = await session.get(Run, run_id)
                if run is None or not run.failed:
                    return 0
                if run.endpoint_id is None or await session.get(Endpoint, run.endpoint_id) is None:
                    raise EngineConflict(ENDPOINT_GONE)
            n = await _reset_failed(run_id)
            if n:
                await _update_run(run_id, status=RUN_RUNNING, status_reason=None, finished_at=None)
                await self._start_locked(run_id)
            return n

    def set_concurrency(self, run_id: int, n: int) -> None:
        task = self._tasks.get(run_id)
        if task is not None:
            task.set_concurrency(n)

    async def stop(self, run_id: int) -> None:
        """Stop a run's task at once (used before deleting it). Its status is left as is."""
        async with self._lock(run_id):
            task = self._tasks.get(run_id)
            if task is not None:
                await task.stop()

    def live(self, run_id: int) -> dict[str, Any] | None:
        task = self._tasks.get(run_id)
        if task is None:
            return None
        return {
            "in_flight": len(task.in_flight),
            "sequences": task.sequences(),
            "tokens_per_s": round(task.tokens_per_s(10), 3),
            "samples_per_min": round(task.samples_per_min(60), 3),
            "eta_seconds": task.eta_seconds(),
        }

    async def recover(self) -> None:
        """On startup: runs left running or pausing become paused; their in-flight samples pending."""
        active = set(self._tasks)
        async with SessionLocal() as session:
            ids = [
                rid
                for rid in (
                    await session.execute(select(Run.id).where(Run.status.in_((RUN_RUNNING, RUN_PAUSING))))
                ).scalars()
                if rid not in active
            ]
            if ids:
                await session.execute(
                    update(Run)
                    .where(Run.id.in_(ids))
                    .values(status=RUN_PAUSED, status_reason=RESTART_REASON)
                    .execution_options(synchronize_session=False)
                )
                await session.execute(
                    update(Sample)
                    .where(Sample.run_id.in_(ids), Sample.status == SAMPLE_RUNNING)
                    .values(status=SAMPLE_PENDING)
                    .execution_options(synchronize_session=False)
                )
                for rid in ids:
                    await session.execute(_recount_stmt(rid))
            await session.commit()
        if ids:
            log.info("Paused %d runs interrupted by a restart: %s", len(ids), ids)

    async def shutdown(self) -> None:
        """Stop every run: cancel in-flight requests (samples back to pending), flush, save time."""
        tasks = list(self._tasks.values())
        if tasks:
            await asyncio.gather(*(t.stop() for t in tasks), return_exceptions=True)
        if self._writer is not None and self._writer.loop is asyncio.get_running_loop():
            await self._writer.stop()
        self._writer = None
        self._tasks.clear()
        self._locks.clear()


engine_registry = EngineRegistry()
