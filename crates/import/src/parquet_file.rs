//! Parquet: a local file read by row group and batch with column projection, and the Arrow to JSON
//! conversion shared with the Hugging Face reader.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use arrow_array::cast::AsArray;
use arrow_array::types::*;
use arrow_array::{Array, RecordBatch};
use arrow_cast::display::{ArrayFormatter, FormatOptions};
use arrow_schema::{DataType, Schema as ArrowSchema};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ProjectionMask;
use parquet::file::metadata::ParquetMetaData;
use serde_json::{Map, Number, Value};

use crate::files::FULL_PASS_MAX_BYTES;
use crate::problem::{Problem, Result};
use crate::schema::Hint;
use crate::table::{Flow, Row, RowSink, Table, Window};

pub const BATCH_ROWS: usize = 1000;

/// Columns with binary data anywhere inside (images, audio) can't hold prompts and are left out.
pub fn has_binary(t: &DataType) -> bool {
    match t {
        DataType::Binary | DataType::LargeBinary | DataType::FixedSizeBinary(_) | DataType::BinaryView => true,
        DataType::Struct(fields) => fields.iter().any(|f| has_binary(f.data_type())),
        DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) | DataType::ListView(f) | DataType::LargeListView(f) => {
            has_binary(f.data_type())
        }
        DataType::Map(f, _) => has_binary(f.data_type()),
        DataType::Dictionary(_, v) => has_binary(v),
        _ => false,
    }
}

fn is_temporal(t: &DataType) -> bool {
    matches!(
        t,
        DataType::Date32 | DataType::Date64 | DataType::Time32(_) | DataType::Time64(_) | DataType::Timestamp(_, _)
            | DataType::Duration(_) | DataType::Interval(_)
    ) || matches!(t, DataType::Dictionary(_, v) if is_temporal(v))
}

/// The columns a table offers: (names, root column index of each, hints).
pub fn usable_columns(schema: &ArrowSchema) -> (Vec<String>, Vec<usize>, Vec<Option<Hint>>) {
    let mut names = Vec::new();
    let mut roots = Vec::new();
    let mut hints = Vec::new();
    for (i, f) in schema.fields().iter().enumerate() {
        if has_binary(f.data_type()) {
            continue;
        }
        names.push(f.name().clone());
        roots.push(i);
        hints.push(is_temporal(f.data_type()).then_some(Hint::Temporal));
    }
    (names, roots, hints)
}

fn num(f: f64) -> Value {
    Number::from_f64(f).map_or(Value::Null, Value::Number)
}

fn formatted(a: &dyn Array, i: usize) -> Value {
    match ArrayFormatter::try_new(a, &FormatOptions::default()) {
        Ok(f) => Value::String(f.value(i).to_string()),
        Err(_) => Value::Null,
    }
}

/// One Arrow cell as JSON, the way pyarrow's `to_pylist` would hand it to the Python importer.
pub fn cell(a: &dyn Array, i: usize) -> Value {
    if a.is_null(i) {
        return Value::Null;
    }
    match a.data_type() {
        DataType::Null => Value::Null,
        DataType::Boolean => Value::Bool(a.as_boolean().value(i)),
        DataType::Int8 => Value::from(a.as_primitive::<Int8Type>().value(i)),
        DataType::Int16 => Value::from(a.as_primitive::<Int16Type>().value(i)),
        DataType::Int32 => Value::from(a.as_primitive::<Int32Type>().value(i)),
        DataType::Int64 => Value::from(a.as_primitive::<Int64Type>().value(i)),
        DataType::UInt8 => Value::from(a.as_primitive::<UInt8Type>().value(i)),
        DataType::UInt16 => Value::from(a.as_primitive::<UInt16Type>().value(i)),
        DataType::UInt32 => Value::from(a.as_primitive::<UInt32Type>().value(i)),
        DataType::UInt64 => Value::from(a.as_primitive::<UInt64Type>().value(i)),
        DataType::Float16 => num(a.as_primitive::<Float16Type>().value(i).to_f64()),
        DataType::Float32 => num(a.as_primitive::<Float32Type>().value(i) as f64),
        DataType::Float64 => num(a.as_primitive::<Float64Type>().value(i)),
        DataType::Utf8 => Value::String(a.as_string::<i32>().value(i).to_string()),
        DataType::LargeUtf8 => Value::String(a.as_string::<i64>().value(i).to_string()),
        DataType::Utf8View => Value::String(a.as_string_view().value(i).to_string()),
        DataType::List(_) => list(a.as_list::<i32>().value(i).as_ref()),
        DataType::LargeList(_) => list(a.as_list::<i64>().value(i).as_ref()),
        DataType::FixedSizeList(_, _) => list(a.as_fixed_size_list().value(i).as_ref()),
        DataType::Struct(fields) => {
            let s = a.as_struct();
            let mut m = Map::new();
            for (f, col) in fields.iter().zip(s.columns()) {
                m.insert(f.name().clone(), cell(col.as_ref(), i));
            }
            Value::Object(m)
        }
        DataType::Map(_, _) => {
            let entries = a.as_map().value(i);
            let (k, v) = (entries.column(0), entries.column(1));
            Value::Array((0..entries.len()).map(|j| Value::Array(vec![cell(k.as_ref(), j), cell(v.as_ref(), j)])).collect())
        }
        DataType::Dictionary(_, _) => {
            let d = a.as_any_dictionary();
            let keys = d.normalized_keys();
            match keys.get(i) {
                Some(&k) => cell(d.values().as_ref(), k),
                None => Value::Null,
            }
        }
        DataType::Decimal128(_, _) | DataType::Decimal256(_, _) => match formatted(a, i) {
            Value::String(s) => s.parse::<f64>().map_or(Value::Null, num),
            _ => Value::Null,
        },
        DataType::Binary | DataType::LargeBinary | DataType::FixedSizeBinary(_) | DataType::BinaryView => Value::Null,
        _ => formatted(a, i),
    }
}

