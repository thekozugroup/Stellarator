//! Run engine: one task per running run, a dynamic worker pool, and a single batching DB writer.
//!
//! How a run moves:
//! - Workers take pending samples (in prompt order) from an in-memory buffer refilled from the DB,
//!   send them, and hand the result to the writer. There are always `concurrency` workers while
//!   the run is running, so `concurrency` requests are in flight. Raising it spawns workers at
//!   once; lowering it lets the extra workers finish their current request and exit.
//! - The writer commits batches (every 250 ms or 200 items) in one transaction: samples marked
//!   running when dispatched, finished samples, run counters, active seconds and metric points.
//!   Finished results live only in the writer's queue until they are committed.
//! - Pause: status "pausing", no new requests, in-flight ones finish, then "paused". Samples
//!   waiting to retry give up their slot and go back to pending.
//! - When nothing is pending or in flight the run is "completed".

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use parking_lot::Mutex;
use serde_json::Value;
use sqlx::SqlitePool;
use stellarator_core::openai::{api_root, build_headers};
use stellarator_core::probes::{ActiveRun, RunsProbe};
use stellarator_core::schemas::{LiveSequence, RunParams, RunStatus};
use stellarator_core::{time, ApiError, Ctx};
use tokio::sync::{mpsc, oneshot, watch, Notify};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::generation::{
    generate, last_user_text, prepare_messages, Backoff, GenerateError, GenerationResult, Progress, ProgressSink,
    ServerCaps,
};

pub const RESTART_REASON: &str = "Paused because Stellarator restarted. Resume to continue.";
pub const ENDPOINT_GONE: &str =
    "The endpoint this run used was deleted, so the run cannot continue. Start a new run on another endpoint.";
const METRIC_INTERVAL: Duration = Duration::from_secs(5);
const WRITE_INTERVAL: Duration = Duration::from_millis(250);
const WRITE_BATCH: usize = 200;
/// Writer queue bound: workers wait (backpressure) instead of piling results up in memory.
const WRITER_QUEUE: usize = 4096;
pub const LIVE_SEQUENCES: usize = 64;
pub const PREVIEW_CHARS: usize = 160;
const BUCKET_KEEP_S: i64 = 120;
const ID_CHUNK: usize = 500;

