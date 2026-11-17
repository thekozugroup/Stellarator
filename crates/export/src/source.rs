//! Reading export records out of the database in keyset-paged batches. A producer task sends one
//! `Vec<Value>` per database page through a small bounded channel, so memory stays flat however
//! many rows there are, and the task stops as soon as the consumer goes away.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::NaiveDateTime;
use serde_json::Value;
use sqlx::types::Json;
use sqlx::{FromRow, SqlitePool};
use stellarator_core::schemas::{ExportOptions, PrefExportOptions, PrefFormat};
use tokio::sync::mpsc;

use crate::records::{pref_records, sft_record, RunInfo, SampleRec};

pub const BATCH: i64 = 500;

pub type Batch = anyhow::Result<Vec<Value>>;

#[derive(Clone)]
pub enum Source {
    Sft { run: Arc<RunInfo>, options: ExportOptions },
    Pref { set_id: i64, options: PrefExportOptions },
}

/// Start reading. Drop the receiver to stop.
pub fn spawn(db: SqlitePool, source: Source, batch: i64) -> mpsc::Receiver<Batch> {
    let (tx, rx) = mpsc::channel(2);
    tokio::spawn(async move {
        let result = match &source {
            Source::Sft { run, options } => sft_loop(&db, run, options, batch, &tx).await,
            Source::Pref { set_id, options } => pref_loop(&db, *set_id, options, batch, &tx).await,
        };
        if let Err(e) = result {
            // A closed channel means the consumer left; anything else is a real error to report.
            let _ = tx.send(Err(e)).await;
        }
    });
    rx
}

// ------------------------------------------------------------------------------ SFT filters

fn blank(col: &str) -> String {
    format!("({col} IS NULL OR trim({col}, char(32, 9, 13, 10)) = '')")
}

pub struct SftConditions {
    pub done: String,
    pub empty: String,
    pub extra: String,
    pub trunc: String,
    pub noreas: String,
}

impl SftConditions {
    pub fn new(o: &ExportOptions) -> Self {
        Self {
            done: "status = 'done'".into(),
            empty: format!("({} AND {})", blank("response"), blank("reasoning")),
            extra: if o.first_sample_only { "sample_idx > 0".into() } else { "0".into() },
            trunc: if o.exclude_truncated { "coalesce(finish_reason, '') = 'length'".into() } else { "0".into() },
            noreas: if o.require_reasoning { blank("reasoning") } else { "0".into() },
        }
    }

    pub fn keep(&self) -> String {
        format!(
            "({} AND NOT {} AND NOT ({}) AND NOT ({}) AND NOT ({}))",
            self.done, self.empty, self.extra, self.trunc, self.noreas
        )
    }
}

/// (records kept, skipped reasons with non-zero counts) for a run.
pub async fn sft_counts(
    db: &SqlitePool,
    run_id: i64,
    o: &ExportOptions,
) -> Result<(i64, Vec<(&'static str, i64)>), sqlx::Error> {
    let c = SftConditions::new(o);
    let n = |cond: &str| format!("coalesce(sum(CASE WHEN {cond} THEN 1 ELSE 0 END), 0)");
    let sql = format!(
        "SELECT {}, {}, {}, {}, {}, {}, {} FROM samples WHERE run_id = ?",
        n(&c.keep()),
        n("status = 'failed'"),
        n("status IN ('pending', 'running')"),
        n(&format!("{} AND {}", c.done, c.empty)),
        n(&format!("{} AND NOT {} AND ({})", c.done, c.empty, c.extra)),
        n(&format!("{} AND NOT {} AND NOT ({}) AND ({})", c.done, c.empty, c.extra, c.trunc)),
        n(&format!(
            "{} AND NOT {} AND NOT ({}) AND NOT ({}) AND ({})",
            c.done, c.empty, c.extra, c.trunc, c.noreas
        )),
    );
    let (keep, failed, pending, empty, extra, trunc, noreas): (i64, i64, i64, i64, i64, i64, i64) =
        sqlx::query_as(&sql).bind(run_id).fetch_one(db).await?;
    let skipped = [
        ("failed", failed),
        ("pending", pending),
        ("truncated", trunc),
        ("no_reasoning", noreas),
        ("extra_samples", extra),
        ("empty_response", empty),
    ]
    .into_iter()
    .filter(|(_, v)| *v != 0)
    .collect();
    Ok((keep, skipped))
}

#[derive(FromRow)]
struct SftRow {
    id: i64,
    run_id: i64,
    prompt_id: i64,
    prompt_idx: i64,
    sample_idx: i64,
    reasoning: Option<String>,
    response: Option<String>,
    finish_reason: Option<String>,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
    latency_ms: Option<i64>,
    ttft_ms: Option<i64>,
    completed_at: Option<NaiveDateTime>,
    messages: Json<Value>,
    meta: Json<Value>,
}

impl SftRow {
    fn sample(&self) -> SampleRec {
        SampleRec {
            id: self.id,
            run_id: self.run_id,
            prompt_id: self.prompt_id,
            prompt_idx: self.prompt_idx,
            sample_idx: self.sample_idx,
            reasoning: self.reasoning.clone(),
            response: self.response.clone(),
            finish_reason: self.finish_reason.clone(),
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            reasoning_tokens: self.reasoning_tokens,
            latency_ms: self.latency_ms,
            ttft_ms: self.ttft_ms,
            completed_at: self.completed_at,
        }
    }
}

async fn sft_loop(
    db: &SqlitePool,
    run: &RunInfo,
    options: &ExportOptions,
    batch: i64,
    tx: &mpsc::Sender<Batch>,
) -> anyhow::Result<()> {
    // The column names in the filters exist only on `samples`, so they need no alias.
    let keep = SftConditions::new(options).keep();
    let sql = format!(
        "SELECT s.id, s.run_id, s.prompt_id, s.prompt_idx, s.sample_idx, s.reasoning, s.response, \
         s.finish_reason, s.prompt_tokens, s.completion_tokens, s.reasoning_tokens, s.latency_ms, \
         s.ttft_ms, s.completed_at, p.messages, p.meta \
         FROM samples s JOIN prompts p ON p.id = s.prompt_id \
         WHERE s.run_id = ?1 AND {keep} AND (s.prompt_idx > ?2 OR (s.prompt_idx = ?2 AND s.sample_idx > ?3)) \
         ORDER BY s.prompt_idx, s.sample_idx LIMIT ?4"
    );
    let mut cursor = (i64::MIN, i64::MIN);
    loop {
        let rows: Vec<SftRow> =
            sqlx::query_as(&sql).bind(run.id).bind(cursor.0).bind(cursor.1).bind(batch).fetch_all(db).await?;
        let n = rows.len() as i64;
        let Some(last) = rows.last() else { return Ok(()) };
        cursor = (last.prompt_idx, last.sample_idx);
        let records: Vec<Value> =
            rows.iter().map(|r| sft_record(options, run, &r.sample(), &r.messages.0, Some(&r.meta.0))).collect();
        if tx.send(Ok(records)).await.is_err() {
            return Ok(());
        }
        if n < batch {
            return Ok(());
        }
    }
}

// ------------------------------------------------------------------------------ preferences

pub async fn pref_count(db: &SqlitePool, set_id: i64, o: &PrefExportOptions) -> Result<i64, sqlx::Error> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM preference_pairs WHERE preference_set_id = ?")
        .bind(set_id)
        .fetch_one(db)
        .await?;
    Ok(n * if o.format == PrefFormat::Kto { 2 } else { 1 })
}

