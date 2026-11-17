//! Background imports. `POST /api/imports` resolves the source (so a bad name or column answers
//! 4xx at once), creates the prompt set as "importing" and returns; a task then streams rows into
//! prompts. A blocking reader thread parses and builds prompts (JSON already encoded) and sends
//! them over a bounded channel to an async writer, which inserts ~5,000 per transaction and
//! updates the set's progress. Memory stays flat whatever the dataset's size.

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;
use futures::future::BoxFuture;
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;
use stellarator_core::probes::ImportsProbe;
use stellarator_core::schemas::{ImportMapping, ImportSource, PromptSetOut, PromptSetStatus, Turns};
use stellarator_core::{settings, time, ApiError, Ctx};
use tokio::sync::{mpsc, watch};

use crate::cancel::Cancel;
use crate::files::{open_path, Opened, TextDoc};
use crate::hf::{Hf, HfParquetTable};
use crate::problem::{Problem, Result};
use crate::prompts::{resolve_plan, window, Builder, Plan, Stats, TextPrompts, NO_PROMPTS_FOUND, NO_TEXT_COLUMN};
use crate::schema::infer_schema;
use crate::table::{Flow, Row, RowSink, Table};
use crate::text::split_text;
use crate::upload::{delete_upload, upload_file, MAX_UPLOAD_BYTES};

/// Prompts per database transaction.
pub const INSERT_BATCH: usize = 5000;
/// Prompts per message from the reader to the writer.
const SEND_BATCH: usize = 1000;
/// Rows per INSERT statement (4 parameters each, well under SQLite's limit).
const ROWS_PER_STATEMENT: usize = 500;
/// Messages the reader may run ahead of the writer.
const CHANNEL_DEPTH: usize = 4;
const PROGRESS_EVERY: Duration = Duration::from_secs(1);

pub const RESTARTED: &str = "Stopped when Stellarator restarted. Delete it and import again.";
const READER_CRASHED: &str = "Stellarator hit an error while importing. Check its logs, then import again.";
const DB_FAILED: &str = "The database hit an error while importing. Check the Stellarator logs, then import again.";

struct Job {
    cancel: Cancel,
    done: watch::Receiver<bool>,
}

/// The import jobs and the HTTP clients they use.
pub struct Imports {
    pub ctx: Ctx,
    /// For Parquet files on Hugging Face: no redirects, no overall timeout (large ranges).
    data_http: reqwest::Client,
    jobs: Mutex<HashMap<i64, Job>>,
    /// Upload size cap (1 GB; lowered in tests).
    pub max_upload: u64,
}

/// What a prepared import reads.
enum Work {
    /// Text pieces, already windowed.
    Pieces { pieces: Vec<String>, dedupe: bool },
    Table { table: Box<dyn Table>, plan: Plan, offset: u64, limit: Option<u64>, dedupe: bool },
}

/// A source resolved and ready to stream.
struct Prepared {
    kind: &'static str,
    info: Map<String, Value>,
    rows_total: Option<i64>,
    detail: Option<String>,
    upload_id: Option<String>,
    work: Work,
}

/// An opened upload, and for text its pieces and split mode.
type FileOpen = (Opened, Option<(Vec<String>, Value)>);

fn windowed_total(total: Option<u64>, mapping: &ImportMapping) -> Option<i64> {
    total.map(|t| {
        let t = t as i64;
        mapping.limit.unwrap_or(t).min(t - mapping.offset).max(0)
    })
}

fn blocking_error(e: tokio::task::JoinError) -> Problem {
    tracing::error!(error = %e, "import reader failed");
    Problem::status(500, READER_CRASHED)
}

fn turns_name(t: Turns) -> &'static str {
    match t {
        Turns::UpToLastUser => "up_to_last_user",
        Turns::FirstUser => "first_user",
    }
}

#[derive(sqlx::FromRow)]
struct SetRow {
    id: i64,
    name: String,
    source_kind: String,
    source: sqlx::types::Json<Value>,
    prompt_count: i64,
    created_at: NaiveDateTime,
    status: String,
    status_detail: Option<String>,
    rows_read: i64,
    rows_total: Option<i64>,
}