/// The text shortened for tables: whitespace collapsed, at most `limit` characters.
pub fn preview(text: &str, limit: usize) -> String {
    let text = crate::generation::one_line(text);
    if text.chars().count() <= limit {
        return text;
    }
    let cut: String = text.chars().take(limit.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn internal_reason(e: &dyn std::fmt::Display) -> String {
    format!("Paused because of an internal error: {e}. Resume to try again.")
}

// ------------------------------------------------------------------------------------ writer

/// A finished sample's row.
struct FinishRow {
    sample_id: i64,
    status: &'static str,
    reasoning: Option<String>,
    response: Option<String>,
    finish_reason: Option<String>,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
    latency_ms: Option<i64>,
    ttft_ms: Option<i64>,
    attempts: i64,
    error: Option<String>,
    completed_at: String,
}

#[derive(Default, Clone, Copy)]
struct Deltas {
    done: i64,
    failed: i64,
    truncated: i64,
    prompt_tokens: i64,
    completion_tokens: i64,
    reasoning_tokens: i64,
    active: f64,
}

impl Deltas {
    fn add(&mut self, o: &Deltas) {
        self.done += o.done;
        self.failed += o.failed;
        self.truncated += o.truncated;
        self.prompt_tokens += o.prompt_tokens;
        self.completion_tokens += o.completion_tokens;
        self.reasoning_tokens += o.reasoning_tokens;
        self.active += o.active;
    }
}

struct MetricRow {
    run_id: i64,
    t: String,
    tokens_per_s: f64,
    samples_per_min: f64,
    active: i64,
    concurrency: i64,
    done: i64,
    failed: i64,
}

enum Op {
    Mark { run_id: i64, sample_id: i64 },
    Finish { run_id: i64, task: Arc<RunTask>, row: Box<FinishRow>, deltas: Deltas },
    Reset { run_id: i64, sample_id: i64, task: Arc<RunTask> },
    Active { run_id: i64, seconds: f64 },
    Metric(MetricRow),
    Flush(oneshot::Sender<()>),
}

impl Op {
    fn run_id(&self) -> Option<i64> {
        match self {
            Op::Mark { run_id, .. }
            | Op::Finish { run_id, .. }
            | Op::Reset { run_id, .. }
            | Op::Active { run_id, .. } => Some(*run_id),
            Op::Metric(m) => Some(m.run_id),
            Op::Flush(_) => None,
        }
    }
}

/// The sending side of the writer. Sends to a writer that is gone are dropped.
#[derive(Clone)]
struct WriterHandle {
    tx: mpsc::Sender<Op>,
}

impl WriterHandle {
    async fn send(&self, op: Op) {
        let _ = self.tx.send(op).await;
    }

    /// Wait until everything queued so far is committed (at once when the writer is gone).
    async fn flush(&self) {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(Op::Flush(tx)).await.is_err() {
            return;
        }
        let _ = rx.await;
    }
}

struct WriterSlot {
    handle: WriterHandle,
    join: JoinHandle<()>,
}

fn spawn_writer(db: SqlitePool) -> WriterSlot {
    let (tx, rx) = mpsc::channel(WRITER_QUEUE);
    let join = tokio::spawn(writer_loop(db, rx));
    WriterSlot { handle: WriterHandle { tx }, join }
}

async fn writer_loop(db: SqlitePool, mut rx: mpsc::Receiver<Op>) {
    while let Some(first) = rx.recv().await {
        let mut batch = vec![first];
        if !matches!(batch[0], Op::Flush(_)) {
            let deadline = tokio::time::Instant::now() + WRITE_INTERVAL;
            while batch.len() < WRITE_BATCH && !matches!(batch.last(), Some(Op::Flush(_))) {
                match rx.try_recv() {
                    Ok(op) => {
                        batch.push(op);
                        continue;
                    }
                    Err(mpsc::error::TryRecvError::Disconnected) => break,
                    Err(mpsc::error::TryRecvError::Empty) => {}
                }
                match tokio::time::timeout_at(deadline, rx.recv()).await {
                    Ok(Some(op)) => batch.push(op),
                    _ => break,
                }
            }
        }
        commit(&db, &batch).await;
        after(batch);
    }
}

async fn commit(db: &SqlitePool, batch: &[Op]) {
    if batch.iter().all(|op| matches!(op, Op::Flush(_))) {
        return;
    }
    let all: Vec<&Op> = batch.iter().collect();
    if let Err(e) = write(db, &all).await {
        tracing::error!(error = %e, "writer batch failed; retrying run by run");
        let mut by_run: BTreeMap<i64, Vec<&Op>> = BTreeMap::new();
        for op in batch {
            if let Some(r) = op.run_id() {
                by_run.entry(r).or_default().push(op);
            }
        }
        for (run_id, ops) in by_run {
            if let Err(e) = write(db, &ops).await {
                tracing::error!(error = %e, run_id, dropped = ops.len(), "dropped engine writes");
            }
        }
    }
}

fn placeholders(n: usize) -> String {
    let mut s = String::with_capacity(n * 2);
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        s.push('?');
    }
    s
}

async fn write(db: &SqlitePool, batch: &[&Op]) -> sqlx::Result<()> {
    let mut marks: Vec<i64> = Vec::new();
    let mut finishes: Vec<&FinishRow> = Vec::new();
    let mut resets: Vec<i64> = Vec::new();
    let mut metrics: Vec<&MetricRow> = Vec::new();
    let mut deltas: BTreeMap<i64, Deltas> = BTreeMap::new();
    for op in batch {
        match op {
            Op::Mark { sample_id, .. } => marks.push(*sample_id),
            Op::Finish { run_id, row, deltas: d, .. } => {
                finishes.push(row.as_ref());
                deltas.entry(*run_id).or_default().add(d);
            }
            Op::Reset { sample_id, .. } => resets.push(*sample_id),
            Op::Active { run_id, seconds } => {
                deltas.entry(*run_id).or_default().active += seconds;
            }
            Op::Metric(m) => metrics.push(m),
            Op::Flush(_) => {}
        }
    }
    let mut tx = db.begin().await?;
    for chunk in marks.chunks(ID_CHUNK) {
        let sql = format!("UPDATE samples SET status = 'running' WHERE status = 'pending' AND id IN ({})", placeholders(chunk.len()));
        let mut q = sqlx::query(&sql);
        for id in chunk {
            q = q.bind(id);
        }
        q.execute(&mut *tx).await?;
    }
    for f in finishes {
        sqlx::query(
            "UPDATE samples SET status = ?, reasoning = ?, response = ?, finish_reason = ?, prompt_tokens = ?, \
             completion_tokens = ?, reasoning_tokens = ?, latency_ms = ?, ttft_ms = ?, attempts = ?, error = ?, \
             completed_at = ? WHERE id = ?",
        )
        .bind(f.status)
        .bind(&f.reasoning)
        .bind(&f.response)
        .bind(&f.finish_reason)
        .bind(f.prompt_tokens)
        .bind(f.completion_tokens)
        .bind(f.reasoning_tokens)
        .bind(f.latency_ms)
        .bind(f.ttft_ms)
        .bind(f.attempts)
        .bind(&f.error)
        .bind(&f.completed_at)
        .bind(f.sample_id)
        .execute(&mut *tx)
        .await?;
    }
    for chunk in resets.chunks(ID_CHUNK) {
        let sql = format!("UPDATE samples SET status = 'pending' WHERE status = 'running' AND id IN ({})", placeholders(chunk.len()));
        let mut q = sqlx::query(&sql);
        for id in chunk {
            q = q.bind(id);
        }
        q.execute(&mut *tx).await?;
    }
    for (run_id, d) in &deltas {
        sqlx::query(
            "UPDATE runs SET done = done + ?, failed = failed + ?, truncated = truncated + ?, \
             prompt_tokens = prompt_tokens + ?, completion_tokens = completion_tokens + ?, \
             reasoning_tokens = reasoning_tokens + ?, active_seconds = active_seconds + ? WHERE id = ?",
        )
        .bind(d.done)
        .bind(d.failed)
        .bind(d.truncated)
        .bind(d.prompt_tokens)
        .bind(d.completion_tokens)
        .bind(d.reasoning_tokens)
        .bind(d.active)
        .bind(run_id)
        .execute(&mut *tx)
        .await?;
    }
    for chunk in metrics.chunks(100) {
        let rows = vec!["(?, ?, ?, ?, ?, ?, ?, ?)"; chunk.len()].join(", ");
        let sql = format!(
            "INSERT INTO run_metrics (run_id, t, tokens_per_s, samples_per_min, active, concurrency, done, failed) VALUES {rows}"
        );
        let mut q = sqlx::query(&sql);
        for m in chunk {
            q = q
                .bind(m.run_id)
                .bind(&m.t)
                .bind(m.tokens_per_s)
                .bind(m.samples_per_min)
                .bind(m.active)
                .bind(m.concurrency)
                .bind(m.done)
                .bind(m.failed);
        }
        q.execute(&mut *tx).await?;
    }
    tx.commit().await
}

fn after(batch: Vec<Op>) {
    for op in batch {
        match op {
            Op::Finish { task, row, .. } => task.flushed(row.sample_id, true),
            Op::Reset { task, sample_id, .. } => task.flushed(sample_id, false),
            Op::Flush(tx) => {
                let _ = tx.send(());
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------------------------------ one run

/// A pending sample taken from the database. `messages` stays raw JSON until it is sent.
struct Item {
    sample_id: i64,
    prompt_idx: i64,
    sample_idx: i64,
    messages: String,
}

/// A request in flight.
struct Seq {
    sample_id: i64,
    prompt_idx: i64,
    sample_idx: i64,
    prompt_preview: String,
    started: Instant,
    progress: Mutex<Progress>,
}

/// Tokens and finished samples per second, for the last two minutes.
struct Rates {
    started: Instant,
    buckets: VecDeque<(i64, f64, f64)>, // (second, tokens, samples)
}

impl Rates {
    fn add(&mut self, tokens: f64, samples: f64) {
        let sec = self.started.elapsed().as_secs() as i64;
        match self.buckets.back_mut() {
            Some(b) if b.0 == sec => {
                b.1 += tokens;
                b.2 += samples;
            }
            _ => {
                self.buckets.push_back((sec, tokens, samples));
                while self.buckets.front().is_some_and(|b| b.0 < sec - BUCKET_KEEP_S) {
                    self.buckets.pop_front();
                }
            }
        }
    }

    /// (tokens, samples, window length) over the last `seconds`, clipped to the task's age.
    fn window(&self, seconds: f64) -> (f64, f64, f64) {
        let now = self.started.elapsed().as_secs_f64();
        let cutoff = now.floor() - seconds;
        let (mut tokens, mut samples) = (0.0, 0.0);
        for (sec, t, s) in self.buckets.iter().rev() {
            if (*sec as f64) <= cutoff {
                break;
            }
            tokens += t;
            samples += s;
        }
        (tokens, samples, seconds.min(now).max(1.0))
    }

    fn tokens_per_s(&self, seconds: f64) -> f64 {
        let (t, _, span) = self.window(seconds);
        (t / span).max(0.0)
    }

    fn samples_per_min(&self, seconds: f64) -> f64 {
        let (_, s, span) = self.window(seconds);
        (s * 60.0 / span).max(0.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TaskStatus {
    Running,
    Pausing,
}

struct State {
    concurrency: usize,
    total: i64,
    done: i64,
    failed: i64,
    status: TaskStatus,
    reason: Option<String>,
    workers: usize,
    /// In dispatch order (oldest first).
    in_flight: BTreeMap<u64, Arc<Seq>>,
    next_key: u64,
    buffer: VecDeque<Item>,
    /// Buffered, in flight, or finished but not yet committed.
    claimed: HashSet<i64>,
    cursor: i64,
    exhausted: bool,
    finalizing: bool,
    cancelled: bool,
    /// Finished samples not yet committed.
    unflushed: i64,
    /// Samples sent by this task.
    dispatched: i64,
    consecutive_failures: i64,
    active_mark: Instant,
}

impl State {
    fn release_buffer(&mut self) {
        for item in self.buffer.drain(..) {
            self.claimed.remove(&item.sample_id);
        }
    }
    fn rescan(&mut self) {
        self.cursor = 0;
        self.exhausted = false;
    }
    fn begin_pause(&mut self, reason: Option<String>) {
        self.status = TaskStatus::Pausing;
        self.reason = reason;
        self.release_buffer();
    }
    fn can_run(&self) -> bool {
        self.status == TaskStatus::Running && !self.finalizing && !self.cancelled
    }
}

/// The task driving one running (or pausing) run.
pub(crate) struct RunTask {
    engine: Weak<Engine>,
    db: SqlitePool,
    writer: WriterHandle,
    run_id: i64,
    endpoint_id: Option<i64>,
    prompt_set_id: i64,
    model: String,
    params: RunParams,
    url: String,
    client: reqwest::Client,
    caps: ServerCaps,
    st: Mutex<State>,
    rates: Mutex<Rates>,
    /// State transitions (pause, resume, retry, finalize).
    lock: tokio::sync::Mutex<()>,
    refill_lock: tokio::sync::Mutex<()>,
    /// Notified when the last worker exits.
    idle: Notify,
    /// True while pausing: samples waiting to retry give up their slot.
    halt: watch::Sender<bool>,
    /// Stops workers at once (their samples go back to pending).
    stop_workers: CancellationToken,
    /// Set when the task has wound down.
    ended: CancellationToken,
    reason_final: Mutex<Option<String>>,
}

struct RunInfo {
    run_id: i64,
    endpoint_id: Option<i64>,
    prompt_set_id: i64,
    model: String,
    params: RunParams,
    concurrency: i64,
    total: i64,
    done: i64,
    failed: i64,
}

struct EndpointInfo {
    base_url: String,
    api_key: Option<String>,
    headers: BTreeMap<String, String>,
}

impl RunTask {
    fn new(engine: &Arc<Engine>, writer: WriterHandle, run: RunInfo, endpoint: EndpointInfo) -> anyhow::Result<Arc<Self>> {
        let timeout = run.params.request_timeout_s.max(0.001);
        // HTTP/1.1 with a connection per request in flight, like the Python backend: over HTTP/2 a
        // server's stream limit (often 100 to 256) would silently cap the run's concurrency.
        let client = reqwest::Client::builder()
            .http1_only()
            .user_agent(concat!("stellarator/", env!("CARGO_PKG_VERSION")))
            .default_headers(build_headers(endpoint.api_key.as_deref(), &endpoint.headers))
            .connect_timeout(Duration::from_secs_f64(timeout.min(30.0)))
            .read_timeout(Duration::from_secs_f64(timeout))
            .build()?;
        let now = Instant::now();
        let (halt, _) = watch::channel(false);
        Ok(Arc::new(Self {
            engine: Arc::downgrade(engine),
            db: engine.ctx.db.clone(),
            writer,
            run_id: run.run_id,
            endpoint_id: run.endpoint_id,
            prompt_set_id: run.prompt_set_id,
            model: run.model,
            url: format!("{}/chat/completions", api_root(&endpoint.base_url)),
            params: run.params,
            client,
            caps: ServerCaps::default(),
            st: Mutex::new(State {
                concurrency: run.concurrency.max(1) as usize,
                total: run.total,
                done: run.done,
                failed: run.failed,
                status: TaskStatus::Running,
                reason: None,
                workers: 0,
                in_flight: BTreeMap::new(),
                next_key: 0,
                buffer: VecDeque::new(),
                claimed: HashSet::new(),
                cursor: 0,
                exhausted: false,
                finalizing: false,
                cancelled: false,
                unflushed: 0,
                dispatched: 0,
                consecutive_failures: 0,
                active_mark: now,
            }),
            rates: Mutex::new(Rates { started: now, buckets: VecDeque::new() }),
            lock: tokio::sync::Mutex::new(()),
            refill_lock: tokio::sync::Mutex::new(()),
            idle: Notify::new(),
            halt,
            stop_workers: CancellationToken::new(),
            ended: CancellationToken::new(),
            reason_final: Mutex::new(None),
        }))
    }

    // ---- lifecycle

    fn start(self: &Arc<Self>) {
        let me = self.clone();
        tokio::spawn(async move { me.main().await });
    }

    async fn wait(&self) {
        self.ended.cancelled().await;
    }

    /// Stop now: cancel in-flight requests (their samples go back to pending). The run's status is
    /// left as it is.
    async fn stop(self: &Arc<Self>) {
        {
            let mut st = self.st.lock();
            st.cancelled = true;
            st.release_buffer();
        }
        self.stop_workers.cancel();
        self.wait().await;
    }

    fn is_finalizing_or_cancelled(&self) -> bool {
        let st = self.st.lock();
        st.finalizing || st.cancelled
    }

    async fn wait_idle(&self) {
        loop {
            let notified = self.idle.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.st.lock().workers == 0 {
                return;
            }
            notified.await;
        }
    }

    async fn main(self: Arc<Self>) {
        self.write_metric(None).await;
        let metrics = {
            let me = self.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + METRIC_INTERVAL, METRIC_INTERVAL);
                loop {
                    tick.tick().await;
                    me.record_active().await;
                    me.write_metric(None).await;
                }
            })
        };
        let mut rescanned_at: i64 = -1; // dispatched count when we last rescanned for leftovers
        let outcome: anyhow::Result<Option<RunStatus>> = async {
            loop {
                self.spawn_workers();
                self.wait_idle().await;
                if self.st.lock().cancelled {
                    return Ok(None);
                }
                self.writer.flush().await;
                let _g = self.lock.lock().await;
                let dispatched = {
                    let mut st = self.st.lock();
                    if st.cancelled {
                        return Ok(None);
                    }
                    if st.workers > 0 {
                        continue; // resumed or respawned while we flushed
                    }
                    if st.status == TaskStatus::Pausing {
                        st.finalizing = true;
                        return Ok(Some(RunStatus::Paused));
                    }
                    st.dispatched
                };
                let pending: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM samples WHERE run_id = ? AND status = 'pending'")
                        .bind(self.run_id)
                        .fetch_one(&self.db)
                        .await?;
                let mut st = self.st.lock();
                if pending > 0 && dispatched != rescanned_at {
                    // Pending samples behind the cursor (e.g. returned while pausing): scan again.
                    // Stop when a full pass dispatched nothing, so this can never spin.
                    rescanned_at = dispatched;
                    st.rescan();
                    continue;
                }
                st.finalizing = true;
                return Ok(Some(RunStatus::Completed));
            }
        }
        .await;
        let final_status = match outcome {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(run_id = self.run_id, error = %e, "run stopped on an internal error");
                *self.reason_final.lock() = Some(internal_reason(&e));
                Some(RunStatus::Paused)
            }
        };
        self.st.lock().finalizing = true;
        metrics.abort();
        self.wind_down(final_status).await;
    }

    async fn wind_down(self: &Arc<Self>, final_status: Option<RunStatus>) {
        if self.st.lock().workers > 0 {
            self.stop_workers.cancel();
            self.wait_idle().await;
        }
        let cancelled = {
            let mut st = self.st.lock();
            st.release_buffer();
            st.cancelled
        };
        self.record_active().await;
        self.write_metric(Some(0)).await;
        self.writer.flush().await;
        if let (Some(status), false) = (final_status, cancelled) {
            let res = match status {
                RunStatus::Completed => sqlx::query(
                    "UPDATE runs SET status = 'completed', finished_at = ?, status_reason = NULL WHERE id = ?",
                )
                .bind(time::now_db())
                .bind(self.run_id)
                .execute(&self.db)
                .await,
                _ => {
                    let reason = self.reason_final.lock().clone().or_else(|| self.st.lock().reason.clone());
                    sqlx::query("UPDATE runs SET status = ?, status_reason = ? WHERE id = ?")
                        .bind(status.as_str())
                        .bind(reason)
                        .bind(self.run_id)
                        .execute(&self.db)
                        .await
                }
            };
            if let Err(e) = res {
                tracing::error!(run_id = self.run_id, error = %e, "could not save the final state of a run");
            }
        }
        if let Some(engine) = self.engine.upgrade() {
            engine.remove(self);
        }
        self.ended.cancel();
    }

    // ---- state changes (call with self.lock held)

    async fn pause_locked(&self, reason: Option<String>) -> Result<(), ApiError> {
        {
            let mut st = self.st.lock();
            if !st.can_run() {
                return Ok(());
            }
            st.begin_pause(reason);
        }
        self.halt.send_replace(true);
        self.save_pausing().await
    }

    /// Write "pausing" to the run row if the task is still pausing (call with self.lock held).
    async fn save_pausing(&self) -> Result<(), ApiError> {
        let reason = {
            let st = self.st.lock();
            if st.status != TaskStatus::Pausing || st.finalizing || st.cancelled {
                return Ok(());
            }
            st.reason.clone()
        };
        sqlx::query("UPDATE runs SET status = 'pausing', status_reason = ? WHERE id = ?")
            .bind(reason)
            .bind(self.run_id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    async fn resume_locked(self: &Arc<Self>) -> Result<(), ApiError> {
        {
            let mut st = self.st.lock();
            if st.finalizing || st.cancelled {
                return Ok(());
            }
            st.status = TaskStatus::Running;
            st.reason = None;
            st.consecutive_failures = 0;
            // Samples interrupted while pausing went back to pending behind the cursor.
            st.rescan();
        }
        self.halt.send_replace(false);
        sqlx::query("UPDATE runs SET status = 'running', status_reason = NULL, finished_at = NULL WHERE id = ?")
            .bind(self.run_id)
            .execute(&self.db)
            .await?;
        self.spawn_workers();
        Ok(())
    }

    async fn set_concurrency(self: &Arc<Self>, n: i64) {
        let live = {
            let mut st = self.st.lock();
            st.concurrency = n.max(1) as usize;
            st.can_run()
        };
        if live {
            self.spawn_workers();
            self.write_metric(None).await;
        }
    }

    // ---- workers

    fn spawn_workers(self: &Arc<Self>) {
        let mut st = self.st.lock();
        if !st.can_run() {
            return;
        }
        while st.workers < st.concurrency {
            st.workers += 1;
            let me = self.clone();
            tokio::spawn(async move { me.worker().await });
        }
    }

    async fn worker(self: Arc<Self>) {
        let mut guard = WorkerGuard { task: &self, counted: true };
        loop {
            {
                let mut st = self.st.lock();
                if st.cancelled || st.status != TaskStatus::Running || st.workers > st.concurrency {
                    guard.exit(&mut st);
                    return;
                }
            }
            let item = match self.next().await {
                Ok(Some(item)) => item,
                Ok(None) => return,
                Err(e) => {
                    tracing::error!(run_id = self.run_id, error = %e, "worker failed");
                    let _g = self.lock.lock().await;
                    if let Err(e2) = self.pause_locked(Some(internal_reason(&e))).await {
                        tracing::error!(run_id = self.run_id, error = %e2.detail, "could not pause a run");
                    }
                    return;
                }
            };
            {
                let mut st = self.st.lock();
                if st.cancelled || st.status != TaskStatus::Running {
                    st.claimed.remove(&item.sample_id);
                    return;
                }
            }
            if !self.process(item).await {
                return;
            }
        }
    }

    async fn next(self: &Arc<Self>) -> anyhow::Result<Option<Item>> {
        loop {
            {
                let mut st = self.st.lock();
                if let Some(item) = st.buffer.pop_front() {
                    return Ok(Some(item));
                }
                if st.exhausted || st.cancelled || st.status != TaskStatus::Running {
                    return Ok(None);
                }
            }
            let _g = self.refill_lock.lock().await;
            {
                let st = self.st.lock();
                if !st.buffer.is_empty() || st.exhausted || st.cancelled || st.status != TaskStatus::Running {
                    continue;
                }
            }
            self.refill().await?;
        }
    }

    async fn refill(&self) -> anyhow::Result<()> {
        let (cursor, limit) = {
            let st = self.st.lock();
            (st.cursor, (st.concurrency * 2).clamp(256, 4096) as i64)
        };
        // Sample ids follow (prompt_idx, sample_idx): see the run creation in routes.rs.
        let rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(
            "SELECT s.id, s.prompt_idx, s.sample_idx, p.messages FROM samples s JOIN prompts p ON p.id = s.prompt_id \
             WHERE s.run_id = ? AND s.status = 'pending' AND s.id > ? ORDER BY s.id LIMIT ?",
        )
        .bind(self.run_id)
        .bind(cursor)
        .bind(limit)
        .fetch_all(&self.db)
        .await?;
        let mut st = self.st.lock();
        let Some(last) = rows.last() else {
            st.exhausted = true;
            return Ok(());
        };
        st.cursor = last.0;
        for (sample_id, prompt_idx, sample_idx, messages) in rows {
            if st.claimed.insert(sample_id) {
                st.buffer.push_back(Item { sample_id, prompt_idx, sample_idx, messages });
            }
        }
        Ok(())
    }

    /// Send one sample and hand its result to the writer. False when the task is stopping.
    async fn process(self: &Arc<Self>, item: Item) -> bool {
        let messages: Vec<Value> = match serde_json::from_str::<Value>(&item.messages) {
            Ok(Value::Array(list)) => list,
            _ => Vec::new(),
        };
        let seq = Arc::new(Seq {
            sample_id: item.sample_id,
            prompt_idx: item.prompt_idx,
            sample_idx: item.sample_idx,
            prompt_preview: preview(&last_user_text(&messages), PREVIEW_CHARS),
            started: Instant::now(),
            progress: Mutex::new(Progress::default()),
        });
        let key = {
            let mut st = self.st.lock();
            st.dispatched += 1;
            let key = st.next_key;
            st.next_key += 1;
            st.in_flight.insert(key, seq.clone());
            key
        };
        self.writer.send(Op::Mark { run_id: self.run_id, sample_id: item.sample_id }).await;
        let prepared = prepare_messages(&messages, &self.params);
        drop(messages);
        let sink = SeqSink { seq: &seq, rates: &self.rates };
        let backoff = HaltBackoff { task: self };
        let outcome = tokio::select! {
            biased;
            _ = self.stop_workers.cancelled() => None,
            r = generate(&self.client, &self.url, &self.model, &prepared, &self.params, item.sample_idx,
                         &self.caps, &sink, &backoff) => Some(r),
        };
        match outcome {
            None => {
                self.reset(key, item.sample_id).await;
                false
            }
            Some(Err(GenerateError::Interrupted)) => {
                self.reset(key, item.sample_id).await;
                true
            }
            Some(Ok(result)) => {
                self.finish_done(key, item.sample_id, result).await;
                true
            }
            Some(Err(GenerateError::Failed { message, attempts })) => {
                if self.finish_failed(key, item.sample_id, message, attempts).await {
                    let _g = self.lock.lock().await;
                    if let Err(e) = self.save_pausing().await {
                        tracing::error!(run_id = self.run_id, error = %e.detail, "could not pause a run");
                    }
                }
                true
            }
        }
    }

    async fn reset(self: &Arc<Self>, key: u64, sample_id: i64) {
        self.st.lock().in_flight.remove(&key);
        self.writer.send(Op::Reset { run_id: self.run_id, sample_id, task: self.clone() }).await;
    }

    async fn finish_done(self: &Arc<Self>, key: u64, sample_id: i64, r: GenerationResult) {
        let truncated = r.finish_reason.as_deref() == Some("length");
        let deltas = Deltas {
            done: 1,
            truncated: i64::from(truncated),
            prompt_tokens: r.prompt_tokens.unwrap_or(0),
            completion_tokens: r.completion_tokens.unwrap_or(0),
            reasoning_tokens: r.reasoning_tokens.unwrap_or(0),
            ..Deltas::default()
        };
        {
            let mut rates = self.rates.lock();
            if let Some(ct) = r.completion_tokens {
                // Chunks are an estimate while streaming; the server's count is the truth.
                rates.add((ct - r.chunks) as f64, 0.0);
            }
            rates.add(0.0, 1.0);
        }
        let row = FinishRow {
            sample_id,
            status: "done",
            finish_reason: r.finish_reason.filter(|f| !f.is_empty()).map(|f| f.chars().take(40).collect()),
            reasoning: r.reasoning,
            response: Some(r.response),
            prompt_tokens: r.prompt_tokens,
            completion_tokens: r.completion_tokens,
            reasoning_tokens: r.reasoning_tokens,
            latency_ms: Some(r.latency_ms),
            ttft_ms: r.ttft_ms,
            attempts: r.attempts,
            error: None,
            completed_at: time::now_db(),
        };
        {
            let mut st = self.st.lock();
            st.unflushed += 1;
            st.done += 1;
            st.consecutive_failures = 0;
            st.in_flight.remove(&key);
        }
        self.writer.send(Op::Finish { run_id: self.run_id, task: self.clone(), row: Box::new(row), deltas }).await;
    }

    /// True when the circuit breaker tripped: the task is pausing and the run row must say so.
    async fn finish_failed(self: &Arc<Self>, key: u64, sample_id: i64, message: String, attempts: i64) -> bool {
        self.rates.lock().add(0.0, 1.0);
        let trip = {
            let mut st = self.st.lock();
            st.unflushed += 1;
            st.failed += 1;
            st.consecutive_failures += 1;
            st.in_flight.remove(&key);
            let limit = self.params.pause_after_failures;
            let trip = limit > 0 && st.consecutive_failures >= limit && st.can_run();
            if trip {
                // In the same critical section, so no other worker sends another request first.
                let reason = format!("Paused after {} failures in a row. Last error: {message}", st.consecutive_failures);
                st.begin_pause(Some(reason));
            }
            trip
        };
        if trip {
            self.halt.send_replace(true);
        }
        let row = FinishRow {
            sample_id,
            status: "failed",
            reasoning: None,
            response: None,
            finish_reason: None,
            prompt_tokens: None,
            completion_tokens: None,
            reasoning_tokens: None,
            latency_ms: None,
            ttft_ms: None,
            attempts,
            error: Some(message),
            completed_at: time::now_db(),
        };
        let deltas = Deltas { failed: 1, ..Deltas::default() };
        self.writer.send(Op::Finish { run_id: self.run_id, task: self.clone(), row: Box::new(row), deltas }).await;
        trip
    }

    /// Called by the writer once a finish or reset is committed.
    fn flushed(&self, sample_id: i64, finished: bool) {
        let mut st = self.st.lock();
        st.claimed.remove(&sample_id);
        if finished {
            st.unflushed = (st.unflushed - 1).max(0);
        }
    }

    // ---- rates and metrics

    fn eta_seconds(&self) -> Option<f64> {
        let (running, remaining) = {
            let st = self.st.lock();
            (st.status == TaskStatus::Running, (st.total - st.done - st.failed).max(0))
        };
        if !running {
            return None;
        }
        if remaining == 0 {
            return Some(0.0);
        }
        let rate = self.rates.lock().samples_per_min(60.0) / 60.0;
        if rate <= 0.0 {
            return None;
        }
        Some(remaining as f64 / rate)
    }

    fn unsaved_active_seconds(&self) -> f64 {
        self.st.lock().active_mark.elapsed().as_secs_f64()
    }

    async fn record_active(&self) {
        let seconds = {
            let mut st = self.st.lock();
            let now = Instant::now();
            let s = now.duration_since(st.active_mark).as_secs_f64();
            st.active_mark = now;
            s
        };
        if seconds > 0.0 {
            self.writer.send(Op::Active { run_id: self.run_id, seconds }).await;
        }
    }

    async fn write_metric(&self, active: Option<i64>) {
        let (tps, spm) = {
            let r = self.rates.lock();
            (round3(r.tokens_per_s(METRIC_INTERVAL.as_secs_f64())), round3(r.samples_per_min(60.0)))
        };
        let row = {
            let st = self.st.lock();
            MetricRow {
                run_id: self.run_id,
                t: time::now_db(),
                tokens_per_s: tps,
                samples_per_min: spm,
                active: active.unwrap_or(st.in_flight.len() as i64),
                concurrency: st.concurrency as i64,
                done: st.done,
                failed: st.failed,
            }
        };
        self.writer.send(Op::Metric(row)).await;
    }

    /// Requests in flight, and the oldest `limit` of them for the live view.
    fn sequences(&self, limit: usize) -> (i64, Vec<LiveSequence>) {
        let (count, seqs): (i64, Vec<Arc<Seq>>) = {
            let st = self.st.lock();
            (st.in_flight.len() as i64, st.in_flight.values().take(limit).cloned().collect())
        };
        let list = seqs
            .iter()
            .map(|s| {
                let p = s.progress.lock();
                LiveSequence {
                    sample_id: s.sample_id,
                    prompt_idx: s.prompt_idx,
                    sample_idx: s.sample_idx,
                    prompt_preview: s.prompt_preview.clone(),
                    phase: p.phase,
                    tokens: p.tokens,
                    tail: p.tail.clone(),
                    elapsed_ms: s.started.elapsed().as_millis() as i64,
                    attempt: p.attempt,
                }
            })
            .collect();
        (count, list)
    }
}

/// Keeps the worker count right however a worker ends.
struct WorkerGuard<'a> {
    task: &'a RunTask,
    counted: bool,
}

impl WorkerGuard<'_> {
    fn exit(&mut self, st: &mut State) {
        if self.counted {
            self.counted = false;
            st.workers -= 1;
            if st.workers == 0 {
                self.task.idle.notify_waiters();
            }
        }
    }
}

impl Drop for WorkerGuard<'_> {
    fn drop(&mut self) {
        if self.counted {
            let mut st = self.task.st.lock();
            self.exit(&mut st);
        }
    }
}

/// Copies one request's progress where the live view reads it, and counts tokens for the rate.
struct SeqSink<'a> {
    seq: &'a Seq,
    rates: &'a Mutex<Rates>,
}

impl ProgressSink for SeqSink<'_> {
    fn on_chunk(&self, p: &Progress) {
        copy_progress(&mut self.seq.progress.lock(), p);
        self.rates.lock().add(1.0, 0.0);
    }
    fn on_reset(&self, p: &Progress) {
        copy_progress(&mut self.seq.progress.lock(), p);
    }
}

fn copy_progress(dst: &mut Progress, src: &Progress) {
    dst.phase = src.phase;
    dst.tokens = src.tokens;
    dst.attempt = src.attempt;
    dst.tail.clone_from(&src.tail);
}

/// Waits before a retry, unless the run starts pausing: then the slot is given up.
struct HaltBackoff<'a> {
    task: &'a RunTask,
}

