//! Dataset exports: SFT formats for a run's samples and DPO/KTO formats for a preference set,
//! written as JSONL, JSON, CSV or Parquet, downloaded or pushed to the Hugging Face Hub.
//!
//! A port of `backend/stellarator/api/exports.py` and `services/exporters.py`. Rows are read in
//! keyset-paged batches and written as they arrive, so memory stays flat for any export size.

pub mod files;
pub mod hub;
pub mod pyjson;
pub mod records;
pub mod schema;
pub mod source;

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;
use sqlx::types::Json as SqlJson;
use sqlx::{FromRow, SqlitePool};
use stellarator_core::schemas::{
    ExportOptions, ExportPreviewOut, FileType, HubPushOut, PrefExportOptions, PrefHubIn, RunHubIn,
};
use stellarator_core::{settings, ApiError, ApiJson, ApiQuery, Ctx};

use crate::files::TempFile;
use crate::records::{filename_for, pref_name, sft_name, tools_refusal, RunInfo, PREF_TOOLS_REFUSAL};
use crate::source::Source;

pub fn router(ctx: Ctx) -> Router {
    Router::new()
        .route("/api/runs/{id}/export/preview", post(preview_run))
        .route("/api/runs/{id}/export", get(download_run))
        .route("/api/runs/{id}/export/hub", post(push_run))
        .route("/api/preference-sets/{id}/export/preview", post(preview_pref))
        .route("/api/preference-sets/{id}/export", get(download_pref))
        .route("/api/preference-sets/{id}/export/hub", post(push_pref))
        .with_state(ctx)
}

// ------------------------------------------------------------------------------ lookups

#[derive(FromRow)]
struct RunRow {
    id: i64,
    name: String,
    endpoint_name: String,
    model: String,
    prompt_set_id: i64,
    params: SqlJson<Value>,
    tools: Option<SqlJson<Value>>,
}

impl From<RunRow> for RunInfo {
    fn from(r: RunRow) -> Self {
        RunInfo {
            id: r.id,
            name: r.name,
            endpoint_name: r.endpoint_name,
            model: r.model,
            prompt_set_id: r.prompt_set_id,
            params: r.params.0,
            tools: r.tools.and_then(|t| serde_json::from_value(t.0).ok()),
        }
    }
}

const RUN_COLS: &str = "id, name, endpoint_name, model, prompt_set_id, params, tools";

/// 422 when the format cannot hold the run's tool calls.
fn check_tools(run: &RunInfo, options: &ExportOptions) -> Result<(), ApiError> {
    match (&run.tools, tools_refusal(options.format)) {
        (Some(_), Some(why)) => Err(ApiError::field("format", why)),
        _ => Ok(()),
    }
}

/// 422 for text preference records of a set with tool runs; else whether the set has tools.
async fn check_pref_tools(db: &SqlitePool, set_id: i64, options: &PrefExportOptions) -> Result<bool, ApiError> {
    let tools = source::pref_has_tools(db, set_id).await?;
    if tools && !options.conversational {
        return Err(ApiError::field("conversational", PREF_TOOLS_REFUSAL));
    }
    Ok(tools)
}

async fn load_run(db: &SqlitePool, id: i64) -> Result<RunInfo, ApiError> {
    let row: Option<RunRow> =
        sqlx::query_as(&format!("SELECT {RUN_COLS} FROM runs WHERE id = ?")).bind(id).fetch_optional(db).await?;
    row.map(RunInfo::from).ok_or_else(|| ApiError::not_found("Run not found"))
}

struct PrefSet {
    id: i64,
    name: String,
    prompt_set_id: i64,
    run_ids: Vec<i64>,
}

async fn load_pset(db: &SqlitePool, id: i64) -> Result<PrefSet, ApiError> {
    let row: Option<(i64, String, i64, SqlJson<Value>)> =
        sqlx::query_as("SELECT id, name, prompt_set_id, run_ids FROM preference_sets WHERE id = ?")
            .bind(id)
            .fetch_optional(db)
            .await?;
    let (id, name, prompt_set_id, run_ids) = row.ok_or_else(|| ApiError::not_found("Preference set not found"))?;
    let run_ids = run_ids.0.as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
    Ok(PrefSet { id, name, prompt_set_id, run_ids })
}

async fn prompt_set_name(db: &SqlitePool, id: i64) -> Result<String, ApiError> {
    let n: Option<String> = sqlx::query_scalar("SELECT name FROM prompt_sets WHERE id = ?").bind(id).fetch_optional(db).await?;
    Ok(n.unwrap_or_default())
}

