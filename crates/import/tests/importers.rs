//! Ports of backend/tests/test_importers.py: files to prompts.

mod common;

use std::sync::Arc;
use std::time::Instant;

use arrow_array::builder::{ListBuilder, StringBuilder, StructBuilder};
use arrow_array::{ArrayRef, BinaryArray, Int64Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Fields};
use common::*;
use serde_json::json;
use stellarator_core::schemas::{ImportFormat, SplitMode};
use stellarator_import::files::{detect_format, open_path, Opened};
use stellarator_import::prompts::{preview_table, preview_text, PreviewOptions};

fn users(r: &Read) -> Vec<String> {
    r.users()
}

#[test]
fn text_file_dedupe_offset_limit() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("p.txt");
    std::fs::write(&f, "a\nb\na\nc\nd\n").unwrap();
    let r = prompts_of(&f, &mapping(json!({"split_mode": "line"}))).unwrap();
    assert_eq!(users(&r), ["a", "b", "c", "d"]);
    assert_eq!(r.stats.duplicates, 1);
    let r = prompts_of(&f, &mapping(json!({"split_mode": "line", "dedupe": false}))).unwrap();
    assert_eq!(users(&r), ["a", "b", "a", "c", "d"]);
    let r = prompts_of(&f, &mapping(json!({"split_mode": "line", "offset": 1, "limit": 2, "dedupe": false}))).unwrap();
    assert_eq!(users(&r), ["b", "a"]);
}

#[test]
fn markdown_fixture_headings() {
    let r = prompts_of(&fixture("imports/doc.md"), &mapping(json!({}))).unwrap();
    assert_eq!(r.split_mode, Some(SplitMode::Heading));
    assert_eq!(users(&r), ["First\n\nBody one.", "Sub\n\nBody sub.", "Second\n\nBody two."]);
    assert!(!users(&r).iter().any(|u| u.contains("title:")));
}

#[test]
fn csv_bom_quoted_newlines() {
    let r = prompts_of(&fixture("imports/bom_quoted.csv"), &mapping(json!({}))).unwrap();
    assert_eq!(r.prompt_field.as_deref(), Some("prompt"));
    assert_eq!(users(&r), ["Line one\nline two", "Say \"hi\""]);
    // Reference answers stay in meta.
    assert_eq!(r.prompts[0].meta, vec![("id".to_string(), json!("1")), ("answer".to_string(), json!("a"))]);
}

#[test]
fn csv_schema_kinds() {
    let Opened::Table(src) = open_path(&fixture("imports/bom_quoted.csv"), ImportFormat::Auto, None).unwrap() else { panic!() };
    let kinds: Vec<_> = src.schema.columns.iter().map(|c| (c.name.as_str(), serde_json::to_value(&c.kind).unwrap())).collect();
    assert_eq!(kinds, [("id", json!("number")), ("prompt", json!("text")), ("answer", json!("text"))]);
    assert!(src.schema.columns[2].looks_like_output);
}

#[test]
fn csv_latin1_warning() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("l.csv");
    std::fs::write(&f, b"prompt\ncaf\xe9\n").unwrap();
    let r = prompts_of(&f, &mapping(json!({}))).unwrap();
    assert_eq!(users(&r), ["café"]);
    assert!(r.warnings.iter().any(|w| w.contains("Latin-1")));
}

#[test]
fn csv_dialects() {
    let dir = tempfile::tempdir().unwrap();
    // Semicolons with decimal commas.
    let f = dir.path().join("semi.csv");
    std::fs::write(&f, "prompt;score\nHello there;1,5\nGood day;2,5\n").unwrap();
    let r = prompts_of(&f, &mapping(json!({}))).unwrap();
    assert_eq!(users(&r), ["Hello there", "Good day"]);
    assert_eq!(r.prompts[0].meta, vec![("score".to_string(), json!("1,5"))]);
    // Pipes, CRLF, a short row padded, a blank row skipped, duplicate and empty headers renamed.
    let f = dir.path().join("pipe.csv");
    std::fs::write(&f, "prompt|x|x|\r\nWhy is the sky blue?|1|2|3\r\n\r\nShort row\r\n").unwrap();
    let Opened::Table(src) = open_path(&f, ImportFormat::Auto, None).unwrap() else { panic!() };
    assert_eq!(src.schema.names(), ["prompt", "x", "x_2", "column_4"]);
    let r = prompts_of(&f, &mapping(json!({}))).unwrap();
    assert_eq!(users(&r), ["Why is the sky blue?", "Short row"]);
    // Commas inside quotes don't confuse the sniffer.
    let f = dir.path().join("quoted.csv");
    std::fs::write(&f, "prompt,n\n\"a, b; c\",1\n\"d; e, f\",2\n").unwrap();
    assert_eq!(users(&prompts_of(&f, &mapping(json!({}))).unwrap()), ["a, b; c", "d; e, f"]);
}