impl Backoff for HaltBackoff<'_> {
    fn sleep(&self, seconds: f64) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            if self.task.st.lock().status != TaskStatus::Running {
                return false;
            }
            let mut halt = self.task.halt.subscribe();
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs_f64(seconds.max(0.0))) => true,
                _ = halt.wait_for(|h| *h) => false,
            }
        })
    }
}

// ------------------------------------------------------------------------------------ engine

/// What the live view of a running run shows.
pub struct LiveInfo {
    pub in_flight: i64,
    pub sequences: Vec<LiveSequence>,
    pub tokens_per_s: f64,
    pub samples_per_min: f64,
    pub eta_seconds: Option<f64>,
}

/// Every running run's task, the shared writer, and the actions the API takes on runs.
pub struct Engine {
    pub(crate) ctx: Ctx,
    me: Weak<Engine>,
    tasks: Mutex<HashMap<i64, Arc<RunTask>>>,
    locks: Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
    writer: Mutex<Option<WriterSlot>>,
}

impl Engine {
    pub fn new(ctx: Ctx) -> Arc<Engine> {
        Arc::new_cyclic(|me| Engine {
            ctx,
            me: me.clone(),
            tasks: Mutex::new(HashMap::new()),
            locks: Mutex::new(HashMap::new()),
            writer: Mutex::new(None),
        })
    }

