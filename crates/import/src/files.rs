//! Uploaded files: format and encoding detection, and the CSV/TSV, JSON and JSONL tables.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use stellarator_core::schemas::ImportFormat;

use crate::parquet_file::ParquetFileTable;
use crate::problem::{thousands, Problem, Result};
use crate::table::{Flow, Row, RowSink, Table, TableSource, Window, SAMPLE_ROWS};

/// Files up to this size are read in full for a preview (exact prompt counts and warnings).
pub const FULL_PASS_MAX_BYTES: u64 = 50 * 1024 * 1024;

pub const LATIN1_WARNING: &str =
    "This file is not valid UTF-8, so it was read as Latin-1. Accented characters may look wrong.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Latin1,
}

/// UTF-8 when the whole file is valid UTF-8 (a BOM is tolerated), else Latin-1 with a warning.
pub fn detect_encoding(path: &Path) -> io::Result<(Encoding, Vec<String>)> {
    let mut f = File::open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    let mut carry = 0usize;
    loop {
        let n = f.read(&mut buf[carry..])?;
        if n == 0 {
            if carry > 0 {
                return Ok((Encoding::Latin1, vec![LATIN1_WARNING.into()]));
            }
            return Ok((Encoding::Utf8, Vec::new()));
        }
        let len = carry + n;
        match std::str::from_utf8(&buf[..len]) {
            Ok(_) => carry = 0,
            Err(e) if e.error_len().is_some() => return Ok((Encoding::Latin1, vec![LATIN1_WARNING.into()])),
            Err(e) => {
                // An incomplete sequence at the end of the chunk: keep it for the next read.
                let valid = e.valid_up_to();
                buf.copy_within(valid..len, 0);
                carry = len - valid;
            }
        }
    }
}

/// Latin-1 bytes in, UTF-8 bytes out.
struct Latin1Reader<R> {
    inner: R,
    raw: Vec<u8>,
    out: Vec<u8>,
    pos: usize,
}

impl<R: Read> Read for Latin1Reader<R> {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.out.len() {
            let n = self.inner.read(&mut self.raw)?;
            if n == 0 {
                return Ok(0);
            }
            self.out.clear();
            self.pos = 0;
            for &b in &self.raw[..n] {
                if b < 0x80 {
                    self.out.push(b);
                } else {
                    self.out.push(0xC0 | (b >> 6));
                    self.out.push(0x80 | (b & 0x3F));
                }
            }
        }
        let k = dst.len().min(self.out.len() - self.pos);
        dst[..k].copy_from_slice(&self.out[self.pos..self.pos + k]);
        self.pos += k;
        Ok(k)
    }
}

/// The file as UTF-8 bytes: a leading BOM skipped, Latin-1 transcoded.
pub fn open_text(path: &Path, enc: Encoding) -> io::Result<Box<dyn BufRead + Send>> {
    let f = File::open(path)?;
    match enc {
        Encoding::Utf8 => {
            let mut r = BufReader::with_capacity(1 << 16, f);
            if r.fill_buf()?.starts_with(b"\xEF\xBB\xBF") {
                r.consume(3);
            }
            Ok(Box::new(r))
        }
        Encoding::Latin1 => Ok(Box::new(BufReader::with_capacity(
            1 << 16,
            Latin1Reader { inner: f, raw: vec![0; 1 << 15], out: Vec::new(), pos: 0 },
        ))),
    }
}

pub fn read_text(path: &Path) -> io::Result<(String, Vec<String>)> {
    let (enc, warnings) = detect_encoding(path)?;
    let mut s = String::new();
    let mut bytes = Vec::new();
    open_text(path, enc)?.read_to_end(&mut bytes)?;
    s.push_str(&String::from_utf8_lossy(&bytes));
    Ok((s, warnings))
}

// ------------------------------------------------------------------------------------ format

