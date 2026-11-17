//! Preference sets: pick a chosen and a rejected response per prompt, by hand or by rule.
//!
//! A preference set groups one or more runs over the same prompt set. For each prompt, the
//! candidates are every finished (status "done") sample from those runs. Only prompts with at
//! least two candidates can be paired; those are the set's prompts.

use std::collections::{BTreeSet, HashMap};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::Value;
use sqlx::types::Json as Db;
use stellarator_core::schemas::{
    AutoPairOut, Candidate, Message, Page, PreferenceItem, PreferenceSetCreate, PreferenceSetOut, RunRef,
};
use stellarator_core::time::{now_db, utc};
use stellarator_core::{no_validation, ApiError, ApiJson, ApiQuery, Ctx, FieldErrors};

use super::{check_range, unprocessable, P};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/preference-sets", get(list_sets).post(create_set))
        .route("/api/preference-sets/{id}", get(read_set).delete(delete_set))
        .route("/api/preference-sets/{id}/items", get(list_items))
        .route("/api/preference-sets/{id}/pairs/{prompt_id}", put(put_pair).delete(delete_pair))
        .route("/api/preference-sets/{id}/auto-pair", post(auto_pair))
}

// The request bodies are declared here (not taken from core's PairIn / AutoPairIn) because the
// Python API checks "two different ids" inside the handler, after the 404 checks, with its own
// sentences.
#[derive(Deserialize)]
struct PairBody {
    chosen_sample_id: i64,
    rejected_sample_id: i64,
}

#[derive(Deserialize)]
struct AutoPairBody {
    chosen_run_id: i64,
    rejected_run_id: i64,
    #[serde(default)]
    overwrite: bool,
}

no_validation!(PairBody, AutoPairBody);

#[derive(sqlx::FromRow)]
struct SetRow {
    id: i64,
    name: String,
    prompt_set_id: i64,
    run_ids: Db<Vec<i64>>,
    created_at: NaiveDateTime,
}

#[derive(sqlx::FromRow)]
struct RunRow {
    id: i64,
    name: String,
    model: String,
}

async fn get_set(ctx: &Ctx, id: i64) -> Result<SetRow, ApiError> {
    sqlx::query_as::<_, SetRow>("SELECT id, name, prompt_set_id, run_ids, created_at FROM preference_sets WHERE id = ?")
        .bind(id)
        .fetch_optional(&ctx.db)
        .await?
        .ok_or_else(|| ApiError::not_found("Preference set not found. It may have been deleted."))
}

/// A comma-separated list of integers for `IN (...)`. Integers can't carry SQL, and it avoids
/// SQLite's bound-parameter cap for long lists.
fn id_list(ids: &[i64]) -> String {
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}

/// The runs of the set that still exist, in the order they were added.
async fn runs_of(ctx: &Ctx, set: &SetRow) -> Result<Vec<RunRow>, ApiError> {
    if set.run_ids.0.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<_, RunRow>(&format!(
        "SELECT id, name, model FROM runs WHERE id IN ({})",
        id_list(&set.run_ids.0)
    ))
    .fetch_all(&ctx.db)
    .await?;
    let mut by_id: HashMap<i64, RunRow> = rows.into_iter().map(|r| (r.id, r)).collect();
    Ok(set.run_ids.0.iter().filter_map(|id| by_id.remove(id)).collect())
}

/// Prompt ids with at least two finished samples across the runs (SQL for a subquery).
fn eligible_sql(run_ids: &[i64]) -> String {
    format!(
        "SELECT prompt_id FROM samples WHERE run_id IN ({}) AND status = 'done' \
         GROUP BY prompt_id HAVING COUNT(*) >= 2",
        id_list(run_ids)
    )
}

async fn prompt_count(ctx: &Ctx, run_ids: &[i64]) -> Result<i64, ApiError> {
    if run_ids.is_empty() {
        return Ok(0);
    }
    Ok(sqlx::query_scalar(&format!("SELECT COUNT(*) FROM ({})", eligible_sql(run_ids))).fetch_one(&ctx.db).await?)
}

async fn decided_count(ctx: &Ctx, set_id: i64) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM preference_pairs WHERE preference_set_id = ?")
        .bind(set_id)
        .fetch_one(&ctx.db)
        .await?)
}