    pub fn ctx(&self) -> &Ctx {
        &self.ctx
    }

    fn writer(&self) -> WriterHandle {
        let mut slot = self.writer.lock();
        if let Some(w) = slot.as_ref() {
            if !w.join.is_finished() && !w.handle.tx.is_closed() {
                return w.handle.clone();
            }
        }
        let w = spawn_writer(self.ctx.db.clone());
        let handle = w.handle.clone();
        *slot = Some(w);
        handle
    }

    fn run_lock(&self, run_id: i64) -> Arc<tokio::sync::Mutex<()>> {
        self.locks.lock().entry(run_id).or_default().clone()
    }

    fn task(&self, run_id: i64) -> Option<Arc<RunTask>> {
        self.tasks.lock().get(&run_id).cloned()
    }

    fn remove(&self, task: &Arc<RunTask>) {
        let mut tasks = self.tasks.lock();
        if tasks.get(&task.run_id).is_some_and(|t| Arc::ptr_eq(t, task)) {
            tasks.remove(&task.run_id);
        }
    }

    async fn start_locked(&self, run_id: i64) -> Result<(), ApiError> {
        if self.tasks.lock().contains_key(&run_id) {
            return Ok(());
        }
        let Some(engine) = self.me.upgrade() else { return Ok(()) };
        let db = &self.ctx.db;
        type RunRow = (i64, Option<i64>, i64, String, sqlx::types::Json<Value>, i64, i64, i64, i64);
        let row: Option<RunRow> = sqlx::query_as(
            "SELECT id, endpoint_id, prompt_set_id, model, params, concurrency, total, done, failed FROM runs WHERE id = ?",
        )
        .bind(run_id)
        .fetch_optional(db)
        .await?;
        let Some((id, endpoint_id, prompt_set_id, model, params, concurrency, total, done, failed)) = row else {
            return Ok(());
        };
        let endpoint: Option<(String, Option<String>, sqlx::types::Json<Value>)> = match endpoint_id {
            Some(eid) => sqlx::query_as("SELECT base_url, api_key, headers FROM endpoints WHERE id = ?")
                .bind(eid)
                .fetch_optional(db)
                .await?,
            None => None,
        };
        let Some((base_url, api_key, headers)) = endpoint else {
            sqlx::query("UPDATE runs SET status = 'paused', status_reason = ? WHERE id = ?")
                .bind(ENDPOINT_GONE)
                .bind(run_id)
                .execute(db)
                .await?;
            return Err(ApiError::conflict(ENDPOINT_GONE));
        };
        // Nothing of this run is in flight, so nothing should say running.
        sqlx::query("UPDATE samples SET status = 'pending' WHERE run_id = ? AND status = 'running'")
            .bind(run_id)
            .execute(db)
            .await?;
        let headers: BTreeMap<String, String> = match headers.0 {
            Value::Object(m) => m
                .into_iter()
                .map(|(k, v)| (k, v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))
                .collect(),
            _ => BTreeMap::new(),
        };
        let info = RunInfo {
            run_id: id,
            endpoint_id,
            prompt_set_id,
            model,
            params: serde_json::from_value(params.0).unwrap_or_default(),
            concurrency,
            total,
            done,
            failed,
        };
        let task = RunTask::new(&engine, self.writer(), info, EndpointInfo { base_url, api_key, headers })?;
        self.tasks.lock().insert(run_id, task.clone());
        task.start();
        Ok(())
    }

