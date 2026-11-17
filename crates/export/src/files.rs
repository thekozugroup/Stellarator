//! Turning record batches into bytes: JSONL, JSON array and CSV stream as batches arrive; Parquet
//! is written to a temporary file first (the format needs a footer), then streamed and deleted.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_json::ReaderBuilder;
use arrow_schema::{DataType, Schema};
use bytes::Bytes;
use futures::stream::{self, BoxStream, StreamExt};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde_json::Value;
use stellarator_core::schemas::FileType;
use tokio::sync::mpsc;
use tokio_util::io::ReaderStream;

use crate::pyjson;
use crate::source::Batch;

pub fn media_type(t: FileType) -> &'static str {
    match t {
        FileType::Jsonl => "application/x-ndjson",
        FileType::Json => "application/json",
        FileType::Parquet => "application/vnd.apache.parquet",
        // Starlette adds the charset to text/* types; keep the header identical.
        FileType::Csv => "text/csv; charset=utf-8",
    }
}

pub fn extension(t: FileType) -> &'static str {
    match t {
        FileType::Jsonl => "jsonl",
        FileType::Json => "json",
        FileType::Parquet => "parquet",
        FileType::Csv => "csv",
    }
}

pub type ByteStream = BoxStream<'static, io::Result<Bytes>>;

fn io_err(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

enum Stage {
    Prefix,
    Body,
    Suffix,
    Done,
}

struct Feed<F> {
    rx: mpsc::Receiver<Batch>,
    f: F,
    stage: Stage,
    prefix: Option<Vec<u8>>,
    suffix: Option<Vec<u8>>,
}

/// Prefix bytes, then `f(batch)` for each batch, then suffix bytes. An error ends the stream.
fn feed<F>(rx: mpsc::Receiver<Batch>, prefix: Option<Vec<u8>>, suffix: Option<Vec<u8>>, f: F) -> ByteStream
where
    F: FnMut(&[Value]) -> Vec<u8> + Send + 'static,
{
    stream::unfold(Feed { rx, f, stage: Stage::Prefix, prefix, suffix }, |mut st| async move {
        loop {
            match st.stage {
                Stage::Prefix => {
                    st.stage = Stage::Body;
                    if let Some(p) = st.prefix.take() {
                        return Some((Ok(Bytes::from(p)), st));
                    }
                }
                Stage::Body => match st.rx.recv().await {
                    Some(Ok(batch)) => {
                        let out = (st.f)(&batch);
                        if !out.is_empty() {
                            return Some((Ok(Bytes::from(out)), st));
                        }
                    }
                    Some(Err(e)) => {
                        st.stage = Stage::Done;
                        return Some((Err(io_err(e)), st));
                    }
                    None => st.stage = Stage::Suffix,
                },
                Stage::Suffix => {
                    st.stage = Stage::Done;
                    if let Some(s) = st.suffix.take() {
                        return Some((Ok(Bytes::from(s)), st));
                    }
                }
                Stage::Done => return None,
            }
        }
    })
    .boxed()
}

pub fn jsonl(rx: mpsc::Receiver<Batch>) -> ByteStream {
    feed(rx, None, None, |recs| {
        let mut out = Vec::with_capacity(recs.len() * 512);
        for r in recs {
            pyjson::write(&mut out, r);
            out.push(b'\n');
        }
        out
    })
}

/// `[`, then each record on its own line separated by commas, then `\n]\n` (Python's layout).
pub fn json_array(rx: mpsc::Receiver<Batch>) -> ByteStream {
    let mut first = true;
    feed(rx, Some(b"[".to_vec()), Some(b"\n]\n".to_vec()), move |recs| {
        let mut out = Vec::with_capacity(recs.len() * 512);
        for r in recs {
            out.extend_from_slice(if first { b"\n" } else { b",\n" });
            first = false;
            pyjson::write(&mut out, r);
        }
        out
    })
}

fn csv_cell(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::Bool(b)) => if *b { "true" } else { "false" }.into(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => pyjson::to_string(other),
    }
}

/// Python's csv.writer with QUOTE_MINIMAL and "\n" line endings.
fn csv_push(out: &mut Vec<u8>, field: &str, single: bool) {
    let needs = field.contains([',', '"', '\n', '\r']) || (single && field.is_empty());
    if needs {
        out.push(b'"');
        for b in field.bytes() {
            if b == b'"' {
                out.push(b'"');
            }
            out.push(b);
        }
        out.push(b'"');
    } else {
        out.extend_from_slice(field.as_bytes());
    }
}

fn csv_line(out: &mut Vec<u8>, fields: &[String]) {
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        csv_push(out, f, fields.len() == 1);
    }
    out.push(b'\n');
}

pub fn csv(rx: mpsc::Receiver<Batch>, schema: &Schema) -> ByteStream {
    let names: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    let mut header = Vec::new();
    csv_line(&mut header, &names);
    feed(rx, Some(header), None, move |recs| {
        let mut out = Vec::with_capacity(recs.len() * 512);
        for r in recs {
            let row: Vec<String> = names.iter().map(|n| csv_cell(r.get(n))).collect();
            csv_line(&mut out, &row);
        }
        out
    })
}

// ------------------------------------------------------------------------------ Parquet

/// A file deleted when dropped.
pub struct TempFile(pub PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Stream a finished file in 1 MiB chunks; the file goes away when the stream does.
pub fn stream_file(file: tokio::fs::File, guard: TempFile) -> ByteStream {
    ReaderStream::with_capacity(file, 1 << 20)
        .map(move |chunk| {
            let _keep = &guard;
            chunk
        })
        .boxed()
}

/// JSON-encode objects and arrays bound for string columns (raw's params and meta).
fn coerce(rec: &mut Value, schema: &Schema) {
    let Some(obj) = rec.as_object_mut() else { return };
    for field in schema.fields() {
        if *field.data_type() == DataType::Utf8 {
            if let Some(v) = obj.get_mut(field.name()) {
                if v.is_object() || v.is_array() {
                    *v = Value::String(pyjson::to_string(v));
                }
            }
        }
    }
}

/// Write every batch to a Parquet file (zstd, row groups of at most 8192 rows) on a blocking
/// thread. Returns the number of rows written.
pub async fn write_parquet(path: &Path, schema: Arc<Schema>, rx: mpsc::Receiver<Batch>) -> anyhow::Result<u64> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> anyhow::Result<u64> {
        let mut rx = rx;
        let file = std::fs::File::create(&path)?;
        let props = WriterProperties::builder()
            .set_compression(Compression::ZSTD(ZstdLevel::try_new(3)?))
            .set_max_row_group_size(8192)
            .build();
        let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;
        let mut decoder = ReaderBuilder::new(schema.clone()).with_batch_size(1024).build_decoder()?;
        let mut rows = 0u64;
        while let Some(batch) = rx.blocking_recv() {
            let mut recs = batch?;
            for r in &mut recs {
                coerce(r, &schema);
            }
            rows += recs.len() as u64;
            decoder.serialize(&recs)?;
            while let Some(rb) = decoder.flush()? {
                writer.write(&rb)?;
            }
        }
        writer.close()?;
        Ok(rows)
    })
    .await?
}