#[test]
fn tsv() {
    let r = prompts_of(&fixture("imports/qa.tsv"), &mapping(json!({}))).unwrap();
    assert_eq!(r.prompt_field.as_deref(), Some("question"));
    assert_eq!(users(&r), ["What is 2+2?", "Name a color"]);
}

#[test]
fn json_alpaca_combines_input() {
    let r = prompts_of(&fixture("imports/alpaca.json"), &mapping(json!({}))).unwrap();
    assert_eq!(r.prompt_field.as_deref(), Some("instruction"));
    assert_eq!(users(&r), ["Translate\n\nhello", "Say hi"]);
    // "input" is part of the prompt; "output" is a generation and left out of meta (v2).
    assert!(r.prompts[0].meta.is_empty());
}

#[test]
fn json_shapes() {
    let dir = tempfile::tempdir().unwrap();
    let p = |name: &str, body: &str| {
        let f = dir.path().join(name);
        std::fs::write(&f, body).unwrap();
        f
    };
    let r = prompts_of(&p("a.json", r#"["one", "two", "one"]"#), &mapping(json!({}))).unwrap();
    assert_eq!(users(&r), ["one", "two"]);
    let r = prompts_of(&p("b.json", r#"{"data": [{"question": "q1"}, {"question": "q2", "extra": 3}]}"#), &mapping(json!({}))).unwrap();
    assert_eq!(users(&r), ["q1", "q2"]);
    assert_eq!(r.prompts[1].meta, vec![("extra".to_string(), json!(3))]);
    let r = prompts_of(&p("c.json", r#"{"prompts": ["x"], "name": "n"}"#), &mapping(json!({}))).unwrap();
    assert_eq!(users(&r), ["x"]);
    // The longest list when no known key; a repeated key keeps its last value, like json.load.
    let r = prompts_of(&p("e.json", r#"{"a": ["1"], "b": ["2", "3"], "a": ["4", "5", "6"]}"#), &mapping(json!({}))).unwrap();
    assert_eq!(users(&r), ["4", "5", "6"]);
    let e = prompts_of(&p("d.json", r#"{"a": 1}"#), &mapping(json!({}))).err().unwrap();
    assert!(e.message.starts_with("Couldn't find a list of prompts"), "{}", e.message);
    let e = prompts_of(&p("f.json", r#"[1, 2]"#), &mapping(json!({}))).err().unwrap();
    assert_eq!(e.message, "This JSON has no strings or objects to use as prompts.");
    let e = prompts_of(&p("g.json", r#"[{"prompt": "a"},"#), &mapping(json!({}))).err().unwrap();
    assert!(e.message.starts_with("Couldn't read this file as JSON:"), "{}", e.message);
    // Window over a streamed JSON array.
    let r = prompts_of(&p("h.json", r#"["a", "b", "c", "d"]"#), &mapping(json!({"offset": 1, "limit": 2}))).unwrap();
    assert_eq!(users(&r), ["b", "c"]);
}

#[test]
fn json_that_is_really_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("x.json");
    std::fs::write(&f, "{\"prompt\": \"a\"}\n{\"prompt\": \"b\"}\n").unwrap();
    assert_eq!(users(&prompts_of(&f, &mapping(json!({}))).unwrap()), ["a", "b"]);
}

#[test]
fn jsonl_sharegpt_bad_lines_and_trailing_assistant() {
    let r = prompts_of(&fixture("imports/share.jsonl"), &mapping(json!({}))).unwrap();
    assert_eq!(r.prompt_field.as_deref(), Some("conversations"));
    let msgs = |i: usize| r.prompts[i].messages_value();
    assert_eq!(msgs(0), [json!({"role": "system", "content": "Be brief."}), json!({"role": "user", "content": "Hello"})]);
    assert_eq!(
        msgs(1),
        [
            json!({"role": "user", "content": "Q2"}),
            json!({"role": "assistant", "content": "A2"}),
            json!({"role": "user", "content": "Q3"}),
        ]
    );
    assert!(r.warnings.iter().any(|w| w.contains("1 line was not valid JSON") && w.contains("line 2")), "{:?}", r.warnings);
}

#[test]
fn jsonl_strings_and_latin1() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("s.jsonl");
    std::fs::write(&f, "\"hello\"\n\"world\"\n").unwrap();
    assert_eq!(users(&prompts_of(&f, &mapping(json!({}))).unwrap()), ["hello", "world"]);
    let f = dir.path().join("l.jsonl");
    std::fs::write(&f, b"{\"prompt\": \"caf\xe9\"}\n").unwrap();
    let r = prompts_of(&f, &mapping(json!({}))).unwrap();
    assert_eq!(users(&r), ["café"]);
    assert!(r.warnings.iter().any(|w| w.contains("Latin-1")));
}

#[test]
fn system_field_auto_and_empty_prompts() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("s.csv");
    std::fs::write(&f, "system,prompt,score\nBe nice,Hi,3\n,,4\nBe nice,Hi,5\n,Yo,x\n").unwrap();
    let r = prompts_of(&f, &mapping(json!({}))).unwrap();
    assert_eq!(r.system_field.as_deref(), Some("system"));
    assert_eq!(r.prompts[0].messages_value(), [json!({"role": "system", "content": "Be nice"}), json!({"role": "user", "content": "Hi"})]);
    assert_eq!(r.prompts[1].messages_value(), [json!({"role": "user", "content": "Yo"})]);
    assert_eq!((r.stats.empty, r.stats.duplicates), (1, 1));
    assert!(r.warnings.iter().any(|w| w == "1 row had an empty prompt and was skipped."));
    assert!(r.warnings.iter().any(|w| w == "1 duplicate prompt was dropped."));
    // system_field "" turns detection off.
    let r = prompts_of(&f, &mapping(json!({"system_field": ""}))).unwrap();
    assert_eq!(r.system_field, None);
    assert_eq!(r.prompts[0].messages.len(), 1);
}

#[test]
fn autodetect_names_and_missing_column() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("a.csv");
    std::fs::write(&f, "id,title,body\n1,short,a much longer body of text here\n2,tiny,another longer body of text\n").unwrap();
    assert_eq!(prompts_of(&f, &mapping(json!({}))).unwrap().prompt_field.as_deref(), Some("body"));
    let g = dir.path().join("b.csv");
    std::fs::write(&g, "Question,notes\nWhy?,zzzzzzzzzzzzzzzz\n").unwrap();
    assert_eq!(prompts_of(&g, &mapping(json!({}))).unwrap().prompt_field.as_deref(), Some("Question"));
    let e = prompts_of(&g, &mapping(json!({"prompt_field": "nope"}))).err().unwrap();
    assert_eq!(e.status, 422);
    assert_eq!(e.message, "There is no column called 'nope' to use as the prompt. Columns are: Question, notes.");
}

#[test]
fn meta_scalars_truncated_and_nested_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("m.jsonl");
    std::fs::write(&f, format!("{{\"prompt\": \"p\", \"long\": \"{}\", \"nested\": {{\"a\": 1}}, \"n\": 2, \"lst\": [1], \"ok\": true}}\n", "é".repeat(9000))).unwrap();
    let r = prompts_of(&f, &mapping(json!({}))).unwrap();
    let meta: std::collections::HashMap<_, _> = r.prompts[0].meta.iter().cloned().collect();
    assert_eq!(meta["long"].as_str().unwrap().chars().count(), 4000);
    assert_eq!(meta["n"], json!(2));
    assert_eq!(meta["ok"], json!(true));
    assert!(!meta.contains_key("nested") && !meta.contains_key("lst"));
    // Meta keeps the columns' order when stored.
    let (_, meta_json) = r.prompts[0].encode();
    assert!(meta_json.starts_with("{\"long\":"), "{}", &meta_json[..20]);
}

fn messages_column(n: usize, offset: usize) -> ArrayRef {
    let fields = Fields::from(vec![Field::new("role", DataType::Utf8, true), Field::new("content", DataType::Utf8, true)]);
    let mut b = ListBuilder::new(StructBuilder::from_fields(fields.clone(), n * 2));
    for i in 0..n {
        let s = b.values();
        for (role, content) in [("user", format!("q{}", i + offset)), ("assistant", "a".to_string())] {
            s.field_builder::<StringBuilder>(0).unwrap().append_value(role);
            s.field_builder::<StringBuilder>(1).unwrap().append_value(content);
            s.append(true);
        }
        b.append(true);
    }
    Arc::new(b.finish())
}

#[test]
fn parquet_messages_and_window() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("d.parquet");
    let batch = RecordBatch::try_from_iter(vec![
        ("messages", messages_column(30, 0)),
        ("score", Arc::new(Int64Array::from_iter_values(0..30)) as ArrayRef),
        ("blob", Arc::new(BinaryArray::from_iter_values((0..30).map(|_| b"x".as_slice()))) as ArrayRef),
    ])
    .unwrap();
    std::fs::write(&f, parquet_bytes(&[batch], 7)).unwrap();
    let r = prompts_of(&f, &mapping(json!({"offset": 10, "limit": 12}))).unwrap();
    assert_eq!(users(&r), (10..22).map(|i| format!("q{i}")).collect::<Vec<_>>());
    assert_eq!(r.prompts[0].meta, vec![("score".to_string(), json!(10))]);
    let Opened::Table(mut src) = open_path(&f, ImportFormat::Auto, None).unwrap() else { panic!() };
    assert_eq!(src.table.total_rows(), Some(30));
    assert!(!src.table.names().contains(&"blob".to_string()));
    let pv = preview_table(&mut src, &mapping(json!({})), PreviewOptions::default()).unwrap();
    assert_eq!(pv.prompt_field.as_deref(), Some("messages"));
    assert_eq!((pv.total_rows, pv.estimated_prompts, pv.prompts.len()), (Some(30), Some(30), 20));
    assert!(pv.has_conversations);
    assert_eq!(pv.dropped, ["assistant turns in messages"]);
}

#[test]
fn parquet_text_column() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("t.parquet");
    let batch = RecordBatch::try_from_iter(vec![
        ("question", Arc::new(StringArray::from(vec!["a", "b", "c"])) as ArrayRef),
        ("answer", Arc::new(StringArray::from(vec!["1", "2", "3"])) as ArrayRef),
    ])
    .unwrap();
    std::fs::write(&f, parquet_bytes(&[batch], 1024)).unwrap();
    assert_eq!(users(&prompts_of(&f, &mapping(json!({}))).unwrap()), ["a", "b", "c"]);
    assert_eq!(detect_format(&f, None), "parquet");
    let renamed = dir.path().join("t.bin");
    std::fs::copy(&f, &renamed).unwrap();
    assert_eq!(detect_format(&renamed, None), "parquet");
}

#[test]
fn format_sniffing() {
    let dir = tempfile::tempdir().unwrap();
    for (name, content, want) in [
        ("a.dat", "[1, 2]", "json"),
        ("b.dat", "{\"a\": 1}\n{\"a\": 2}\n", "jsonl"),
        ("c.dat", "a,b\n1,2\n3,4\n", "csv"),
        ("d.dat", "a\tb\n1\t2\n", "tsv"),
        ("e.dat", "hello there\n", "text"),
        ("f.NDJSON", "{}", "jsonl"),
        ("g.Markdown", "#", "markdown"),
    ] {
        let p = dir.path().join(name);
        std::fs::write(&p, content).unwrap();
        assert_eq!(detect_format(&p, Some(name)), want, "{name}");
    }
}

#[test]
fn preview_text_and_totals() {
    let doc = stellarator_import::files::TextDoc { text: "a\n\nb\n\nc".into(), kind: "paste", warnings: vec![] };
    let pv = preview_text(&doc, &mapping(json!({})));
    assert_eq!(pv.split_mode, Some(SplitMode::BlankLine));
    assert_eq!((pv.estimated_prompts, pv.prompts.len()), (Some(3), 3));
}

#[test]
fn preview_csv_counts_and_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("w.csv");
    std::fs::write(&f, "prompt\na\na\n\nb\n").unwrap();
    let Opened::Table(mut src) = open_path(&f, ImportFormat::Auto, None).unwrap() else { panic!() };
    let pv = preview_table(&mut src, &mapping(json!({})), PreviewOptions::default()).unwrap();
    assert_eq!((pv.estimated_prompts, pv.total_rows), (Some(2), Some(3)));
    assert!(pv.warnings.iter().any(|w| w.contains("duplicate")));
}

#[test]
fn csv_100k_rows_fast() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("big.csv");
    let mut s = String::from("prompt,idx\n");
    for i in 0..100_000 {
        s.push_str(&format!("Question number {i} about something,{i}\n"));
    }
    std::fs::write(&f, s).unwrap();
    let t0 = Instant::now();
    let r = prompts_of(&f, &mapping(json!({}))).unwrap();
    assert_eq!(r.prompts.len(), 100_000);
    assert!(t0.elapsed().as_secs_f64() < 10.0, "{:?}", t0.elapsed());
}