async fn set_out(ctx: &Ctx, set: SetRow, decided: i64, prompt_set_name: Option<String>) -> Result<PreferenceSetOut, ApiError> {
    let runs = runs_of(ctx, &set).await?;
    let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();
    let prompt_set_name = match prompt_set_name {
        Some(n) => n,
        None => sqlx::query_scalar("SELECT name FROM prompt_sets WHERE id = ?")
            .bind(set.prompt_set_id)
            .fetch_optional(&ctx.db)
            .await?
            .unwrap_or_default(),
    };
    Ok(PreferenceSetOut {
        id: set.id,
        name: set.name,
        prompt_set_id: set.prompt_set_id,
        prompt_set_name,
        prompt_count: prompt_count(ctx, &run_ids).await?,
        run_ids,
        runs: runs.into_iter().map(|r| RunRef { id: r.id, name: r.name, model: r.model }).collect(),
        decided,
        created_at: utc(set.created_at),
    })
}

// ------------------------------------------------------------------------------------ sets

async fn list_sets(State(state): State<AppState>) -> Result<Json<Vec<PreferenceSetOut>>, ApiError> {
    let ctx = &state.ctx;
    let sets = sqlx::query_as::<_, SetRow>(
        "SELECT id, name, prompt_set_id, run_ids, created_at FROM preference_sets ORDER BY created_at DESC, id DESC",
    )
    .fetch_all(&ctx.db)
    .await?;
    let decided: HashMap<i64, i64> =
        sqlx::query_as::<_, (i64, i64)>("SELECT preference_set_id, COUNT(*) FROM preference_pairs GROUP BY preference_set_id")
            .fetch_all(&ctx.db)
            .await?
            .into_iter()
            .collect();
    let names: HashMap<i64, String> = sqlx::query_as::<_, (i64, String)>("SELECT id, name FROM prompt_sets")
        .fetch_all(&ctx.db)
        .await?
        .into_iter()
        .collect();
    let mut out = Vec::with_capacity(sets.len());
    for s in sets {
        let d = decided.get(&s.id).copied().unwrap_or(0);
        let name = names.get(&s.prompt_set_id).cloned().unwrap_or_default();
        out.push(set_out(ctx, s, d, Some(name)).await?);
    }
    Ok(Json(out))
}

async fn create_set(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<PreferenceSetCreate>,
) -> Result<(StatusCode, Json<PreferenceSetOut>), ApiError> {
    let ctx = &state.ctx;
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::field("name", "Enter a name for the preference set."));
    }
    // Drop repeats, keep order.
    let mut seen = BTreeSet::new();
    let run_ids: Vec<i64> = body.run_ids.iter().copied().filter(|id| seen.insert(*id)).collect();
    let runs: Vec<(i64, i64)> =
        sqlx::query_as(&format!("SELECT id, prompt_set_id FROM runs WHERE id IN ({})", id_list(&run_ids)))
            .fetch_all(&ctx.db)
            .await?;
    if runs.len() != run_ids.len() {
        return Err(unprocessable("One of the chosen runs no longer exists. Refresh the list and choose again."));
    }
    let prompt_set_id = runs[0].1;
    if runs.iter().any(|r| r.1 != prompt_set_id) {
        return Err(unprocessable("All runs must use the same prompt set."));
    }
    let id = sqlx::query("INSERT INTO preference_sets (name, prompt_set_id, run_ids, created_at) VALUES (?, ?, ?, ?)")
        .bind(&name)
        .bind(prompt_set_id)
        .bind(Db(&run_ids))
        .bind(now_db())
        .execute(&ctx.db)
        .await?
        .last_insert_rowid();
    let set = get_set(ctx, id).await?;
    Ok((StatusCode::CREATED, Json(set_out(ctx, set, 0, None).await?)))
}

async fn read_set(State(state): State<AppState>, P(id): P<i64>) -> Result<Json<PreferenceSetOut>, ApiError> {
    let ctx = &state.ctx;
    let set = get_set(ctx, id).await?;
    let decided = decided_count(ctx, id).await?;
    Ok(Json(set_out(ctx, set, decided, None).await?))
}