    // ---- queries

    /// Samples that are neither pending nor counted in the run row yet.
    pub fn in_flight(&self, run_id: i64) -> i64 {
        match self.task(run_id) {
            Some(t) => {
                let st = t.st.lock();
                st.in_flight.len() as i64 + st.unflushed
            }
            None => 0,
        }
    }

    /// Run time not yet added to the run row.
    pub fn unsaved_active_seconds(&self, run_id: i64) -> f64 {
        self.task(run_id).map_or(0.0, |t| t.unsaved_active_seconds())
    }

    /// The live view of a run the engine drives, else None.
    pub fn live(&self, run_id: i64) -> Option<LiveInfo> {
        let t = self.task(run_id)?;
        let (in_flight, sequences) = t.sequences(LIVE_SEQUENCES);
        let (tps, spm) = {
            let r = t.rates.lock();
            (round3(r.tokens_per_s(10.0)), round3(r.samples_per_min(60.0)))
        };
        Some(LiveInfo {
            in_flight,
            sequences,
            tokens_per_s: tps,
            samples_per_min: spm,
            eta_seconds: t.eta_seconds(),
        })
    }

    /// The concurrency the engine is using for a run, if it drives it.
    pub fn concurrency(&self, run_id: i64) -> Option<i64> {
        self.task(run_id).map(|t| t.st.lock().concurrency as i64)
    }

