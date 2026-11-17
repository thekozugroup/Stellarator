//! `/api/imports/upload`, `/api/imports/preview` and `/api/imports`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::multipart::MultipartRejection;
use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use stellarator_core::schemas::{ImportCommitIn, ImportMapping, ImportPreviewIn, ImportPreviewOut, ImportSource, PromptSetOut, UploadOut};
use stellarator_core::error::{parse_json, FieldErrors, Validate};
use stellarator_core::ApiError;
use tokio::io::AsyncWriteExt;

use crate::cancel::Cancel;
use crate::files::{detect_format, open_path, Opened, TextDoc};
use crate::hf::HfParquetTable;
use crate::jobs::Imports;
use crate::problem::{Problem, Result};
use crate::prompts::{preview_table, preview_text, resolve_plan, PreviewOptions};
use crate::schema::infer_schema;
use crate::table::{read_rows, MemoryTable, Row, Table, TableSource};
use crate::upload::{cleanup_old_uploads, sanitize_filename, upload_file, TOO_BIG, UPLOAD_MAX_AGE};

/// Rows read from the Parquet export for a preview that starts past the first rows.
const WINDOW_PREVIEW_ROWS: u64 = 100;

pub fn router(imports: Arc<Imports>) -> Router {
    let body_limit = (imports.max_upload + (1 << 20)) as usize;
    Router::new()
        .route("/api/imports/upload", post(upload).layer(DefaultBodyLimit::max(body_limit)))
        .route("/api/imports/preview", post(preview))
        .route("/api/imports", post(commit))
        .with_state(imports)
}

/// Like `ApiJson`, but a missing "mapping" gets serde's defaults: `ImportMapping::default()` (a
/// derived Default) would turn dedupe off, while the API's default is on.
pub struct ImportJson<T>(pub T);

impl<S, T> axum::extract::FromRequest<S> for ImportJson<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned + Validate,
{
    type Rejection = ApiError;

    async fn from_request(req: axum::extract::Request, state: &S) -> std::result::Result<Self, ApiError> {
        let bytes = <axum::body::Bytes as axum::extract::FromRequest<S>>::from_request(req, state)
            .await
            .map_err(|_| ApiError::bad_request("Couldn't read the request body."))?;
        let bytes = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(serde_json::Value::Object(mut o)) => {
                if matches!(o.get("mapping"), None | Some(serde_json::Value::Null)) {
                    o.insert("mapping".into(), serde_json::json!({}));
                }
                serde_json::to_vec(&o).unwrap_or_else(|_| bytes.to_vec())
            }
            _ => bytes.to_vec(),
        };
        let mut value: T = parse_json(&bytes)?;
        let mut errors = FieldErrors::new();
        value.validate(&mut errors);
        errors.finish()?;
        Ok(ImportJson(value))
    }
}

fn too_big() -> ApiError {
    ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, TOO_BIG)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    match tokio::task::spawn_blocking(f).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "import task failed");
            Err(Problem::status(500, "Stellarator hit an error reading this source. Check its logs, then retry."))
        }
    }
}

async fn upload(
    State(im): State<Arc<Imports>>,
    headers: HeaderMap,
    multipart: std::result::Result<Multipart, MultipartRejection>,
) -> std::result::Result<Json<UploadOut>, ApiError> {
    let max = im.max_upload;
    let declared = headers.get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    if declared.is_some_and(|n| n > max + (1 << 20)) {
        return Err(too_big());
    }
    let mut multipart = multipart.map_err(|_| ApiError::field("file", "Upload a file as multipart/form-data."))?;
    let root = im.ctx.config.upload_dir();
    let r = root.clone();
    let _ = tokio::task::spawn_blocking(move || cleanup_old_uploads(&r, UPLOAD_MAX_AGE)).await;
    let multipart_error = |e: axum::extract::multipart::MultipartError| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            too_big()
        } else {
            ApiError::bad_request("The upload was cut off or malformed. Try again.")
        }
    };
    let mut field = loop {
        match multipart.next_field().await.map_err(multipart_error)? {
            Some(f) if f.name() == Some("file") => break f,
            Some(_) => continue,
            None => return Err(ApiError::field("file", "Field required.")),
        }
    };
    let upload_id = uuid::Uuid::new_v4().simple().to_string();
    let folder = root.join(&upload_id);
    let name = sanitize_filename(field.file_name());
    let dest = folder.join(&name);
    let result: std::result::Result<UploadOut, ApiError> = async {
        tokio::fs::create_dir_all(&folder).await.map_err(|e| ApiError::internal(format!("Couldn't save the upload: {e}.")))?;
        let mut out = tokio::fs::File::create(&dest).await.map_err(|e| ApiError::internal(format!("Couldn't save the upload: {e}.")))?;
        let mut size: u64 = 0;
        while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
            size += chunk.len() as u64;
            if size > max {
                return Err(too_big());
            }
            out.write_all(&chunk).await.map_err(|e| ApiError::internal(format!("Couldn't save the upload: {e}.")))?;
        }
        out.flush().await.map_err(|e| ApiError::internal(format!("Couldn't save the upload: {e}.")))?;
        drop(out);
        if size == 0 {
            return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "That file is empty."));
        }
        let (d, n) = (dest.clone(), name.clone());
        let fmt = tokio::task::spawn_blocking(move || detect_format(&d, Some(&n))).await.unwrap_or("text");
        Ok(UploadOut { upload_id: upload_id.clone(), filename: name.clone(), size: size as i64, detected_format: fmt.to_string() })
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_dir_all(&folder).await;
    }
    result.map(Json)
}