pub fn format_for_extension(name: &str) -> Option<&'static str> {
    let ext = Path::new(name).extension()?.to_str()?.to_lowercase();
    Some(match ext.as_str() {
        "txt" | "text" => "text",
        "md" | "markdown" => "markdown",
        "csv" => "csv",
        "tsv" => "tsv",
        "json" => "json",
        "jsonl" | "ndjson" => "jsonl",
        "parquet" => "parquet",
        _ => return None,
    })
}

/// Format from the extension, else by sniffing the content.
pub fn detect_format(path: &Path, filename: Option<&str>) -> &'static str {
    let name = filename.map(str::to_string).unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    if let Some(f) = format_for_extension(&name) {
        return f;
    }
    let mut head = Vec::with_capacity(65536);
    match File::open(path) {
        Ok(f) => {
            if f.take(65536).read_to_end(&mut head).is_err() {
                return "text";
            }
        }
        Err(_) => return "text",
    }
    if head.starts_with(b"PAR1") {
        return "parquet";
    }
    let text = String::from_utf8_lossy(&head);
    let text = text.trim_start_matches('\u{feff}');
    let stripped = text.trim_start();
    if stripped.starts_with('[') {
        return "json";
    }
    if stripped.starts_with('{') {
        let lines: Vec<&str> = stripped.split('\n').filter(|l| !l.trim().is_empty()).collect();
        if lines.len() >= 2 && matches!(serde_json::from_str::<Value>(lines[0]), Ok(Value::Object(_))) {
            return "jsonl";
        }
        return "json";
    }
    let all: Vec<&str> = text.split('\n').collect();
    let mut lines: Vec<&str> = all[..all.len().saturating_sub(1)].iter().copied().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        lines = all.iter().copied().filter(|l| !l.trim().is_empty()).collect();
    }
    if lines.len() >= 2 {
        for (delim, fmt) in [('\t', "tsv"), (',', "csv"), (';', "csv"), ('|', "csv")] {
            let counts: HashSet<usize> = lines.iter().take(20).map(|l| l.matches(delim).count()).collect();
            if counts.len() == 1 && counts.iter().next().is_some_and(|&c| c >= 1) {
                return fmt;
            }
        }
    }
    "text"
}

pub fn format_name(f: ImportFormat) -> &'static str {
    match f {
        ImportFormat::Auto => "auto",
        ImportFormat::Text => "text",
        ImportFormat::Markdown => "markdown",
        ImportFormat::Csv => "csv",
        ImportFormat::Tsv => "tsv",
        ImportFormat::Json => "json",
        ImportFormat::Jsonl => "jsonl",
        ImportFormat::Parquet => "parquet",
    }
}

/// Plain text or Markdown read whole (or pasted), split into prompts by a split mode.
pub struct TextDoc {
    pub text: String,
    /// "text" | "markdown" | "paste"
    pub kind: &'static str,
    pub warnings: Vec<String>,
}

impl TextDoc {
    pub fn markdown(&self) -> Option<bool> {
        match self.kind {
            "markdown" => Some(true),
            "text" => Some(false),
            _ => None,
        }
    }
}

pub enum Opened {
    Text(TextDoc),
    Table(TableSource),
}

fn io_problem(fmt: &str, e: impl fmt::Display) -> Problem {
    Problem::new(format!("Couldn't read this file as {fmt}: {e}"))
}