    // ---- actions

    /// Start a run whose row already says status "running".
    pub async fn start(&self, run_id: i64) -> Result<(), ApiError> {
        let lock = self.run_lock(run_id);
        let _g = lock.lock().await;
        self.start_locked(run_id).await
    }

    pub async fn pause(&self, run_id: i64) -> Result<(), ApiError> {
        let lock = self.run_lock(run_id);
        let _g = lock.lock().await;
        if let Some(task) = self.task(run_id) {
            {
                let _tl = task.lock.lock().await;
                if !task.is_finalizing_or_cancelled() {
                    return task.pause_locked(None).await;
                }
            }
            task.wait().await;
        }
        let db = &self.ctx.db;
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM runs WHERE id = ?").bind(run_id).fetch_optional(db).await?;
        match status.as_deref() {
            None => Ok(()),
            Some("completed") => Err(ApiError::conflict("This run has finished, so there is nothing to pause.")),
            Some("running") | Some("pausing") => {
                sqlx::query("UPDATE runs SET status = 'paused' WHERE id = ?").bind(run_id).execute(db).await?;
                Ok(())
            }
            Some(_) => Ok(()),
        }
    }

    pub async fn resume(&self, run_id: i64) -> Result<(), ApiError> {
        let lock = self.run_lock(run_id);
        let _g = lock.lock().await;
        if let Some(task) = self.task(run_id) {
            {
                let _tl = task.lock.lock().await;
                if !task.is_finalizing_or_cancelled() {
                    if task.st.lock().status == TaskStatus::Pausing {
                        task.resume_locked().await?;
                    }
                    return Ok(());
                }
            }
            task.wait().await;
        }
        let db = &self.ctx.db;
        let row: Option<(String, Option<i64>)> =
            sqlx::query_as("SELECT status, endpoint_id FROM runs WHERE id = ?").bind(run_id).fetch_optional(db).await?;
        let Some((status, endpoint_id)) = row else { return Ok(()) };
        if status == "completed" {
            let pending: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM samples WHERE run_id = ? AND status IN ('pending', 'running')",
            )
            .bind(run_id)
            .fetch_one(db)
            .await?;
            if pending == 0 {
                return Err(ApiError::conflict(
                    "This run has finished, so there is nothing to resume. Retry failed samples or start a new run.",
                ));
            }
        }
        self.require_endpoint(endpoint_id).await?;
        sqlx::query("UPDATE runs SET status = 'running', status_reason = NULL, finished_at = NULL WHERE id = ?")
            .bind(run_id)
            .execute(db)
            .await?;
        self.start_locked(run_id).await
    }

    async fn require_endpoint(&self, endpoint_id: Option<i64>) -> Result<(), ApiError> {
        let exists = match endpoint_id {
            Some(eid) => sqlx::query_scalar::<_, i64>("SELECT id FROM endpoints WHERE id = ?")
                .bind(eid)
                .fetch_optional(&self.ctx.db)
                .await?
                .is_some(),
            None => false,
        };
        if exists {
            Ok(())
        } else {
            Err(ApiError::conflict(ENDPOINT_GONE))
        }
    }

    /// Failed samples go back to pending and the run runs. Returns how many were reset.
    pub async fn retry_failed(&self, run_id: i64) -> Result<i64, ApiError> {
        let lock = self.run_lock(run_id);
        let _g = lock.lock().await;
        if let Some(task) = self.task(run_id) {
            {
                let _tl = task.lock.lock().await;
                if !task.is_finalizing_or_cancelled() {
                    let n = reset_failed(&self.ctx.db, run_id).await?;
                    let pausing = {
                        let mut st = task.st.lock();
                        st.failed -= n;
                        st.consecutive_failures = 0;
                        if n > 0 {
                            st.rescan();
                        }
                        st.status == TaskStatus::Pausing
                    };
                    if n > 0 {
                        if pausing {
                            task.resume_locked().await?;
                        } else {
                            task.spawn_workers();
                        }
                    }
                    return Ok(n);
                }
            }
            task.wait().await;
        }
        let db = &self.ctx.db;
        let row: Option<(i64, Option<i64>)> =
            sqlx::query_as("SELECT failed, endpoint_id FROM runs WHERE id = ?").bind(run_id).fetch_optional(db).await?;
        let Some((failed, endpoint_id)) = row else { return Ok(0) };
        if failed == 0 {
            return Ok(0);
        }
        self.require_endpoint(endpoint_id).await?;
        let n = reset_failed(db, run_id).await?;
        if n > 0 {
            sqlx::query("UPDATE runs SET status = 'running', status_reason = NULL, finished_at = NULL WHERE id = ?")
                .bind(run_id)
                .execute(db)
                .await?;
            self.start_locked(run_id).await?;
        }
        Ok(n)
    }

    /// Apply a new concurrency to a running run at once.
    pub async fn set_concurrency(&self, run_id: i64, n: i64) {
        if let Some(task) = self.task(run_id) {
            task.set_concurrency(n).await;
        }
    }

    /// Stop a run's task at once (used before deleting it). Its status is left as is.
    pub async fn stop(&self, run_id: i64) {
        let lock = self.run_lock(run_id);
        let _g = lock.lock().await;
        if let Some(task) = self.task(run_id) {
            task.stop().await;
        }
    }

    /// On startup: runs left running or pausing become paused; their in-flight samples pending;
    /// their counters are recounted from the sample rows.
    pub async fn recover(&self) -> anyhow::Result<()> {
        let active: HashSet<i64> = self.tasks.lock().keys().copied().collect();
        let mut tx = self.ctx.db.begin().await?;
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM runs WHERE status IN ('running', 'pausing')")
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .filter(|id| !active.contains(id))
            .collect();
        for id in &ids {
            sqlx::query("UPDATE runs SET status = 'paused', status_reason = ? WHERE id = ?")
                .bind(RESTART_REASON)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE samples SET status = 'pending' WHERE run_id = ? AND status = 'running'")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            recount(&mut tx, *id).await?;
        }
        tx.commit().await?;
        if !ids.is_empty() {
            tracing::info!(runs = ?ids, "paused runs interrupted by a restart");
        }
        Ok(())
    }

    /// Stop every run: cancel in-flight requests (samples back to pending), flush, save run time.
    /// Run statuses stay as they are, so `recover` pauses them on the next start.
    pub async fn shutdown(&self) {
        let tasks: Vec<Arc<RunTask>> = self.tasks.lock().values().cloned().collect();
        futures::future::join_all(tasks.iter().map(|t| t.stop())).await;
        let slot = self.writer.lock().take();
        if let Some(w) = slot {
            w.handle.flush().await;
            w.join.abort();
        }
        self.tasks.lock().clear();
        self.locks.lock().clear();
    }

    /// Tests only: lose the writer's unsaved work and drop every run task without saving anything,
    /// as a crash would.
    #[doc(hidden)]
    pub async fn crash_for_tests(&self) {
        let slot = self.writer.lock().take();
        if let Some(w) = slot {
            w.join.abort();
            let _ = w.join.await;
        }
        let tasks: Vec<Arc<RunTask>> = self.tasks.lock().values().cloned().collect();
        futures::future::join_all(tasks.iter().map(|t| t.stop())).await;
        self.tasks.lock().clear();
        self.locks.lock().clear();
    }
}

