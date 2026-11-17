//! Tables: rows with named columns (CSV, TSV, JSON, JSONL, Parquet, Hugging Face).
//!
//! Rows are pushed to a [`RowSink`] so readers can stream from files, parsers and HTTP without
//! holding a dataset in memory. A row is a vector aligned with [`Table::names`]; columns a scan was
//! not asked for may be `Null`.

use serde_json::Value;

use crate::problem::Result;
use crate::schema::{infer_schema, Hint, Schema};

pub type Row = Vec<Value>;

/// Rows read to infer column kinds and pick fields.
pub const SAMPLE_ROWS: u64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Stop,
}

pub trait RowSink {
    fn row(&mut self, row: Row) -> Flow;
    /// A new part of the source starts (a Hugging Face split).
    fn part(&mut self, _name: &str) {}
}

impl<F: FnMut(Row) -> Flow> RowSink for F {
    fn row(&mut self, row: Row) -> Flow {
        self(row)
    }
}

pub trait Table: Send {
    /// "csv", "tsv", "json", "jsonl", "parquet", "huggingface".
    fn format(&self) -> &'static str;
    fn names(&self) -> &[String];
    /// Rows in the source, when known without reading it all.
    fn total_rows(&self) -> Option<u64>;
    /// Cheap enough to read in full for a preview.
    fn small(&self) -> bool {
        true
    }
    /// CSV cells are always strings; numeric-looking ones count as numbers.
    fn numeric_strings(&self) -> bool {
        false
    }
    /// Per-column hints from a typed schema (Parquet, Hugging Face features), aligned with names.
    fn hints(&self) -> Vec<Option<Hint>> {
        Vec::new()
    }
    /// Warnings about the source (encoding, bad lines seen by the last scan).
    fn warnings(&self) -> Vec<String> {
        Vec::new()
    }
    /// Push rows `offset..offset + limit` to `sink` until it says stop. `columns`: the column
    /// indices needed (None = all); others may be left `Null`.
    fn scan(&mut self, columns: Option<&[usize]>, offset: u64, limit: Option<u64>, sink: &mut dyn RowSink) -> Result<()>;
}

/// Rows held in memory (Hugging Face's first rows, a window read for a preview).
pub struct MemoryTable {
    pub rows: Vec<Row>,
    pub names: Vec<String>,
    pub format: &'static str,
    pub total_rows: Option<u64>,
    pub hints: Vec<Option<Hint>>,
    pub warnings: Vec<String>,
}

impl MemoryTable {
    pub fn new(names: Vec<String>, rows: Vec<Row>, format: &'static str, total_rows: Option<u64>) -> Self {
        let total_rows = total_rows.or(Some(rows.len() as u64));
        Self { rows, names, format, total_rows, hints: Vec::new(), warnings: Vec::new() }
    }
}

impl Table for MemoryTable {
    fn format(&self) -> &'static str {
        self.format
    }
    fn names(&self) -> &[String] {
        &self.names
    }
    fn total_rows(&self) -> Option<u64> {
        self.total_rows
    }
    fn hints(&self) -> Vec<Option<Hint>> {
        self.hints.clone()
    }
    fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }
    fn scan(&mut self, _columns: Option<&[usize]>, offset: u64, limit: Option<u64>, sink: &mut dyn RowSink) -> Result<()> {
        let rows = self.rows.iter().skip(offset as usize);
        let rows: Box<dyn Iterator<Item = &Row>> = match limit {
            Some(l) => Box::new(rows.take(l as usize)),
            None => Box::new(rows),
        };
        for r in rows {
            if sink.row(r.clone()) == Flow::Stop {
                break;
            }
        }
        Ok(())
    }
}

/// A table with its first rows read and its columns described.
pub struct TableSource {
    pub table: Box<dyn Table>,
    pub sample: Vec<Row>,
    pub schema: Schema,
}

impl TableSource {
    pub fn new(mut table: Box<dyn Table>) -> Result<Self> {
        let sample = read_rows(table.as_mut(), None, 0, Some(SAMPLE_ROWS))?;
        let schema = infer_schema(table.names(), &sample, table.numeric_strings(), &table.hints());
        Ok(Self { table, sample, schema })
    }

    /// For Hugging Face imports: describe columns from the first rows the preview used, read rows
    /// from the Parquet export.
    pub fn with_schema(table: Box<dyn Table>, sample: Vec<Row>, schema: Schema) -> Self {
        Self { table, sample, schema }
    }
}

/// Collect rows into a vector (small windows only).
pub fn read_rows(table: &mut dyn Table, columns: Option<&[usize]>, offset: u64, limit: Option<u64>) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    let mut sink = |r: Row| {
        rows.push(r);
        Flow::Continue
    };
    table.scan(columns, offset, limit, &mut sink)?;
    Ok(rows)
}

/// Apply an offset/limit window to a stream of rows as they arrive.
pub struct Window {
    pub skip: u64,
    pub remaining: Option<u64>,
}

impl Window {
    pub fn new(offset: u64, limit: Option<u64>) -> Self {
        Self { skip: offset, remaining: limit }
    }
    pub fn done(&self) -> bool {
        self.remaining == Some(0)
    }
    /// Push one row through the window. Stop once the limit is reached.
    pub fn push(&mut self, row: Row, sink: &mut dyn RowSink) -> Flow {
        if self.skip > 0 {
            self.skip -= 1;
            return Flow::Continue;
        }
        if let Some(r) = self.remaining.as_mut() {
            if *r == 0 {
                return Flow::Stop;
            }
            *r -= 1;
        }
        let flow = sink.row(row);
        if flow == Flow::Stop || self.done() {
            Flow::Stop
        } else {
            Flow::Continue
        }
    }
}