async fn delete_set(State(state): State<AppState>, P(id): P<i64>) -> Result<StatusCode, ApiError> {
    let ctx = &state.ctx;
    get_set(ctx, id).await?;
    let mut tx = ctx.db.begin().await?;
    sqlx::query("DELETE FROM preference_pairs WHERE preference_set_id = ?").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM preference_sets WHERE id = ?").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------------------------ items

#[derive(sqlx::FromRow)]
struct ItemRow {
    id: i64,
    idx: i64,
    messages: Db<Value>,
    chosen_sample_id: Option<i64>,
    rejected_sample_id: Option<i64>,
    source: Option<String>,
    #[sqlx(default)]
    total: i64,
}

#[derive(sqlx::FromRow)]
struct SampleRow {
    id: i64,
    run_id: i64,
    prompt_id: i64,
    sample_idx: i64,
    reasoning: Option<String>,
    response: Option<String>,
    finish_reason: Option<String>,
    completion_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
}

fn item_sql(run_ids: &[i64], extra_where: &str, window_total: bool) -> String {
    format!(
        "SELECT p.id, p.idx, p.messages, pp.chosen_sample_id, pp.rejected_sample_id, pp.source{total} \
         FROM prompts p \
         JOIN ({eligible}) e ON e.prompt_id = p.id \
         LEFT JOIN preference_pairs pp ON pp.prompt_id = p.id AND pp.preference_set_id = ? \
         {extra_where}",
        total = if window_total { ", COUNT(*) OVER () AS total" } else { "" },
        eligible = eligible_sql(run_ids),
    )
}

/// Attach the candidates to a page of prompts with one query for all their samples.
async fn build_items(ctx: &Ctx, runs: &[RunRow], rows: Vec<ItemRow>) -> Result<Vec<PreferenceItem>, ApiError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();
    let run_pos: HashMap<i64, usize> = runs.iter().enumerate().map(|(n, r)| (r.id, n)).collect();
    let prompt_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let samples = sqlx::query_as::<_, SampleRow>(&format!(
        "SELECT id, run_id, prompt_id, sample_idx, reasoning, response, finish_reason, completion_tokens, \
         reasoning_tokens FROM samples WHERE prompt_id IN ({}) AND run_id IN ({}) AND status = 'done'",
        id_list(&prompt_ids),
        id_list(&run_ids)
    ))
    .fetch_all(&ctx.db)
    .await?;
    let mut by_prompt: HashMap<i64, Vec<SampleRow>> = HashMap::new();
    for s in samples {
        by_prompt.entry(s.prompt_id).or_default().push(s);
    }
    Ok(rows
        .into_iter()
        .map(|row| {
            let mut cands = by_prompt.remove(&row.id).unwrap_or_default();
            cands.sort_by_key(|s| (run_pos.get(&s.run_id).copied().unwrap_or(usize::MAX), s.sample_idx));
            let candidates = cands
                .into_iter()
                .filter_map(|s| {
                    let run = &runs[*run_pos.get(&s.run_id)?];
                    Some(Candidate {
                        sample_id: s.id,
                        run_id: s.run_id,
                        run_name: run.name.clone(),
                        model: run.model.clone(),
                        sample_idx: s.sample_idx,
                        reasoning: s.reasoning,
                        response: s.response,
                        finish_reason: s.finish_reason,
                        completion_tokens: s.completion_tokens,
                        reasoning_tokens: s.reasoning_tokens,
                    })
                })
                .collect();
            let messages: Vec<Message> = match row.messages.0 {
                Value::Array(a) => a,
                _ => Vec::new(),
            };
            PreferenceItem {
                prompt_id: row.id,
                prompt_idx: row.idx,
                messages,
                candidates,
                chosen_sample_id: row.chosen_sample_id,
                rejected_sample_id: row.rejected_sample_id,
                source: row.source,
            }
        })
        .collect())
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum ItemFilter {
    All,
    Undecided,
    Decided,
}

#[derive(Deserialize)]
struct ItemsQuery {
    #[serde(default)]
    offset: i64,
    #[serde(default = "default_item_limit")]
    limit: i64,
    #[serde(default = "default_filter")]
    filter: ItemFilter,
}

fn default_item_limit() -> i64 {
    20
}

fn default_filter() -> ItemFilter {
    ItemFilter::All
}

async fn list_items(
    State(state): State<AppState>,
    P(id): P<i64>,
    ApiQuery(q): ApiQuery<ItemsQuery>,
) -> Result<Json<Page<PreferenceItem>>, ApiError> {
    let mut errors = FieldErrors::new();
    check_range(&mut errors, "offset", q.offset, 0, None);
    check_range(&mut errors, "limit", q.limit, 1, Some(200));
    errors.finish()?;
    let ctx = &state.ctx;
    let set = get_set(ctx, id).await?;
    let runs = runs_of(ctx, &set).await?;
    if runs.is_empty() {
        return Ok(Json(Page { items: Vec::new(), total: 0, offset: q.offset, limit: q.limit }));
    }
    let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();
    let filter = match q.filter {
        ItemFilter::All => "",
        ItemFilter::Undecided => "WHERE pp.id IS NULL",
        ItemFilter::Decided => "WHERE pp.id IS NOT NULL",
    };
    // The total rides along as a window count, so the eligible-prompt grouping runs once.
    let sql = format!("{} ORDER BY p.idx, p.id LIMIT ? OFFSET ?", item_sql(&run_ids, filter, true));
    let rows = sqlx::query_as::<_, ItemRow>(&sql).bind(id).bind(q.limit).bind(q.offset).fetch_all(&ctx.db).await?;
    let total = match rows.first() {
        Some(r) => r.total,
        None => {
            // Past the end: count on its own.
            let sql = format!("SELECT COUNT(*) FROM ({})", item_sql(&run_ids, filter, false));
            sqlx::query_scalar(&sql).bind(id).fetch_one(&ctx.db).await?
        }
    };
    let items = build_items(ctx, &runs, rows).await?;
    Ok(Json(Page { items, total, offset: q.offset, limit: q.limit }))
}

async fn one_item(ctx: &Ctx, set_id: i64, runs: &[RunRow], prompt_id: i64) -> Result<PreferenceItem, ApiError> {
    let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();
    let mut items = if run_ids.is_empty() {
        Vec::new()
    } else {
        let sql = item_sql(&run_ids, "WHERE p.id = ?", false);
        let rows = sqlx::query_as::<_, ItemRow>(&sql).bind(set_id).bind(prompt_id).fetch_all(&ctx.db).await?;
        build_items(ctx, runs, rows).await?
    };
    items
        .pop()
        .ok_or_else(|| ApiError::not_found("This prompt has fewer than two finished responses in the set's runs."))
}

// ------------------------------------------------------------------------------------ pairs

async fn put_pair(
    State(state): State<AppState>,
    P((id, prompt_id)): P<(i64, i64)>,
    ApiJson(body): ApiJson<PairBody>,
) -> Result<Json<PreferenceItem>, ApiError> {
    let ctx = &state.ctx;
    let set = get_set(ctx, id).await?;
    let in_set: Option<i64> = sqlx::query_scalar("SELECT prompt_set_id FROM prompts WHERE id = ?")
        .bind(prompt_id)
        .fetch_optional(&ctx.db)
        .await?;
    if in_set != Some(set.prompt_set_id) {
        return Err(ApiError::not_found("Prompt not found in this preference set's prompt set."));
    }
    if body.chosen_sample_id == body.rejected_sample_id {
        return Err(unprocessable("Chosen and rejected must be two different responses."));
    }
    let runs = runs_of(ctx, &set).await?;
    let run_ids: BTreeSet<i64> = runs.iter().map(|r| r.id).collect();
    let samples: HashMap<i64, (i64, i64, String)> = sqlx::query_as::<_, (i64, i64, i64, String)>(
        "SELECT id, run_id, prompt_id, status FROM samples WHERE id IN (?, ?)",
    )
    .bind(body.chosen_sample_id)
    .bind(body.rejected_sample_id)
    .fetch_all(&ctx.db)
    .await?
    .into_iter()
    .map(|(id, run, prompt, status)| (id, (run, prompt, status)))
    .collect();
    for (label, sid) in [("chosen", body.chosen_sample_id), ("rejected", body.rejected_sample_id)] {
        let Some((run, prompt, status)) = samples.get(&sid) else {
            return Err(unprocessable(format!(
                "The {label} response does not exist. Reload the prompt and pick again."
            )));
        };
        if !run_ids.contains(run) {
            return Err(unprocessable(format!(
                "The {label} response comes from a run that is not in this preference set."
            )));
        }
        if *prompt != prompt_id {
            return Err(unprocessable(format!(
                "The {label} response answers a different prompt. Reload and pick again."
            )));
        }
        if status != "done" {
            return Err(unprocessable(format!("The {label} response has not finished, so it cannot be paired.")));
        }
    }
    let now = now_db();
    sqlx::query(
        "INSERT INTO preference_pairs (preference_set_id, prompt_id, chosen_sample_id, rejected_sample_id, source, \
         created_at, updated_at) VALUES (?, ?, ?, ?, 'manual', ?, ?) \
         ON CONFLICT(preference_set_id, prompt_id) DO UPDATE SET chosen_sample_id = excluded.chosen_sample_id, \
         rejected_sample_id = excluded.rejected_sample_id, source = 'manual', updated_at = excluded.updated_at",
    )
    .bind(id)
    .bind(prompt_id)
    .bind(body.chosen_sample_id)
    .bind(body.rejected_sample_id)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await?;
    Ok(Json(one_item(ctx, id, &runs, prompt_id).await?))
}

async fn delete_pair(
    State(state): State<AppState>,
    P((id, prompt_id)): P<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    let ctx = &state.ctx;
    get_set(ctx, id).await?;
    sqlx::query("DELETE FROM preference_pairs WHERE preference_set_id = ? AND prompt_id = ?")
        .bind(id)
        .bind(prompt_id)
        .execute(&ctx.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Per prompt, the run's first finished sample (lowest sample_idx), as a CTE body.
const FIRST_DONE: &str = "SELECT prompt_id, id FROM (SELECT prompt_id, id, ROW_NUMBER() OVER \
    (PARTITION BY prompt_id ORDER BY sample_idx) AS rn FROM samples WHERE run_id = ? AND status = 'done') WHERE rn = 1";

async fn auto_pair(
    State(state): State<AppState>,
    P(id): P<i64>,
    ApiJson(body): ApiJson<AutoPairBody>,
) -> Result<Json<AutoPairOut>, ApiError> {
    let ctx = &state.ctx;
    let set = get_set(ctx, id).await?;
    let run_ids: BTreeSet<i64> = runs_of(ctx, &set).await?.iter().map(|r| r.id).collect();
    if body.chosen_run_id == body.rejected_run_id {
        return Err(unprocessable("Pick two different runs: one for chosen and one for rejected."));
    }
    for (label, rid) in [("chosen", body.chosen_run_id), ("rejected", body.rejected_run_id)] {
        if !run_ids.contains(&rid) {
            return Err(unprocessable(format!("The {label} run is not in this preference set.")));
        }
    }
    // One transaction, all in SQL: nothing is loaded into memory however many prompts there are.
    let mut tx = ctx.db.begin().await?;
    let skipped: i64 = if body.overwrite {
        0
    } else {
        sqlx::query_scalar(&format!(
            "WITH c AS ({FIRST_DONE}), r AS ({FIRST_DONE}) SELECT COUNT(*) FROM c JOIN r ON r.prompt_id = c.prompt_id \
             JOIN preference_pairs ex ON ex.prompt_id = c.prompt_id AND ex.preference_set_id = ? AND ex.source = 'manual'"
        ))
        .bind(body.chosen_run_id)
        .bind(body.rejected_run_id)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?
    };
    let now = now_db();
    let created = sqlx::query(&format!(
        "WITH c AS ({FIRST_DONE}), r AS ({FIRST_DONE}) \
         INSERT INTO preference_pairs (preference_set_id, prompt_id, chosen_sample_id, rejected_sample_id, source, \
         created_at, updated_at) \
         SELECT ?, c.prompt_id, c.id, r.id, 'rule', ?, ? FROM c JOIN r ON r.prompt_id = c.prompt_id \
         LEFT JOIN preference_pairs ex ON ex.prompt_id = c.prompt_id AND ex.preference_set_id = ? \
         WHERE ? OR ex.source IS NOT 'manual' \
         ON CONFLICT(preference_set_id, prompt_id) DO UPDATE SET chosen_sample_id = excluded.chosen_sample_id, \
         rejected_sample_id = excluded.rejected_sample_id, source = excluded.source, updated_at = excluded.updated_at"
    ))
    .bind(body.chosen_run_id)
    .bind(body.rejected_run_id)
    .bind(id)
    .bind(&now)
    .bind(&now)
    .bind(id)
    .bind(body.overwrite)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;
    tx.commit().await?;
    Ok(Json(AutoPairOut { created, skipped }))
}