impl RunsProbe for Engine {
    fn active_runs(&self) -> Vec<ActiveRun> {
        self.tasks
            .lock()
            .values()
            .map(|t| ActiveRun { run_id: t.run_id, endpoint_id: t.endpoint_id, prompt_set_id: t.prompt_set_id })
            .collect()
    }
}

async fn recount(tx: &mut sqlx::SqliteConnection, run_id: i64) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE runs SET \
         done = (SELECT COUNT(*) FROM samples WHERE run_id = ?1 AND status = 'done'), \
         failed = (SELECT COUNT(*) FROM samples WHERE run_id = ?1 AND status = 'failed'), \
         truncated = (SELECT COUNT(*) FROM samples WHERE run_id = ?1 AND status = 'done' AND finish_reason = 'length'), \
         prompt_tokens = (SELECT COALESCE(SUM(prompt_tokens), 0) FROM samples WHERE run_id = ?1 AND status = 'done'), \
         completion_tokens = (SELECT COALESCE(SUM(completion_tokens), 0) FROM samples WHERE run_id = ?1 AND status = 'done'), \
         reasoning_tokens = (SELECT COALESCE(SUM(reasoning_tokens), 0) FROM samples WHERE run_id = ?1 AND status = 'done') \
         WHERE id = ?1",
    )
    .bind(run_id)
    .execute(tx)
    .await?;
    Ok(())
}