/// The first record of an export, read with a batch size of one.
async fn first_record(db: &SqlitePool, source: Source) -> Result<Option<Value>, ApiError> {
    let mut rx = source::spawn(db.clone(), source, 1);
    match rx.recv().await {
        Some(Ok(mut batch)) if !batch.is_empty() => Ok(Some(batch.swap_remove(0))),
        Some(Err(e)) => Err(e.into()),
        _ => Ok(None),
    }
}

// ------------------------------------------------------------------------------ downloads

fn content_disposition(filename: &str) -> HeaderValue {
    HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

/// A streaming download of `rx` as `file_type`. Parquet is written to a temp file first.
async fn download(
    ctx: &Ctx,
    file_type: FileType,
    schema: Arc<arrow_schema::Schema>,
    rx: tokio::sync::mpsc::Receiver<source::Batch>,
    filename: String,
) -> Result<Response, ApiError> {
    let stream = match file_type {
        FileType::Jsonl => files::jsonl(rx),
        FileType::Json => files::json_array(rx),
        FileType::Csv => files::csv(rx, &schema),
        FileType::Parquet => {
            let (guard, file) = write_temp_parquet(ctx, schema, rx).await?;
            files::stream_file(file, guard)
        }
    };
    let mut resp = Response::new(Body::from_stream(stream));
    resp.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(files::media_type(file_type)));
    resp.headers_mut().insert(header::CONTENT_DISPOSITION, content_disposition(&filename));
    Ok(resp)
}

/// Write the Parquet file under the export directory. The returned guard deletes it.
async fn write_temp_parquet(
    ctx: &Ctx,
    schema: Arc<arrow_schema::Schema>,
    rx: tokio::sync::mpsc::Receiver<source::Batch>,
) -> Result<(TempFile, tokio::fs::File), ApiError> {
    let dir = ctx.config.export_dir();
    tokio::fs::create_dir_all(&dir).await.map_err(|e| export_failed(&e))?;
    let guard = TempFile(dir.join(format!("{}.parquet", uuid::Uuid::new_v4().simple())));
    files::write_parquet(&guard.0, schema, rx).await.map_err(|e| export_failed(&*e))?;
    let file = tokio::fs::File::open(&guard.0).await.map_err(|e| export_failed(&e))?;
    Ok((guard, file))
}

fn export_failed(e: &dyn std::fmt::Display) -> ApiError {
    tracing::error!(error = %e, "export failed");
    ApiError::internal("The export failed. Check the Stellarator logs, then retry.")
}

// ------------------------------------------------------------------------------ run routes

async fn preview_run(
    State(ctx): State<Ctx>,
    Path(id): Path<i64>,
    ApiJson(options): ApiJson<ExportOptions>,
) -> Result<Json<ExportPreviewOut>, ApiError> {
    let run = load_run(&ctx.db, id).await?;
    check_tools(&run, &options)?;
    let (count, skipped) = source::sft_counts(&ctx.db, run.id, &options).await?;
    let filename = filename_for(&run.name, sft_name(options.format), files::extension(options.file_type));
    let example = first_record(&ctx.db, Source::Sft { run: Arc::new(run), options }).await?;
    Ok(Json(ExportPreviewOut {
        count,
        skipped: skipped.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        example,
        filename,
    }))
}

async fn download_run(
    State(ctx): State<Ctx>,
    Path(id): Path<i64>,
    ApiQuery(options): ApiQuery<ExportOptions>,
) -> Result<Response, ApiError> {
    let run = load_run(&ctx.db, id).await?;
    check_tools(&run, &options)?;
    let filename = filename_for(&run.name, sft_name(options.format), files::extension(options.file_type));
    let schema = schema::sft_schema_for(&options, run.tools.is_some());
    let file_type = options.file_type;
    let rx = source::spawn(ctx.db.clone(), Source::Sft { run: Arc::new(run), options }, source::BATCH);
    download(&ctx, file_type, schema, rx, filename).await
}