pub async fn load_set(db: &SqlitePool, id: i64) -> sqlx::Result<PromptSetOut> {
    let r: SetRow = sqlx::query_as(
        "SELECT id, name, source_kind, source, prompt_count, created_at, status, status_detail, rows_read, rows_total
         FROM prompt_sets WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await?;
    let run_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE prompt_set_id = ?").bind(id).fetch_one(db).await?;
    Ok(PromptSetOut {
        id: r.id,
        name: r.name,
        source_kind: r.source_kind,
        source: r.source.0,
        prompt_count: r.prompt_count,
        created_at: time::utc(r.created_at),
        run_count,
        status: PromptSetStatus::parse(&r.status),
        status_detail: r.status_detail,
        rows_read: r.rows_read,
        rows_total: r.rows_total,
    })
}

impl Imports {
    pub fn new(ctx: Ctx) -> Arc<Self> {
        Self::with_max_upload(ctx, MAX_UPLOAD_BYTES)
    }

    /// For tests: a smaller upload cap.
    pub fn with_max_upload(ctx: Ctx, max_upload: u64) -> Arc<Self> {
        let data_http = reqwest::Client::builder()
            .user_agent(concat!("stellarator/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Arc::new(Self { ctx, data_http, jobs: Mutex::new(HashMap::new()), max_upload })
    }

    pub(crate) async fn hf(&self) -> Hf {
        Hf {
            http: self.ctx.http.clone(),
            data: self.data_http.clone(),
            server: self.ctx.config.hf_datasets_server.clone(),
            hub: self.ctx.config.hf_hub.clone(),
            token: settings::hf_token(&self.ctx).await,
            file_requests: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Startup: sets left "importing" by a previous process failed; old uploads are removed.
    pub async fn recover(&self) -> anyhow::Result<()> {
        let n = sqlx::query("UPDATE prompt_sets SET status = 'failed', status_detail = ? WHERE status = 'importing'")
            .bind(RESTARTED)
            .execute(&self.ctx.db)
            .await?
            .rows_affected();
        if n > 0 {
            tracing::warn!(sets = n, "imports interrupted by a restart were marked failed");
        }
        let root = self.ctx.config.upload_dir();
        tokio::task::spawn_blocking(move || crate::upload::cleanup_old_uploads(&root, crate::upload::UPLOAD_MAX_AGE)).await?;
        Ok(())
    }

    /// Stop every import and wait until they have stopped writing. Their sets stay "importing" and
    /// are marked failed by `recover` at the next start.
    pub async fn shutdown(&self) {
        let jobs: Vec<(Cancel, watch::Receiver<bool>)> = {
            let map = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
            map.values().map(|j| (j.cancel.clone(), j.done.clone())).collect()
        };
        for (cancel, _) in &jobs {
            cancel.cancel();
        }
        for (_, mut done) in jobs {
            let _ = done.wait_for(|d| *d).await;
        }
    }

    /// Resolve a source: everything that can fail because of the request fails here.
    async fn prepare(&self, source: &ImportSource, mapping: &ImportMapping, cancel: &Cancel) -> Result<Prepared> {
        match source {
            ImportSource::Paste { text } => {
                if text.trim().is_empty() {
                    return Err(Problem::new("Paste some text to import.").on("source.text"));
                }
                let doc = TextDoc { text: text.clone(), kind: "paste", warnings: Vec::new() };
                let mapping2 = mapping.clone();
                let (pieces, mode) = tokio::task::spawn_blocking(move || text_work(&doc, &mapping2))
                    .await
                    .map_err(blocking_error)?;
                let mut info = Map::new();
                info.insert("format".into(), json!("paste"));
                info.insert("split_mode".into(), json!(mode));
                if pieces.is_empty() {
                    return Err(Problem::new(NO_PROMPTS_FOUND));
                }
                Ok(Prepared {
                    kind: "paste",
                    info,
                    rows_total: Some(pieces.len() as i64),
                    detail: None,
                    upload_id: None,
                    work: Work::Pieces { pieces, dedupe: mapping.dedupe },
                })
            }
            ImportSource::File { upload_id, format } => {
                let path = upload_file(&self.ctx.config.upload_dir(), upload_id)?;
                let filename = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let (fmt, name, m) = (*format, filename.clone(), mapping.clone());
                let opened = tokio::task::spawn_blocking(move || -> Result<FileOpen> {
                    let opened = open_path(&path, fmt, Some(&name))?;
                    let text = match &opened {
                        Opened::Text(doc) => {
                            let (pieces, mode) = text_work(doc, &m);
                            Some((pieces, json!(mode)))
                        }
                        Opened::Table(_) => None,
                    };
                    Ok((opened, text))
                })
                .await
                .map_err(blocking_error)??;
                let mut info = Map::new();
                info.insert("filename".into(), json!(filename));
                let detail = Some(format!("Reading {filename}"));
                match opened {
                    (Opened::Text(doc), Some((pieces, mode))) => {
                        info.insert("format".into(), json!(doc.kind));
                        info.insert("split_mode".into(), mode);
                        if pieces.is_empty() {
                            return Err(Problem::new(NO_PROMPTS_FOUND));
                        }
                        Ok(Prepared {
                            kind: "file",
                            info,
                            rows_total: Some(pieces.len() as i64),
                            detail,
                            upload_id: Some(upload_id.clone()),
                            work: Work::Pieces { pieces, dedupe: mapping.dedupe },
                        })
                    }
                    (Opened::Table(src), _) => {
                        let (plan, _, _) = resolve_plan(&src.schema, &src.sample, mapping)?;
                        let plan = plan.ok_or_else(|| Problem::new(NO_TEXT_COLUMN))?;
                        info.insert("format".into(), json!(src.table.format()));
                        describe_plan(&mut info, &plan);
                        Ok(Prepared {
                            kind: "file",
                            info,
                            rows_total: windowed_total(src.table.total_rows(), mapping),
                            detail,
                            upload_id: Some(upload_id.clone()),
                            work: Work::Table {
                                table: src.table,
                                plan,
                                offset: mapping.offset.max(0) as u64,
                                limit: mapping.limit.map(|l| l.max(0) as u64),
                                dedupe: mapping.dedupe,
                            },
                        })
                    }
                    (Opened::Text(_), None) => Err(Problem::status(500, READER_CRASHED)),
                }
            }
            ImportSource::Huggingface { dataset, config, split, all_splits } => {
                let hf = self.hf().await;
                let ds = hf.dataset(dataset, config.as_deref(), split.as_deref(), *all_splits).await?;
                let first = &ds.first_rows;
                let schema = infer_schema(&first.names, &first.rows, false, &first.hints);
                let (plan, _, _) = resolve_plan(&schema, &first.rows, mapping)?;
                let plan = plan.ok_or_else(|| Problem::new(NO_TEXT_COLUMN))?;
                let (shards, partial) = hf.parquet_files(&ds.dataset, &ds.config, &ds.splits).await?;
                let split_rows = split_rows(&ds, partial);
                let total = if partial { None } else { ds.num_rows };
                let table = HfParquetTable::open(&hf, shards, total, split_rows, cancel.clone()).await?;
                plan.bind(table.names())?;
                let mut info = Map::new();
                info.insert("dataset".into(), json!(ds.dataset));
                info.insert("config".into(), json!(ds.config));
                if *all_splits {
                    info.insert("all_splits".into(), json!(true));
                    info.insert("splits".into(), json!(ds.splits));
                } else {
                    info.insert("split".into(), json!(ds.splits.first()));
                }
                describe_plan(&mut info, &plan);
                if partial {
                    info.insert("partial".into(), json!(true));
                }
                Ok(Prepared {
                    kind: "huggingface",
                    info,
                    rows_total: windowed_total(total, mapping),
                    detail: ds.splits.first().map(|s| format!("Reading {s}")),
                    upload_id: None,
                    work: Work::Table {
                        table: Box::new(table),
                        plan,
                        offset: mapping.offset.max(0) as u64,
                        limit: mapping.limit.map(|l| l.max(0) as u64),
                        dedupe: mapping.dedupe,
                    },
                })
            }
        }
    }

    /// Validate and resolve the source, create the prompt set ("importing") and start the import.
    pub async fn start(self: &Arc<Self>, source: &ImportSource, mapping: &ImportMapping, name: &str) -> std::result::Result<PromptSetOut, ApiError> {
        let cancel = Cancel::new();
        let prepared = self.prepare(source, mapping, &cancel).await?;
        let name = if name.trim().is_empty() { name.to_string() } else { name.trim().to_string() };
        let id = sqlx::query(
            "INSERT INTO prompt_sets (name, source_kind, source, prompt_count, created_at, status, status_detail, rows_read, rows_total)
             VALUES (?, ?, ?, 0, ?, 'importing', ?, 0, ?)",
        )
        .bind(&name)
        .bind(prepared.kind)
        .bind(sqlx::types::Json(Value::Object(prepared.info.clone())))
        .bind(time::now_db())
        .bind(&prepared.detail)
        .bind(prepared.rows_total)
        .execute(&self.ctx.db)
        .await?
        .last_insert_rowid();
        // Read back before the job starts, so the answer always shows the set as created.
        let out = load_set(&self.ctx.db, id).await?;
        let (done_tx, done_rx) = watch::channel(false);
        {
            let mut jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
            jobs.insert(id, Job { cancel: cancel.clone(), done: done_rx });
        }
        let me = Arc::clone(self);
        tokio::spawn(async move {
            me.run(id, prepared, cancel).await;
            me.jobs.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
            let _ = done_tx.send(true);
        });
        Ok(out)
    }

    async fn run(&self, id: i64, prepared: Prepared, cancel: Cancel) {
        let Prepared { info, rows_total, detail, upload_id, work, .. } = prepared;
        let started = Instant::now();
        let (tx, mut rx) = mpsc::channel::<Msg>(CHANNEL_DEPTH);
        let reader_cancel = cancel.clone();
        let reader = tokio::task::spawn_blocking(move || produce(work, tx, reader_cancel));
        let mut writer = Writer::new(self.ctx.db.clone(), id, detail);
        let mut db_error = false;
        while let Some(msg) = rx.recv().await {
            if cancel.is_cancelled() {
                break;
            }
            writer.take(msg);
            if writer.due() {
                if let Err(e) = writer.flush().await {
                    tracing::error!(error = %e, set = id, "import write failed");
                    db_error = true;
                    cancel.cancel();
                    break;
                }
            }
        }
        drop(rx);
        let outcome = match reader.await {
            Ok(r) => r,
            Err(e) => Err(blocking_error(e)),
        };
        if !db_error && !cancel.is_cancelled() {
            if let Err(e) = writer.flush().await {
                tracing::error!(error = %e, set = id, "import write failed");
                db_error = true;
            }
        }
        let written = writer.written as i64;
        let result = match (db_error, outcome) {
            (true, _) => Err(Problem::status(500, DB_FAILED)),
            (false, Err(p)) if p.is_cancelled() => {
                tracing::info!(set = id, written, "import stopped");
                return;
            }
            (false, _) if cancel.is_cancelled() => {
                tracing::info!(set = id, written, "import stopped");
                return;
            }
            (false, Err(p)) => Err(p),
            (false, Ok(_)) if written == 0 => Err(Problem::new(NO_PROMPTS_FOUND)),
            (false, Ok(stats)) => Ok(stats),
        };
        let db = &self.ctx.db;
        let update = match &result {
            Ok(stats) => {
                let mut source = info.clone();
                source.insert("rows".into(), json!(written));
                let warnings = stats.warnings();
                if !warnings.is_empty() {
                    source.insert("warnings".into(), json!(warnings));
                }
                sqlx::query(
                    "UPDATE prompt_sets SET status = 'ready', status_detail = NULL, prompt_count = ?, rows_read = ?,
                     rows_total = ?, source = ? WHERE id = ?",
                )
                .bind(written)
                .bind(writer.rows_read as i64)
                .bind(writer.rows_read as i64)
                .bind(sqlx::types::Json(Value::Object(source)))
                .bind(id)
                .execute(db)
                .await
            }
            Err(p) => {
                sqlx::query("UPDATE prompt_sets SET status = 'failed', status_detail = ?, prompt_count = ?, rows_read = ? WHERE id = ?")
                    .bind(&p.message)
                    .bind(written)
                    .bind(writer.rows_read as i64)
                    .bind(id)
                    .execute(db)
                    .await
            }
        };
        if let Err(e) = update {
            tracing::error!(error = %e, set = id, "couldn't record the import's outcome");
        }
        match &result {
            Ok(_) => {
                tracing::info!(set = id, prompts = written, rows = writer.rows_read, total = ?rows_total, secs = started.elapsed().as_secs_f64(), "import finished");
                if let Some(u) = upload_id {
                    let root = self.ctx.config.upload_dir();
                    let _ = tokio::task::spawn_blocking(move || delete_upload(&root, &u)).await;
                }
            }
            Err(p) => tracing::warn!(set = id, prompts = written, reason = %p.message, "import failed"),
        }
    }
}

fn split_rows(ds: &crate::hf::HfDataset, partial: bool) -> HashMap<String, u64> {
    if partial {
        return HashMap::new();
    }
    ds.hf_splits
        .iter()
        .filter(|s| s.config == ds.config)
        .filter_map(|s| s.num_rows.map(|n| (s.split.clone(), n.max(0) as u64)))
        .collect()
}

fn describe_plan(info: &mut Map<String, Value>, plan: &Plan) {
    info.insert("prompt_field".into(), json!(plan.prompt_field));
    if let Some(s) = &plan.system_field {
        info.insert("system_field".into(), json!(s));
    }
    if plan.has_conversations {
        info.insert("turns".into(), json!(turns_name(plan.turns)));
    }
}

/// The text pieces a commit imports (window applied) and the split mode used.
fn text_work(doc: &TextDoc, mapping: &ImportMapping) -> (Vec<String>, stellarator_core::schemas::SplitMode) {
    let (pieces, mode) = split_text(&doc.text, mapping.split_mode, doc.markdown());
    (window(&pieces, mapping).to_vec(), mode)
}

// ------------------------------------------------------------------------------------ reader

enum Msg {
    /// Encoded (messages, meta) and the source rows read so far.
    Prompts(Vec<(String, String)>, u64),
    Progress(u64),
    /// A new part started: "Reading test".
    Part(String),
}

struct JobSink {
    builder: Builder,
    batch: Vec<(String, String)>,
    rows_read: u64,
    last_progress: Instant,
    tx: mpsc::Sender<Msg>,
    cancel: Cancel,
    closed: bool,
}

impl JobSink {
    fn send(&mut self, msg: Msg) -> Flow {
        if self.tx.blocking_send(msg).is_err() {
            self.closed = true;
            return Flow::Stop;
        }
        Flow::Continue
    }
    fn flush(&mut self) -> Flow {
        if self.batch.is_empty() {
            return Flow::Continue;
        }
        let batch = std::mem::take(&mut self.batch);
        self.last_progress = Instant::now();
        self.send(Msg::Prompts(batch, self.rows_read))
    }
}

impl RowSink for JobSink {
    fn row(&mut self, row: Row) -> Flow {
        if self.cancel.is_cancelled() || self.closed {
            return Flow::Stop;
        }
        self.rows_read += 1;
        if let Some(p) = self.builder.take(&row) {
            self.batch.push(p.encode());
        }
        if self.batch.len() >= SEND_BATCH {
            return self.flush();
        }
        if self.last_progress.elapsed() >= PROGRESS_EVERY {
            self.last_progress = Instant::now();
            return self.send(Msg::Progress(self.rows_read));
        }
        Flow::Continue
    }
    fn part(&mut self, name: &str) {
        if self.flush() == Flow::Continue {
            let _ = self.send(Msg::Part(format!("Reading {name}")));
        }
    }
}

/// Runs on a blocking thread: source rows in, encoded prompts out.
fn produce(work: Work, tx: mpsc::Sender<Msg>, cancel: Cancel) -> Result<Stats> {
    match work {
        Work::Pieces { pieces, dedupe } => {
            let mut tp = TextPrompts::new(dedupe);
            let mut batch = Vec::with_capacity(SEND_BATCH);
            let mut read = 0u64;
            for piece in &pieces {
                if cancel.is_cancelled() {
                    return Err(Problem::cancelled());
                }
                read += 1;
                if let Some(p) = tp.take(piece) {
                    batch.push(p.encode());
                }
                if batch.len() >= SEND_BATCH && tx.blocking_send(Msg::Prompts(std::mem::take(&mut batch), read)).is_err() {
                    return Err(Problem::cancelled());
                }
            }
            if tx.blocking_send(Msg::Prompts(batch, read)).is_err() {
                return Err(Problem::cancelled());
            }
            Ok(tp.stats)
        }
        Work::Table { mut table, plan, offset, limit, dedupe } => {
            let bound = plan.bind(table.names())?;
            let columns = bound.columns();
            let mut sink = JobSink {
                builder: Builder::new(bound, dedupe),
                batch: Vec::with_capacity(SEND_BATCH),
                rows_read: 0,
                last_progress: Instant::now(),
                tx,
                cancel: cancel.clone(),
                closed: false,
            };
            table.scan(Some(&columns), offset, limit, &mut sink)?;
            if cancel.is_cancelled() || sink.flush() == Flow::Stop {
                return Err(Problem::cancelled());
            }
            let _ = sink.send(Msg::Progress(sink.rows_read));
            Ok(sink.builder.stats)
        }
    }
}

// ------------------------------------------------------------------------------------ writer

struct Writer {
    db: SqlitePool,
    id: i64,
    pending: Vec<(String, String)>,
    written: u64,
    rows_read: u64,
    detail: Option<String>,
    dirty: bool,
    last_flush: Instant,
}

impl Writer {
    fn new(db: SqlitePool, id: i64, detail: Option<String>) -> Self {
        Self { db, id, pending: Vec::new(), written: 0, rows_read: 0, detail, dirty: false, last_flush: Instant::now() }
    }

    fn take(&mut self, msg: Msg) {
        match msg {
            Msg::Prompts(rows, read) => {
                self.pending.extend(rows);
                self.rows_read = read;
            }
            Msg::Progress(read) => self.rows_read = read,
            Msg::Part(detail) => self.detail = Some(detail),
        }
        self.dirty = true;
    }

    fn due(&self) -> bool {
        self.pending.len() >= INSERT_BATCH || (self.dirty && self.last_flush.elapsed() >= PROGRESS_EVERY)
    }

    /// Insert pending prompts and record progress, in one transaction.
    async fn flush(&mut self) -> sqlx::Result<()> {
        if !self.dirty && self.pending.is_empty() {
            return Ok(());
        }
        let mut tx = self.db.begin().await?;
        for chunk in self.pending.chunks(ROWS_PER_STATEMENT) {
            let mut sql = String::with_capacity(64 + chunk.len() * 14);
            sql.push_str("INSERT INTO prompts (prompt_set_id, idx, messages, meta) VALUES ");
            for i in 0..chunk.len() {
                sql.push_str(if i == 0 { "(?, ?, ?, ?)" } else { ",(?, ?, ?, ?)" });
            }
            let mut q = sqlx::query(&sql);
            for (i, (messages, meta)) in chunk.iter().enumerate() {
                q = q.bind(self.id).bind((self.written + i as u64) as i64).bind(messages.as_str()).bind(meta.as_str());
            }
            q.execute(&mut *tx).await?;
            self.written += chunk.len() as u64;
        }
        sqlx::query("UPDATE prompt_sets SET prompt_count = ?, rows_read = ?, status_detail = ? WHERE id = ?")
            .bind(self.written as i64)
            .bind(self.rows_read as i64)
            .bind(&self.detail)
            .bind(self.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.pending.clear();
        self.dirty = false;
        self.last_flush = Instant::now();
        Ok(())
    }
}

// ------------------------------------------------------------------------------------ probe

impl ImportsProbe for Imports {
    fn is_importing(&self, prompt_set_id: i64) -> bool {
        self.jobs.lock().unwrap_or_else(|p| p.into_inner()).contains_key(&prompt_set_id)
    }

    fn cancel(&self, prompt_set_id: i64) -> BoxFuture<'_, ()> {
        let job = {
            let jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
            jobs.get(&prompt_set_id).map(|j| (j.cancel.clone(), j.done.clone()))
        };
        Box::pin(async move {
            if let Some((cancel, mut done)) = job {
                cancel.cancel();
                let _ = done.wait_for(|d| *d).await;
            }
        })
    }
}