/// Open an uploaded file. Blocking: reads the file's start (and checks its encoding).
pub fn open_path(path: &Path, format: ImportFormat, filename: Option<&str>) -> Result<Opened> {
    let fmt = match format {
        ImportFormat::Auto => detect_format(path, filename),
        other => format_name(other),
    };
    let table: Box<dyn Table> = match fmt {
        "text" | "markdown" => {
            let (text, warnings) = read_text(path).map_err(|e| io_problem(fmt, e))?;
            let kind = if fmt == "markdown" { "markdown" } else { "text" };
            return Ok(Opened::Text(TextDoc { text, kind, warnings }));
        }
        "csv" | "tsv" => Box::new(CsvTable::open(path, fmt)?),
        "jsonl" => Box::new(JsonlTable::open(path)?),
        "json" => match JsonTable::open(path)? {
            JsonOpen::Table(t) => Box::new(t),
            JsonOpen::Lines => Box::new(JsonlTable::open(path)?),
        },
        "parquet" => Box::new(ParquetFileTable::open(path)?),
        other => return Err(Problem::new(format!("Unknown format '{other}'."))),
    };
    Ok(Opened::Table(TableSource::new(table)?))
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

// ------------------------------------------------------------------------------------ CSV

fn unique_headers(raw: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(raw.len());
    let mut seen: HashSet<String> = HashSet::new();
    for (i, h) in raw.iter().enumerate() {
        let trimmed = h.trim().trim_start_matches('\u{feff}');
        let base = if trimmed.is_empty() { format!("column_{}", i + 1) } else { trimmed.to_string() };
        let mut name = base.clone();
        let mut n = 2;
        while seen.contains(&name) {
            name = format!("{base}_{n}");
            n += 1;
        }
        seen.insert(name.clone());
        out.push(name);
    }
    out
}

fn blank(record: &csv::ByteRecord) -> bool {
    record.iter().all(|c| String::from_utf8_lossy(c).trim().is_empty())
}

/// The delimiter that splits the file's start into the most consistent number of fields.
pub fn sniff_delimiter(head: &[u8]) -> u8 {
    let head = match head.iter().rposition(|&b| b == b'\n') {
        Some(cut) if cut > 0 => &head[..cut],
        _ => head,
    };
    let mut best: Option<(f64, usize, u8)> = None;
    for (rank, d) in [b',', b'\t', b';', b'|'].into_iter().enumerate() {
        let mut rdr = csv::ReaderBuilder::new().delimiter(d).has_headers(false).flexible(true).from_reader(head);
        let mut counts: HashMap<usize, usize> = HashMap::new();
        let mut total = 0usize;
        for rec in rdr.byte_records().take(200) {
            let Ok(rec) = rec else { break };
            if blank(&rec) {
                continue;
            }
            *counts.entry(rec.len()).or_default() += 1;
            total += 1;
        }
        let Some((&fields, &freq)) = counts.iter().max_by_key(|(f, n)| (**n, **f)) else { continue };
        if fields < 2 {
            continue;
        }
        let consistency = freq as f64 / total as f64;
        let better = match best {
            None => true,
            Some((c, r, _)) => consistency > c + 1e-9 || ((consistency - c).abs() <= 1e-9 && rank < r),
        };
        if better {
            best = Some((consistency, rank, d));
        }
    }
    best.map_or(b',', |(_, _, d)| d)
}

pub struct CsvTable {
    path: PathBuf,
    fmt: &'static str,
    enc: Encoding,
    delimiter: u8,
    names: Vec<String>,
    small: bool,
    warnings: Vec<String>,
}

impl CsvTable {
    pub fn open(path: &Path, fmt: &'static str) -> Result<Self> {
        let (enc, warnings) = detect_encoding(path).map_err(|e| io_problem(fmt, e))?;
        let mut t = Self {
            path: path.to_path_buf(),
            fmt,
            enc,
            delimiter: b'\t',
            names: Vec::new(),
            small: file_size(path) <= FULL_PASS_MAX_BYTES,
            warnings,
        };
        if fmt != "tsv" {
            let mut head = Vec::new();
            open_text(path, enc).map_err(|e| io_problem(fmt, e))?.take(65536).read_to_end(&mut head).map_err(|e| io_problem(fmt, e))?;
            t.delimiter = sniff_delimiter(&head);
        }
        let mut rdr = t.reader()?;
        let mut rec = csv::ByteRecord::new();
        loop {
            match rdr.read_byte_record(&mut rec) {
                Ok(true) if blank(&rec) => continue,
                Ok(true) => {
                    let raw: Vec<String> = rec.iter().map(|c| String::from_utf8_lossy(c).into_owned()).collect();
                    t.names = unique_headers(&raw);
                    return Ok(t);
                }
                Ok(false) => return Err(Problem::new("That file is empty.")),
                Err(e) => return Err(Problem::new(format!("Couldn't read this file as {}: {e}.", fmt.to_uppercase()))),
            }
        }
    }

    fn reader(&self) -> Result<csv::Reader<Box<dyn BufRead + Send>>> {
        let inner = open_text(&self.path, self.enc).map_err(|e| io_problem(self.fmt, e))?;
        Ok(csv::ReaderBuilder::new()
            .delimiter(self.delimiter)
            .has_headers(false)
            .flexible(true)
            .buffer_capacity(1 << 16)
            .from_reader(inner))
    }
}

impl Table for CsvTable {
    fn format(&self) -> &'static str {
        self.fmt
    }
    fn names(&self) -> &[String] {
        &self.names
    }
    fn total_rows(&self) -> Option<u64> {
        None
    }
    fn small(&self) -> bool {
        self.small
    }
    fn numeric_strings(&self) -> bool {
        true
    }
    fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }
    fn scan(&mut self, columns: Option<&[usize]>, offset: u64, limit: Option<u64>, sink: &mut dyn RowSink) -> Result<()> {
        let n = self.names.len();
        let wanted: Vec<bool> = match columns {
            Some(c) => (0..n).map(|i| c.contains(&i)).collect(),
            None => vec![true; n],
        };
        let mut rdr = self.reader()?;
        let mut rec = csv::ByteRecord::new();
        let mut header_seen = false;
        let mut window = Window::new(offset, limit);
        loop {
            match rdr.read_byte_record(&mut rec) {
                Ok(false) => return Ok(()),
                Ok(true) => {}
                Err(e) => {
                    let line = rdr.position().line();
                    return Err(Problem::new(format!(
                        "Couldn't read this file as {} near line {line}: {e}.",
                        self.fmt.to_uppercase()
                    )));
                }
            }
            if blank(&rec) {
                continue;
            }
            if !header_seen {
                header_seen = true;
                continue;
            }
            if window.skip > 0 {
                window.skip -= 1;
                continue;
            }
            let mut row: Row = vec![Value::Null; n];
            for (i, cell) in rec.iter().take(n).enumerate() {
                if wanted[i] {
                    row[i] = Value::String(String::from_utf8_lossy(cell).into_owned());
                }
            }
            for (i, slot) in row.iter_mut().enumerate().skip(rec.len()) {
                if wanted[i] {
                    *slot = Value::String(String::new());
                }
            }
            if window.push(row, sink) == Flow::Stop {
                return Ok(());
            }
        }
    }
}