fn list(a: &dyn Array) -> Value {
    Value::Array((0..a.len()).map(|j| cell(a, j)).collect())
}

/// Push a batch's rows to the window; batch columns are matched to `names` by name.
pub fn emit_batch(batch: &RecordBatch, index: &HashMap<String, usize>, n: usize, window: &mut Window, sink: &mut dyn RowSink) -> Flow {
    let schema = batch.schema();
    let cols: Vec<(usize, &dyn Array)> = schema
        .fields()
        .iter()
        .zip(batch.columns())
        .filter_map(|(f, c)| index.get(f.name()).map(|&i| (i, c.as_ref())))
        .collect();
    for r in 0..batch.num_rows() {
        if window.skip > 0 {
            window.skip -= 1;
            continue;
        }
        let mut row: Row = vec![Value::Null; n];
        for &(i, col) in &cols {
            row[i] = cell(col, r);
        }
        if window.push(row, sink) == Flow::Stop {
            return Flow::Stop;
        }
    }
    Flow::Continue
}

/// Which row groups to read for `skip` leading rows: (groups, rows still to skip in the first).
/// Returns None when the whole file is skipped (`skip` is reduced by its rows).
pub fn plan_row_groups(meta: &ParquetMetaData, skip: &mut u64) -> Option<(Vec<usize>, u64)> {
    let total = meta.file_metadata().num_rows().max(0) as u64;
    if *skip >= total {
        *skip -= total;
        return None;
    }
    let mut groups = Vec::new();
    let mut first_skip = 0;
    for (g, rg) in meta.row_groups().iter().enumerate() {
        let n = rg.num_rows().max(0) as u64;
        if groups.is_empty() && *skip >= n {
            *skip -= n;
            continue;
        }
        if groups.is_empty() {
            first_skip = *skip;
            *skip = 0;
        }
        groups.push(g);
    }
    Some((groups, first_skip))
}

pub fn batch_size(remaining: Option<u64>, first_skip: u64) -> usize {
    match remaining {
        None => BATCH_ROWS,
        Some(r) => (r + first_skip).clamp(1, BATCH_ROWS as u64) as usize,
    }
}

pub fn parquet_problem(e: impl std::fmt::Display) -> Problem {
    Problem::new(format!("Couldn't read this file as Parquet: {e}"))
}

pub struct ParquetFileTable {
    path: PathBuf,
    names: Vec<String>,
    roots: Vec<usize>,
    hints: Vec<Option<Hint>>,
    index: HashMap<String, usize>,
    total_rows: u64,
    small: bool,
}

impl ParquetFileTable {
    pub fn open(path: &Path) -> Result<Self> {
        let f = File::open(path).map_err(parquet_problem)?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(f).map_err(parquet_problem)?;
        let (names, roots, hints) = usable_columns(builder.schema());
        let total_rows = builder.metadata().file_metadata().num_rows().max(0) as u64;
        let small = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) <= FULL_PASS_MAX_BYTES;
        let index = names.iter().enumerate().map(|(i, n)| (n.clone(), i)).collect();
        Ok(Self { path: path.to_path_buf(), names, roots, hints, index, total_rows, small })
    }
}

impl Table for ParquetFileTable {
    fn format(&self) -> &'static str {
        "parquet"
    }
    fn names(&self) -> &[String] {
        &self.names
    }
    fn total_rows(&self) -> Option<u64> {
        Some(self.total_rows)
    }
    fn small(&self) -> bool {
        self.small
    }
    fn hints(&self) -> Vec<Option<Hint>> {
        self.hints.clone()
    }
    fn scan(&mut self, columns: Option<&[usize]>, offset: u64, limit: Option<u64>, sink: &mut dyn RowSink) -> Result<()> {
        let f = File::open(&self.path).map_err(parquet_problem)?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(f).map_err(parquet_problem)?;
        let mut skip = offset;
        let Some((groups, first_skip)) = plan_row_groups(builder.metadata(), &mut skip) else { return Ok(()) };
        let roots: Vec<usize> = match columns {
            Some(c) => c.iter().filter_map(|&i| self.roots.get(i).copied()).collect(),
            None => self.roots.clone(),
        };
        let mask = ProjectionMask::roots(builder.parquet_schema(), roots);
        let reader = builder
            .with_projection(mask)
            .with_row_groups(groups)
            .with_batch_size(batch_size(limit, first_skip))
            .build()
            .map_err(parquet_problem)?;
        let mut window = Window::new(first_skip, limit);
        for batch in reader {
            let batch = batch.map_err(parquet_problem)?;
            if emit_batch(&batch, &self.index, self.names.len(), &mut window, sink) == Flow::Stop || window.done() {
                break;
            }
        }
        Ok(())
    }
}