async fn preview(
    State(im): State<Arc<Imports>>,
    ImportJson(body): ImportJson<ImportPreviewIn>,
) -> std::result::Result<Json<ImportPreviewOut>, ApiError> {
    Ok(Json(im.preview(body.source, body.mapping).await?))
}

async fn commit(
    State(im): State<Arc<Imports>>,
    ImportJson(body): ImportJson<ImportCommitIn>,
) -> std::result::Result<(StatusCode, Json<PromptSetOut>), ApiError> {
    let set = im.start(&body.source, &body.mapping, &body.name).await?;
    Ok((StatusCode::ACCEPTED, Json(set)))
}

/// Rows aligned with `from` re-aligned with `to` (by column name).
fn realign(rows: Vec<Row>, from: &[String], to: &[String]) -> Vec<Row> {
    let pos: HashMap<&str, usize> = from.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let map: Vec<Option<usize>> = to.iter().map(|n| pos.get(n.as_str()).copied()).collect();
    rows.into_iter()
        .map(|mut r| {
            map.iter()
                .map(|m| m.and_then(|i| r.get_mut(i).map(std::mem::take)).unwrap_or(serde_json::Value::Null))
                .collect()
        })
        .collect()
}

impl Imports {
    pub async fn preview(&self, source: ImportSource, mapping: ImportMapping) -> Result<ImportPreviewOut> {
        match source {
            ImportSource::Paste { text } => {
                if text.trim().is_empty() {
                    return Err(Problem::new("Paste some text to import.").on("source.text"));
                }
                blocking(move || Ok(preview_text(&TextDoc { text, kind: "paste", warnings: Vec::new() }, &mapping))).await
            }
            ImportSource::File { upload_id, format } => {
                let path = upload_file(&self.ctx.config.upload_dir(), &upload_id)?;
                blocking(move || {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                    match open_path(&path, format, name.as_deref())? {
                        Opened::Text(doc) => Ok(preview_text(&doc, &mapping)),
                        Opened::Table(mut src) => preview_table(&mut src, &mapping, PreviewOptions::default()),
                    }
                })
                .await
            }
            ImportSource::Huggingface { dataset, config, split, all_splits } => {
                self.hf_preview(&dataset, config.as_deref(), split.as_deref(), all_splits, mapping).await
            }
        }
    }

    async fn hf_preview(
        &self,
        dataset: &str,
        config: Option<&str>,
        split: Option<&str>,
        all_splits: bool,
        mapping: ImportMapping,
    ) -> Result<ImportPreviewOut> {
        let hf = self.hf().await;
        let ds = hf.dataset(dataset, config, split, all_splits).await?;
        let first = ds.first_rows;
        let names = first.names.clone();
        let sample = first.rows.clone();
        let schema = infer_schema(&names, &sample, false, &first.hints);
        let mut out = if mapping.offset > 0 {
            // The first rows only cover the start of the split: read the requested window from
            // the Parquet export, keeping the columns described from the first rows.
            let (plan, _, _) = resolve_plan(&schema, &sample, &mapping)?;
            let rows = match plan {
                Some(plan) => {
                    let (shards, partial) = hf.parquet_files(&ds.dataset, &ds.config, &ds.splits).await?;
                    let split_rows = if partial {
                        HashMap::new()
                    } else {
                        ds.hf_splits
                            .iter()
                            .filter(|s| s.config == ds.config)
                            .filter_map(|s| s.num_rows.map(|n| (s.split.clone(), n.max(0) as u64)))
                            .collect()
                    };
                    let mut table = HfParquetTable::open(&hf, shards, ds.num_rows, split_rows, Cancel::new()).await?;
                    let bound = plan.bind(table.names())?;
                    let offset = mapping.offset as u64;
                    let from = table.names().to_vec();
                    let rows = blocking(move || {
                        let cols = bound.columns();
                        read_rows(&mut table, Some(&cols), offset, Some(WINDOW_PREVIEW_ROWS))
                    })
                    .await?;
                    realign(rows, &from, &names)
                }
                None => Vec::new(),
            };
            let mut table = MemoryTable::new(names, rows, "huggingface", None);
            table.total_rows = ds.num_rows;
            table.warnings = first.warnings.clone();
            table.hints = first.hints.clone();
            let mut src = TableSource::with_schema(Box::new(table), sample, schema);
            let m = mapping.clone();
            blocking(move || preview_table(&mut src, &m, PreviewOptions { full_pass: Some(false), apply_window: false })).await?
        } else {
            let mut src = TableSource::with_schema(Box::new(first), sample, schema);
            let m = mapping.clone();
            blocking(move || preview_table(&mut src, &m, PreviewOptions { full_pass: Some(false), apply_window: true })).await?
        };
        out.hf_splits = ds.hf_splits;
        out.hf_config = Some(ds.config);
        out.hf_split = Some(ds.shown_split);
        Ok(out)
    }
}