// ------------------------------------------------------------------------------------ JSON rows

/// One parsed JSON item: an object as an aligned row, a bare string, or something else.
enum Item {
    Row(Row),
    Str(String),
    Other,
}

/// Parses an object into a row aligned with `index`, skipping keys that aren't needed without
/// building their values (a distilled dataset's long responses cost nothing).
struct RowSeed<'a> {
    index: &'a HashMap<String, usize>,
    wanted: &'a [bool],
}

impl<'de> DeserializeSeed<'de> for RowSeed<'_> {
    type Value = Item;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> std::result::Result<Item, D::Error> {
        d.deserialize_any(self)
    }
}

macro_rules! other_items {
    () => {
        fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<Self::Value, E> {
            Ok(Self::Value::other())
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<Self::Value, E> {
            Ok(Self::Value::other())
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<Self::Value, E> {
            Ok(Self::Value::other())
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<Self::Value, E> {
            Ok(Self::Value::other())
        }
        fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(Self::Value::other())
        }
        fn visit_none<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(Self::Value::other())
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error> {
            while seq.next_element::<IgnoredAny>()?.is_some() {}
            Ok(Self::Value::other())
        }
    };
}

trait OtherItem {
    fn other() -> Self;
}
impl OtherItem for Item {
    fn other() -> Self {
        Item::Other
    }
}

impl<'de> Visitor<'de> for RowSeed<'_> {
    type Value = Item;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON object or string")
    }
    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Item, E> {
        Ok(Item::Str(v.to_string()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Item, E> {
        Ok(Item::Str(v))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Item, A::Error> {
        let mut row: Row = vec![Value::Null; self.wanted.len()];
        while let Some(key) = map.next_key::<String>()? {
            match self.index.get(&key) {
                Some(&i) if self.wanted[i] => row[i] = map.next_value::<Value>()?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Item::Row(row))
    }
    other_items!();
}

/// An object's keys in order with their values (for discovering column names).
enum OrderedItem {
    Pairs(Vec<(String, Value)>),
    Str(String),
    Other,
}
impl OtherItem for OrderedItem {
    fn other() -> Self {
        OrderedItem::Other
    }
}

struct OrderedSeed;

impl<'de> DeserializeSeed<'de> for OrderedSeed {
    type Value = OrderedItem;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> std::result::Result<OrderedItem, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for OrderedSeed {
    type Value = OrderedItem;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON object or string")
    }
    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<OrderedItem, E> {
        Ok(OrderedItem::Str(v.to_string()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<OrderedItem, E> {
        Ok(OrderedItem::Str(v))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<OrderedItem, A::Error> {
        let mut pairs: Vec<(String, Value)> = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            let v = map.next_value::<Value>()?;
            if let Some(slot) = pairs.iter_mut().find(|(k, _)| *k == key) {
                slot.1 = v;
            } else {
                pairs.push((key, v));
            }
        }
        Ok(OrderedItem::Pairs(pairs))
    }
    other_items!();
}

/// Column names in first-seen order.
#[derive(Default)]
struct NameSet {
    names: Vec<String>,
    seen: HashSet<String>,
}

impl NameSet {
    fn add(&mut self, key: &str) {
        if !self.seen.contains(key) {
            self.seen.insert(key.to_string());
            self.names.push(key.to_string());
        }
    }
}

fn index_of(names: &[String]) -> HashMap<String, usize> {
    names.iter().enumerate().map(|(i, n)| (n.clone(), i)).collect()
}

fn item_to_row(item: Item, prompt_idx: Option<usize>, n: usize) -> Option<Row> {
    match item {
        Item::Row(r) => Some(r),
        Item::Str(s) => {
            let mut row = vec![Value::Null; n];
            if let Some(i) = prompt_idx {
                row[i] = Value::String(s);
            }
            Some(row)
        }
        Item::Other => None,
    }
}

fn wanted_mask(n: usize, columns: Option<&[usize]>) -> Vec<bool> {
    match columns {
        Some(c) => (0..n).map(|i| c.contains(&i)).collect(),
        None => vec![true; n],
    }
}

// ------------------------------------------------------------------------------------ JSONL

pub struct JsonlTable {
    path: PathBuf,
    enc: Encoding,
    names: Vec<String>,
    index: HashMap<String, usize>,
    small: bool,
    warnings: Vec<String>,
    bad_note: Option<String>,
}

impl JsonlTable {
    pub fn open(path: &Path) -> Result<Self> {
        let (enc, warnings) = detect_encoding(path).map_err(|e| io_problem("jsonl", e))?;
        let mut names = NameSet::default();
        let mut reader = open_text(path, enc).map_err(|e| io_problem("jsonl", e))?;
        let mut line = Vec::new();
        let mut rows = 0;
        while rows < SAMPLE_ROWS {
            line.clear();
            if reader.read_until(b'\n', &mut line).map_err(|e| io_problem("jsonl", e))? == 0 {
                break;
            }
            let text = String::from_utf8_lossy(&line);
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let mut de = serde_json::Deserializer::from_str(text);
            match OrderedSeed.deserialize(&mut de).and_then(|v| de.end().map(|_| v)) {
                Ok(OrderedItem::Pairs(pairs)) => {
                    for (k, _) in &pairs {
                        names.add(k);
                    }
                    rows += 1;
                }
                Ok(OrderedItem::Str(_)) => {
                    names.add("prompt");
                    rows += 1;
                }
                _ => {}
            }
        }
        let names = names.names;
        Ok(Self {
            path: path.to_path_buf(),
            enc,
            index: index_of(&names),
            names,
            small: file_size(path) <= FULL_PASS_MAX_BYTES,
            warnings,
            bad_note: None,
        })
    }
}

impl Table for JsonlTable {
    fn format(&self) -> &'static str {
        "jsonl"
    }
    fn names(&self) -> &[String] {
        &self.names
    }
    fn total_rows(&self) -> Option<u64> {
        None
    }
    fn small(&self) -> bool {
        self.small
    }
    fn warnings(&self) -> Vec<String> {
        let mut w = self.warnings.clone();
        w.extend(self.bad_note.clone());
        w
    }
    fn scan(&mut self, columns: Option<&[usize]>, offset: u64, limit: Option<u64>, sink: &mut dyn RowSink) -> Result<()> {
        let n = self.names.len();
        let wanted = wanted_mask(n, columns);
        let prompt_idx = self.index.get("prompt").copied();
        let mut reader = open_text(&self.path, self.enc).map_err(|e| io_problem("jsonl", e))?;
        let mut window = Window::new(offset, limit);
        let mut line = Vec::new();
        let mut lineno = 0u64;
        let mut bad = 0u64;
        let mut first_bad = 0u64;
        let result = loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) => break Ok(()),
                Ok(_) => {}
                Err(e) => break Err(io_problem("jsonl", e)),
            }
            lineno += 1;
            let text = String::from_utf8_lossy(&line);
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let mut de = serde_json::Deserializer::from_str(text);
            let seed = RowSeed { index: &self.index, wanted: &wanted };
            let parsed = seed.deserialize(&mut de).and_then(|v| de.end().map(|_| v));
            let row = match parsed {
                Ok(item) => item_to_row(item, prompt_idx, n),
                Err(_) => None,
            };
            let Some(row) = row else {
                bad += 1;
                if first_bad == 0 {
                    first_bad = lineno;
                }
                continue;
            };
            if window.push(row, sink) == Flow::Stop {
                break Ok(());
            }
        };
        self.bad_note = (bad > 0).then(|| {
            let one = bad == 1;
            format!(
                "{} {} not valid JSON objects or strings and {} skipped (first: line {first_bad}).",
                thousands(bad),
                if one { "line was" } else { "lines were" },
                if one { "was" } else { "were" },
            )
        });
        result
    }
}

// ------------------------------------------------------------------------------------ JSON

/// Names in the JSON list keys Python looked for first, in order.
const JSON_LIST_KEYS: &[&str] = &["data", "prompts", "rows", "examples", "items", "train", "records", "questions"];

#[derive(Default)]
struct ListStats {
    elems: u64,
    rows: u64,
    names: NameSet,
}

/// Consumes one list item, recording its keys.
struct SurveyItem<'a>(&'a mut ListStats);

impl<'de> DeserializeSeed<'de> for SurveyItem<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for SurveyItem<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_str<E: de::Error>(self, _: &str) -> std::result::Result<(), E> {
        self.0.rows += 1;
        self.0.names.add("prompt");
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            self.0.names.add(&key);
            map.next_value::<IgnoredAny>()?;
        }
        self.0.rows += 1;
        Ok(())
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<(), A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(())
    }
}

/// A list's items, surveyed.
struct SurveyList;

impl<'de> DeserializeSeed<'de> for SurveyList {
    type Value = Option<ListStats>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> std::result::Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for SurveyList {
    type Value = Option<ListStats>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error> {
        let mut stats = ListStats::default();
        loop {
            let item = SurveyItem(&mut stats);
            if seq.next_element_seed(item)?.is_none() {
                break;
            }
            stats.elems += 1;
        }
        Ok(Some(stats))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Self::Value, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(None)
    }
    fn visit_str<E: de::Error>(self, _: &str) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
        Ok(None)
    }
}

enum Survey {
    /// The document is a list.
    List(ListStats),
    /// An object: its list-valued keys in order, with which occurrence of the key it was.
    Object(Vec<(String, ListStats, usize)>),
    Scalar,
}

struct SurveyDoc;

impl<'de> DeserializeSeed<'de> for SurveyDoc {
    type Value = Survey;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> std::result::Result<Survey, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for SurveyDoc {
    type Value = Survey;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> std::result::Result<Survey, A::Error> {
        Ok(SurveyList.visit_seq(seq)?.map_or(Survey::Scalar, Survey::List))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Survey, A::Error> {
        let mut lists: Vec<(String, ListStats, usize)> = Vec::new();
        let mut occurrences: HashMap<String, usize> = HashMap::new();
        while let Some(key) = map.next_key::<String>()? {
            let nth = {
                let n = occurrences.entry(key.clone()).or_default();
                *n += 1;
                *n
            };
            match map.next_value_seed(SurveyList)? {
                Some(stats) => {
                    // json.load keeps the last value of a repeated key.
                    lists.retain(|(k, _, _)| *k != key);
                    lists.push((key, stats, nth));
                }
                None => lists.retain(|(k, _, _)| *k != key),
            }
        }
        Ok(Survey::Object(lists))
    }
    fn visit_str<E: de::Error>(self, _: &str) -> std::result::Result<Survey, E> {
        Ok(Survey::Scalar)
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<Survey, E> {
        Ok(Survey::Scalar)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<Survey, E> {
        Ok(Survey::Scalar)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<Survey, E> {
        Ok(Survey::Scalar)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<Survey, E> {
        Ok(Survey::Scalar)
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Survey, E> {
        Ok(Survey::Scalar)
    }
}

/// Streams the items of the chosen list into a sink.
struct Stream<'a> {
    key: Option<&'a str>,
    index: &'a HashMap<String, usize>,
    wanted: &'a [bool],
    prompt_idx: Option<usize>,
    window: &'a mut Window,
    sink: &'a mut dyn RowSink,
    /// Set when the sink asked to stop (parsing is then aborted with an error).
    stopped: &'a mut bool,
    /// Which occurrence of the key to read (the last one, like json.load).
    occurrence: usize,
}

const STOP: &str = "\u{0}stop";

impl<'de> DeserializeSeed<'de> for Stream<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Stream<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON list or object")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<(), A::Error> {
        let n = self.wanted.len();
        loop {
            let seed = RowSeed { index: self.index, wanted: self.wanted };
            let Some(item) = seq.next_element_seed(seed)? else { return Ok(()) };
            let Some(row) = item_to_row(item, self.prompt_idx, n) else { continue };
            if self.window.push(row, self.sink) == Flow::Stop {
                *self.stopped = true;
                return Err(de::Error::custom(STOP));
            }
        }
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<(), A::Error> {
        let Some(key) = self.key else {
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            return Ok(());
        };
        let mut seen = 0usize;
        while let Some(k) = map.next_key::<String>()? {
            if k == key {
                seen += 1;
                if seen == self.occurrence {
                    let inner = Stream {
                        key: None,
                        index: self.index,
                        wanted: self.wanted,
                        prompt_idx: self.prompt_idx,
                        window: &mut *self.window,
                        sink: &mut *self.sink,
                        stopped: &mut *self.stopped,
                        occurrence: 0,
                    };
                    map.next_value_seed(inner)?;
                    continue;
                }
            }
            map.next_value::<IgnoredAny>()?;
        }
        Ok(())
    }
}

pub enum JsonOpen {
    Table(JsonTable),
    /// Several JSON values one after another: really JSON Lines.
    Lines,
}

/// A JSON file streamed from disk: a list of strings or objects, or an object holding one.
pub struct JsonTable {
    path: PathBuf,
    enc: Encoding,
    key: Option<String>,
    occurrence: usize,
    names: Vec<String>,
    index: HashMap<String, usize>,
    rows: u64,
    small: bool,
    warnings: Vec<String>,
}

impl JsonTable {
    pub fn open(path: &Path) -> Result<JsonOpen> {
        let (enc, warnings) = detect_encoding(path).map_err(|e| io_problem("json", e))?;
        let reader = open_text(path, enc).map_err(|e| io_problem("json", e))?;
        let mut de = serde_json::Deserializer::from_reader(reader);
        let survey = SurveyDoc.deserialize(&mut de).and_then(|s| de.end().map(|_| s));
        let survey = match survey {
            Ok(s) => s,
            Err(e) if e.to_string().starts_with("trailing characters") => return Ok(JsonOpen::Lines),
            Err(e) => return Err(Problem::new(format!("Couldn't read this file as JSON: {e}."))),
        };
        let (key, stats, occurrence) = match survey {
            Survey::List(stats) => (None, stats, 0),
            Survey::Object(lists) => {
                if lists.is_empty() {
                    return Err(Problem::new(
                        "Couldn't find a list of prompts in this JSON. Use an array, or an object like {\"data\": [...]}.",
                    ));
                }
                let pick = JSON_LIST_KEYS
                    .iter()
                    .find_map(|k| lists.iter().position(|(name, _, _)| name == k))
                    .unwrap_or_else(|| {
                        // The longest list; the first of equals.
                        let mut best = 0;
                        for (i, (_, s, _)) in lists.iter().enumerate() {
                            if s.elems > lists[best].1.elems {
                                best = i;
                            }
                        }
                        best
                    });
                let (k, s, nth) = lists.into_iter().nth(pick).expect("index in range");
                (Some(k), s, nth)
            }
            Survey::Scalar => return Err(Problem::new("Couldn't find a list of prompts in this JSON.")),
        };
        if stats.rows == 0 {
            return Err(Problem::new("This JSON has no strings or objects to use as prompts."));
        }
        let names = stats.names.names;
        Ok(JsonOpen::Table(JsonTable {
            path: path.to_path_buf(),
            enc,
            key,
            occurrence,
            index: index_of(&names),
            names,
            rows: stats.rows,
            small: file_size(path) <= FULL_PASS_MAX_BYTES,
            warnings,
        }))
    }
}

impl Table for JsonTable {
    fn format(&self) -> &'static str {
        "json"
    }
    fn names(&self) -> &[String] {
        &self.names
    }
    fn total_rows(&self) -> Option<u64> {
        Some(self.rows)
    }
    fn small(&self) -> bool {
        self.small
    }
    fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }
    fn scan(&mut self, columns: Option<&[usize]>, offset: u64, limit: Option<u64>, sink: &mut dyn RowSink) -> Result<()> {
        let wanted = wanted_mask(self.names.len(), columns);
        let reader = open_text(&self.path, self.enc).map_err(|e| io_problem("json", e))?;
        let mut de = serde_json::Deserializer::from_reader(reader);
        let mut window = Window::new(offset, limit);
        let mut stopped = false;
        let stream = Stream {
            key: self.key.as_deref(),
            index: &self.index,
            wanted: &wanted,
            prompt_idx: self.index.get("prompt").copied(),
            window: &mut window,
            sink,
            stopped: &mut stopped,
            occurrence: self.occurrence,
        };
        match stream.deserialize(&mut de) {
            Ok(()) => Ok(()),
            Err(_) if stopped => Ok(()),
            Err(e) => Err(Problem::new(format!("Couldn't read this file as JSON: {e}."))),
        }
    }
}