#[derive(FromRow)]
struct PairRow {
    chosen_sample_id: i64,
    rejected_sample_id: i64,
    idx: i64,
    messages: Json<Value>,
}

#[derive(FromRow)]
struct PrefSampleRow {
    id: i64,
    run_id: i64,
    prompt_id: i64,
    prompt_idx: i64,
    sample_idx: i64,
    reasoning: Option<String>,
    response: Option<String>,
}

async fn pref_loop(
    db: &SqlitePool,
    set_id: i64,
    options: &PrefExportOptions,
    batch: i64,
    tx: &mpsc::Sender<Batch>,
) -> anyhow::Result<()> {
    let mut cursor = i64::MIN;
    let mut run_params: HashMap<i64, Value> = HashMap::new();
    loop {
        let pairs: Vec<PairRow> = sqlx::query_as(
            "SELECT pp.chosen_sample_id, pp.rejected_sample_id, p.idx, p.messages \
             FROM preference_pairs pp JOIN prompts p ON p.id = pp.prompt_id \
             WHERE pp.preference_set_id = ? AND p.idx > ? ORDER BY p.idx LIMIT ?",
        )
        .bind(set_id)
        .bind(cursor)
        .bind(batch)
        .fetch_all(db)
        .await?;
        let Some(last) = pairs.last() else { return Ok(()) };
        cursor = last.idx;

        let mut ids: Vec<i64> = pairs.iter().flat_map(|p| [p.chosen_sample_id, p.rejected_sample_id]).collect();
        ids.sort_unstable();
        ids.dedup();
        let marks = vec!["?"; ids.len()].join(",");
        let sql = format!(
            "SELECT id, run_id, prompt_id, prompt_idx, sample_idx, reasoning, response FROM samples WHERE id IN ({marks})"
        );
        let mut q = sqlx::query_as::<_, PrefSampleRow>(&sql);
        for id in &ids {
            q = q.bind(id);
        }
        let samples: HashMap<i64, SampleRec> = q
            .fetch_all(db)
            .await?
            .into_iter()
            .map(|r| {
                (
                    r.id,
                    SampleRec {
                        id: r.id,
                        run_id: r.run_id,
                        prompt_id: r.prompt_id,
                        prompt_idx: r.prompt_idx,
                        sample_idx: r.sample_idx,
                        reasoning: r.reasoning,
                        response: r.response,
                        ..Default::default()
                    },
                )
            })
            .collect();

        let missing: Vec<i64> = {
            let mut m: Vec<i64> =
                samples.values().map(|s| s.run_id).filter(|r| !run_params.contains_key(r)).collect();
            m.sort_unstable();
            m.dedup();
            m
        };
        for rid in missing {
            let p: Option<Json<Value>> =
                sqlx::query_scalar("SELECT params FROM runs WHERE id = ?").bind(rid).fetch_optional(db).await?;
            run_params.insert(rid, p.map(|j| j.0).unwrap_or(Value::Null));
        }

        let mut records = Vec::new();
        for pair in &pairs {
            let (Some(c), Some(r)) = (samples.get(&pair.chosen_sample_id), samples.get(&pair.rejected_sample_id))
            else {
                continue;
            };
            let params = run_params.get(&c.run_id).cloned().unwrap_or(Value::Null);
            records.extend(pref_records(options, c, r, &pair.messages.0, &params));
        }
        let n = pairs.len() as i64;
        if tx.send(Ok(records)).await.is_err() {
            return Ok(());
        }
        if n < batch {
            return Ok(());
        }
    }
}