async fn push_run(
    State(ctx): State<Ctx>,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<RunHubIn>,
) -> Result<Json<HubPushOut>, ApiError> {
    let run = load_run(&ctx.db, id).await?;
    let token = settings::hf_token(&ctx).await.ok_or_else(|| ApiError::bad_request(hub::HF_TOKEN_MISSING))?;
    let options = body.options;
    check_tools(&run, &options)?;
    let (count, _) = source::sft_counts(&ctx.db, run.id, &options).await?;
    let set_name = prompt_set_name(&ctx.db, run.prompt_set_id).await?;
    let card = hub::run_card(&run, &set_name, count, &options, &body.target.split);
    let schema = schema::sft_schema_for(&options, run.tools.is_some());
    let rx = source::spawn(ctx.db.clone(), Source::Sft { run: Arc::new(run), options }, source::BATCH);
    push_to_hub(&ctx, &token, &body.target, schema, rx, count, card).await.map(Json)
}

// ------------------------------------------------------------------------------ preference routes

async fn preview_pref(
    State(ctx): State<Ctx>,
    Path(id): Path<i64>,
    ApiJson(options): ApiJson<PrefExportOptions>,
) -> Result<Json<ExportPreviewOut>, ApiError> {
    let ps = load_pset(&ctx.db, id).await?;
    check_pref_tools(&ctx.db, ps.id, &options).await?;
    let count = source::pref_count(&ctx.db, ps.id, &options).await?;
    let filename = filename_for(&ps.name, pref_name(options.format), files::extension(options.file_type));
    let example = first_record(&ctx.db, Source::Pref { set_id: ps.id, options }).await?;
    Ok(Json(ExportPreviewOut { count, skipped: Default::default(), example, filename }))
}

async fn download_pref(
    State(ctx): State<Ctx>,
    Path(id): Path<i64>,
    ApiQuery(options): ApiQuery<PrefExportOptions>,
) -> Result<Response, ApiError> {
    let ps = load_pset(&ctx.db, id).await?;
    let tools = check_pref_tools(&ctx.db, ps.id, &options).await?;
    let filename = filename_for(&ps.name, pref_name(options.format), files::extension(options.file_type));
    let schema = schema::pref_schema_for(&options, tools);
    let file_type = options.file_type;
    let rx = source::spawn(ctx.db.clone(), Source::Pref { set_id: ps.id, options }, source::BATCH);
    download(&ctx, file_type, schema, rx, filename).await
}

async fn push_pref(
    State(ctx): State<Ctx>,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<PrefHubIn>,
) -> Result<Json<HubPushOut>, ApiError> {
    let ps = load_pset(&ctx.db, id).await?;
    let token = settings::hf_token(&ctx).await.ok_or_else(|| ApiError::bad_request(hub::HF_TOKEN_MISSING))?;
    let options = body.options;
    let tools = check_pref_tools(&ctx.db, ps.id, &options).await?;
    let count = source::pref_count(&ctx.db, ps.id, &options).await?;
    let set_name = prompt_set_name(&ctx.db, ps.prompt_set_id).await?;
    let mut runs = Vec::new();
    let mut ids = ps.run_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    for rid in ids {
        let row: Option<RunRow> = sqlx::query_as(&format!("SELECT {RUN_COLS} FROM runs WHERE id = ?"))
            .bind(rid)
            .fetch_optional(&ctx.db)
            .await?;
        runs.extend(row.map(RunInfo::from));
    }
    let card = hub::pref_card(&ps.name, &set_name, &runs, count, &options, &body.target.split);
    let schema = schema::pref_schema_for(&options, tools);
    let rx = source::spawn(ctx.db.clone(), Source::Pref { set_id: ps.id, options }, source::BATCH);
    push_to_hub(&ctx, &token, &body.target, schema, rx, count, card).await.map(Json)
}

// ------------------------------------------------------------------------------ hub

async fn push_to_hub(
    ctx: &Ctx,
    token: &str,
    target: &stellarator_core::schemas::HubPushIn,
    schema: Arc<arrow_schema::Schema>,
    rx: tokio::sync::mpsc::Receiver<source::Batch>,
    count: i64,
    card: String,
) -> Result<HubPushOut, ApiError> {
    if !hub::valid_split(&target.split) {
        return Err(ApiError::bad_request(
            "The split name can only use letters, numbers, dots, dashes and underscores.",
        ));
    }
    if !hub::valid_repo_id(&target.repo_id) {
        return Err(ApiError::bad_request(
            "The repo id must look like owner/name, using letters, numbers, dashes and dots.",
        ));
    }
    if count == 0 {
        return Err(ApiError::bad_request("There are no records to push with these options."));
    }
    let (guard, _file) = write_temp_parquet(ctx, schema, rx).await?;
    let url = hub::push(ctx, token, &target.repo_id, target.private, &target.split, &guard.0, &card).await?;
    drop(guard);
    Ok(HubPushOut { url, count })
}