async fn reset_failed(db: &SqlitePool, run_id: i64) -> sqlx::Result<i64> {
    let mut tx = db.begin().await?;
    let n = sqlx::query(
        "UPDATE samples SET status = 'pending', reasoning = NULL, response = NULL, finish_reason = NULL, \
         prompt_tokens = NULL, completion_tokens = NULL, reasoning_tokens = NULL, latency_ms = NULL, ttft_ms = NULL, \
         attempts = 0, error = NULL, completed_at = NULL WHERE run_id = ? AND status = 'failed'",
    )
    .bind(run_id)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;
    if n > 0 {
        sqlx::query("UPDATE runs SET failed = failed - ? WHERE id = ?").bind(n).bind(run_id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_shortens() {
        assert_eq!(preview("  a\n\n b  ", 160), "a b");
        let p = preview(&"word ".repeat(100), 160);
        assert_eq!(p.chars().count(), 160);
        assert!(p.ends_with('…'));
    }

    #[test]
    fn rates_window() {
        let mut r = Rates { started: Instant::now(), buckets: VecDeque::new() };
        r.add(10.0, 0.0);
        r.add(0.0, 2.0);
        assert_eq!(r.buckets.len(), 1);
        assert_eq!(r.tokens_per_s(10.0), 10.0); // the span is at least one second
        assert_eq!(r.samples_per_min(60.0), 120.0);
    }
}
