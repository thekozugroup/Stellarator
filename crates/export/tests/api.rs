mod common;

use common::*;
use serde_json::{json, Value};
use stellarator_export::records::strip_none;

fn lines(r: &Resp) -> Vec<Value> {
    r.text().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn csv_rows(text: &str) -> Vec<Vec<String>> {
    // A small RFC 4180 reader that handles quoted newlines, enough to check the writer.
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == ',' {
            row.push(std::mem::take(&mut field));
        } else if c == '\n' {
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
        } else {
            field.push(c);
        }
    }
    rows
}

#[tokio::test]
async fn preview_counts() {
    let e = env().await;
    let s = seed(&e.ctx.db).await;
    let app = e.app();
    let r = post(&app, &format!("/api/runs/{}/export/preview", s.run), json!({})).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let b = r.json();
    assert_eq!(b["count"], 3); // q0 s0, q0 s1, q2 (truncated excluded)
    assert_eq!(b["skipped"], json!({"failed": 1, "pending": 1, "truncated": 1, "empty_response": 1}));
    assert_eq!(b["filename"], "qwen3-32b-on-gsm8k-messages.jsonl");
    let msgs = &b["example"]["messages"];
    assert_eq!(msgs[1]["content"], "Solve: q0");
    assert_eq!(msgs[2], json!({"role": "assistant", "content": "<think>\nthink 0-0\n</think>\n\nanswer 0-0 é"}));

    let r = post(
        &app,
        &format!("/api/runs/{}/export/preview", s.run),
        json!({"exclude_truncated": false, "require_reasoning": true, "first_sample_only": true, "format": "alpaca"}),
    )
    .await;
    let b = r.json();
    assert_eq!(b["count"], 2); // q0 s0, q1 (length kept)
    assert_eq!(
        b["skipped"],
        json!({"failed": 1, "pending": 1, "no_reasoning": 1, "extra_samples": 1, "empty_response": 1})
    );
    assert_eq!(b["example"]["instruction"], "Solve: q0");
}

#[tokio::test]
async fn running_counts_as_pending() {
    let e = env().await;
    let s = seed(&e.ctx.db).await;
    sqlx::query("UPDATE samples SET status = 'running' WHERE id = ?").bind(s.samples[5]).execute(&e.ctx.db).await.unwrap();
    let r = post(&e.app(), &format!("/api/runs/{}/export/preview", s.run), json!({})).await;
    assert_eq!(r.json()["skipped"]["pending"], 1);
}

#[tokio::test]
async fn download_jsonl_exact_bytes() {
    let e = env().await;
    let s = seed(&e.ctx.db).await;
    let r = get(&e.app(), &format!("/api/runs/{}/export?format=messages&reasoning=reasoning_content", s.run)).await;
    assert_eq!(r.status, 200);
    assert!(r.header("content-type").starts_with("application/x-ndjson"));
    assert_eq!(r.header("content-disposition"), "attachment; filename=\"qwen3-32b-on-gsm8k-messages.jsonl\"");
    let text = r.text();
    // Python's json.dumps(ensure_ascii=False) layout, one record per line.
    let first = text.lines().next().unwrap();
    assert_eq!(
        first,
        r#"{"messages": [{"role": "system", "content": "sys"}, {"role": "user", "content": "Solve: q0"}, {"role": "assistant", "content": "answer 0-0 é", "reasoning_content": "think 0-0"}]}"#
    );
    assert!(text.ends_with('\n'));
    let recs = lines(&r);
    assert_eq!(recs.len(), 3);
    let users: Vec<&str> = recs.iter().map(|x| x["messages"][1]["content"].as_str().unwrap()).collect();
    assert_eq!(users, ["Solve: q0", "Solve: q0", "Solve: q2"]);
    assert!(recs[2]["messages"][2]["reasoning_content"].is_null());
    assert!(text.contains('é') && !text.contains("\\u"));
}

#[tokio::test]
async fn download_types() {
    let e = env().await;
    let s = seed(&e.ctx.db).await;
    let app = e.app();
    let base = format!("/api/runs/{}/export", s.run);

    let r = get(&app, &format!("{base}?file_type=json&format=sharegpt")).await;
    assert!(r.header("content-type").starts_with("application/json"));
    assert!(r.header("content-disposition").ends_with("sharegpt.json\""));
    assert_eq!(r.json().as_array().unwrap().len(), 3);
    assert!(r.text().starts_with("[\n{") && r.text().ends_with("}\n]\n"));

    let r = get(&app, &format!("{base}?file_type=csv&format=raw&exclude_truncated=false")).await;
    assert_eq!(r.header("content-type"), "text/csv; charset=utf-8");
    let rows = csv_rows(&r.text());
    assert_eq!(rows.len(), 5); // header + 4
    let ix = rows[0].iter().position(|h| h == "params").unwrap();
    assert_eq!(serde_json::from_str::<Value>(&rows[1][ix]).unwrap()["temperature"], 0.7);
    let ix = rows[0].iter().position(|h| h == "usage").unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&rows[1][ix]).unwrap(),
        json!({"prompt_tokens": 5, "completion_tokens": 9, "reasoning_tokens": 3})
    );

    let r = get(&app, &format!("{base}?file_type=parquet&format=prompt_completion")).await;
    assert_eq!(r.header("content-type"), "application/vnd.apache.parquet");
    assert!(r.header("content-disposition").ends_with("prompt_completion.parquet\""));
    let rows = read_parquet(&r.body);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["completion"][0]["role"], "assistant");
    assert!(e.leftovers().is_empty(), "{:?}", e.leftovers());

    assert_eq!(get(&app, &format!("{base}?format=nope")).await.status, 422);
}

#[tokio::test]
async fn small_batches_give_the_same_records() {
    use stellarator_export::records::RunInfo;
    use stellarator_export::source::{spawn, Source};
    let e = env().await;
    let s = seed(&e.ctx.db).await;
    let run = RunInfo {
        id: s.run,
        name: "x".into(),
        endpoint_name: "spark".into(),
        model: "m".into(),
        prompt_set_id: s.ps,
        params: json!({}),
        tools: None,
    };
    let options = serde_json::from_value(json!({})).unwrap();
    let mut all = vec![];
    for batch in [1, 2, 500] {
        let mut rx = spawn(e.ctx.db.clone(), Source::Sft { run: run.clone().into(), options: serde_json::from_value(json!({})).unwrap() }, batch);
        let mut got = vec![];
        while let Some(b) = rx.recv().await {
            got.extend(b.unwrap());
        }
        all.push(got);
    }
    let _: &stellarator_core::schemas::ExportOptions = &options;
    assert_eq!(all[0].len(), 3);
    assert_eq!(all[0], all[1]);
    assert_eq!(all[1], all[2]);
}

#[tokio::test]
async fn missing_things_are_404() {
    let e = env().await;
    let app = e.app();
    for (m, u, body) in [
        ("POST", "/api/runs/99/export/preview", Some(json!({}))),
        ("GET", "/api/runs/99/export", None),
        ("POST", "/api/preference-sets/99/export/preview", Some(json!({}))),
        ("GET", "/api/preference-sets/99/export", None),
        ("POST", "/api/runs/99/export/hub", Some(json!({"target": {"repo_id": "a/b"}}))),
        ("POST", "/api/preference-sets/99/export/hub", Some(json!({"target": {"repo_id": "a/b"}}))),
    ] {
        let r = send(&app, m, u, body).await;
        assert_eq!(r.status, 404, "{u}");
    }
    assert_eq!(post(&app, "/api/runs/99/export/preview", json!({})).await.json()["detail"], "Run not found");
}

#[tokio::test]
async fn preference_preview_and_download() {
    let e = env().await;
    let s = seed(&e.ctx.db).await;
    let pref = seed_pref(&e.ctx.db, &s).await;
    let app = e.app();
    let r = post(&app, &format!("/api/preference-sets/{pref}/export/preview"), json!({})).await;
    let b = r.json();
    assert_eq!(b["count"], 2);
    assert_eq!(b["filename"], "gsm8k-prefs-dpo.jsonl");
    assert_eq!(b["skipped"], json!({}));
    let keys: Vec<&String> = b["example"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["prompt", "chosen", "rejected"]);
    assert_eq!(b["example"]["prompt"][1]["content"], "Solve: q0"); // run params applied

    let r = post(&app, &format!("/api/preference-sets/{pref}/export/preview"), json!({"format": "kto", "conversational": false})).await;
    let b = r.json();
    assert_eq!(b["count"], 4);
    assert_eq!(b["example"]["label"], true);
    assert_eq!(b["example"]["prompt"], "sys\n\nSolve: q0");

    let r = get(&app, &format!("/api/preference-sets/{pref}/export?conversational=false")).await;
    let recs = lines(&r);
    assert_eq!(recs.len(), 2);
    assert!(recs[0]["chosen"].is_string());
    assert_eq!(r.header("content-disposition"), "attachment; filename=\"gsm8k-prefs-dpo.jsonl\"");

    let r = get(&app, &format!("/api/preference-sets/{pref}/export?format=kto&file_type=parquet")).await;
    let labels: Vec<Value> = read_parquet(&r.body).iter().map(|x| x["label"].clone()).collect();
    assert_eq!(labels, [json!(true), json!(false), json!(true), json!(false)]);
    assert!(e.leftovers().is_empty());
}

// ------------------------------------------------------------------------------ writers

fn many(o: &Value) -> Vec<Value> {
    use stellarator_export::records::{sft_record, RunInfo, SampleRec};
    let options = serde_json::from_value(o.clone()).unwrap();
    let run = RunInfo {
        id: 7,
        name: "r".into(),
        endpoint_name: "spark".into(),
        model: "qwen3-32b".into(),
        prompt_set_id: 1,
        params: json!({"temperature": 0.7, "stop": ["a", "b"]}),
        tools: None,
    };
    let prompt = json!([{"role": "system", "content": "Be brief."}, {"role": "user", "content": "What is 2+2? é"}]);
    [(Some("why\nbecause \"quotes\", commas"), "line1\nline2 é"), (None, "x")]
        .iter()
        .enumerate()
        .map(|(i, (r, resp))| {
            let s = SampleRec {
                id: i as i64,
                run_id: 7,
                prompt_id: 3,
                prompt_idx: 2,
                sample_idx: i as i64,
                reasoning: r.map(String::from),
                response: Some(resp.to_string()),
                finish_reason: Some("stop".into()),
                prompt_tokens: Some(1),
                latency_ms: Some(100),
                completed_at: chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap().and_hms_opt(3, 4, 5),
                ..Default::default()
            };
            sft_record(&options, &run, &s, &prompt, Some(&json!({"k": 1})))
        })
        .collect()
}

fn channel(records: Vec<Value>) -> tokio::sync::mpsc::Receiver<stellarator_export::source::Batch> {
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tx.try_send(Ok(records)).unwrap();
    rx
}

async fn collect(s: stellarator_export::files::ByteStream) -> Vec<u8> {
    use futures::StreamExt;
    let chunks: Vec<_> = s.collect().await;
    chunks.into_iter().flat_map(|c| c.unwrap().to_vec()).collect()
}

#[tokio::test]
async fn jsonl_valid_non_ascii() {
    let recs = many(&json!({}));
    let data = String::from_utf8(collect(stellarator_export::files::jsonl(channel(recs.clone()))).await).unwrap();
    let ls: Vec<&str> = data.lines().collect();
    assert_eq!(ls.len(), 2);
    for (l, r) in ls.iter().zip(&recs) {
        assert_eq!(&serde_json::from_str::<Value>(l).unwrap(), r);
    }
    assert!(data.contains('é') && !data.contains("\\u"));
}

#[tokio::test]
async fn json_array_and_empty() {
    use stellarator_export::files::json_array;
    let recs = many(&json!({}));
    let data = collect(json_array(channel(recs.clone()))).await;
    assert_eq!(serde_json::from_slice::<Vec<Value>>(&data).unwrap(), recs);
    let data = collect(json_array(channel(vec![]))).await;
    assert_eq!(String::from_utf8(data).unwrap(), "[\n]\n");
    assert_eq!(serde_json::from_slice::<Vec<Value>>(b"[\n]\n").unwrap().len(), 0);
}

#[tokio::test]
async fn csv_quoting_and_nested() {
    use stellarator_export::{files, schema};
    let o = json!({"format": "alpaca", "reasoning": "reasoning_content"});
    let options = serde_json::from_value(o.clone()).unwrap();
    let data = String::from_utf8(collect(files::csv(channel(many(&o)), &schema::sft_schema(&options))).await).unwrap();
    let rows = csv_rows(&data);
    assert_eq!(rows.len(), 3);
    let h = &rows[0];
    let col = |name: &str| h.iter().position(|x| x == name).unwrap();
    assert_eq!(rows[1][col("output")], "line1\nline2 é");
    assert!(rows[1][col("reasoning")].starts_with("why\nbecause \"quotes\""));
    assert_eq!(rows[2][col("reasoning")], "");
    assert_eq!(rows[1][col("system")], "Be brief.");
    // exact quoting: embedded quotes doubled, newlines inside quotes
    assert!(data.contains("\"why\nbecause \"\"quotes\"\", commas\""));

    let o2 = json!({});
    let options = serde_json::from_value(o2.clone()).unwrap();
    let data = String::from_utf8(collect(files::csv(channel(many(&o2)), &schema::sft_schema(&options))).await).unwrap();
    let rows = csv_rows(&data);
    let msgs: Value = serde_json::from_str(&rows[1][0]).unwrap();
    assert_eq!(msgs[2]["role"], "assistant");
    assert_eq!(msgs[0]["content"], "Be brief.");
}

#[tokio::test]
async fn parquet_round_trip_every_format() {
    use stellarator_export::{files, schema};
    let dir = tempfile::tempdir().unwrap();
    for fmt in ["messages", "sharegpt", "alpaca", "prompt_completion", "raw"] {
        for placement in ["think_tags", "reasoning_content", "thinking", "omit"] {
            let o = json!({"format": fmt, "reasoning": placement});
            let options = serde_json::from_value(o.clone()).unwrap();
            let recs = many(&o);
            let path = dir.path().join("x.parquet");
            let n = files::write_parquet(&path, schema::sft_schema(&options), channel(recs.clone())).await.unwrap();
            assert_eq!(n, 2);
            let mut back = read_parquet(&std::fs::read(&path).unwrap());
            assert_eq!(back.len(), 2);
            if fmt == "raw" {
                for b in &mut back {
                    for k in ["params", "meta"] {
                        let t = b[k].as_str().unwrap().to_string();
                        b[k] = serde_json::from_str(&t).unwrap();
                    }
                }
            }
            let want: Vec<Value> = recs.iter().map(strip_none).collect();
            let got: Vec<Value> = back.iter().map(strip_none).collect();
            assert_eq!(got, want, "{fmt} {placement}");
        }
    }
}

#[tokio::test]
async fn pref_parquet_round_trip() {
    use stellarator_export::records::{pref_records, SampleRec};
    use stellarator_export::{files, schema};
    let dir = tempfile::tempdir().unwrap();
    let prompt = json!([{"role": "system", "content": "Be brief."}, {"role": "user", "content": "q é"}]);
    let c = SampleRec { id: 1, reasoning: Some("why".into()), response: Some("4".into()), ..Default::default() };
    let r = SampleRec { id: 2, reasoning: None, response: Some("5".into()), ..Default::default() };
    for fmt in ["dpo", "kto"] {
        for conv in [true, false] {
            let o = json!({"format": fmt, "conversational": conv, "reasoning": "thinking"});
            let options = serde_json::from_value(o).unwrap();
            let recs = pref_records(&options, &c, &r, &prompt, &json!({}));
            let path = dir.path().join("p.parquet");
            files::write_parquet(&path, schema::pref_schema(&options), channel(recs.clone())).await.unwrap();
            let back = read_parquet(&std::fs::read(&path).unwrap());
            let want: Vec<Value> = recs.iter().map(strip_none).collect();
            assert_eq!(back.iter().map(strip_none).collect::<Vec<_>>(), want, "{fmt} {conv}");
        }
    }
}

#[tokio::test]
async fn empty_parquet_is_valid() {
    use stellarator_export::{files, schema};
    let dir = tempfile::tempdir().unwrap();
    let options = serde_json::from_value(json!({})).unwrap();
    let path = dir.path().join("e.parquet");
    files::write_parquet(&path, schema::sft_schema(&options), channel(vec![])).await.unwrap();
    let data = std::fs::read(&path).unwrap();
    assert_eq!(&data[..4], b"PAR1");
    assert_eq!(parquet_rows(&data), 0);
}

/// A run with tools and a sample with two parallel calls, for the Parquet round trips.
fn tool_fixture() -> (stellarator_export::records::RunInfo, stellarator_export::records::SampleRec) {
    use stellarator_core::schemas::{ToolSnapshot, TraceMessage};
    use stellarator_export::records::{RunInfo, SampleRec};
    let defs = vec![json!({"type": "function", "function": {"name": "lookup", "description": "Look up.",
        "parameters": {"type": "object", "properties": {"query": {"type": "string"}}}}})];
    let run = RunInfo {
        id: 7,
        name: "r".into(),
        endpoint_name: "spark".into(),
        model: "m".into(),
        prompt_set_id: 1,
        params: json!({}),
        tools: Some(ToolSnapshot { servers: vec![], definitions: defs }),
    };
    let trace: Vec<TraceMessage> = serde_json::from_value(json!([
        {"role": "assistant", "reasoning": "why", "content": "", "finish_reason": "tool_calls", "prompt_tokens": 3,
         "completion_tokens": 2, "reasoning_tokens": 1, "latency_ms": 9,
         "tool_calls": [{"id": "c1", "name": "lookup", "arguments": "{\"query\": \"a\"}"},
                        {"id": "c2", "name": "lookup", "arguments": "{\"query\": \"b\", \"n\": 2}"}]},
        {"role": "tool", "tool_call_id": "c1", "name": "lookup", "server": "docs", "content": "A.", "is_error": false, "truncated": false, "latency_ms": 1},
        {"role": "tool", "tool_call_id": "c2", "name": "lookup", "server": "docs", "content": "B.", "is_error": true, "truncated": false, "latency_ms": 1},
        {"role": "assistant", "reasoning": null, "content": "A and B.", "tool_calls": [], "finish_reason": "stop",
         "prompt_tokens": 9, "completion_tokens": 3, "reasoning_tokens": null, "latency_ms": 9}
    ]))
    .unwrap();
    let sample = SampleRec {
        id: 1,
        run_id: 7,
        prompt_id: 1,
        response: Some("A and B.".into()),
        finish_reason: Some("stop".into()),
        trace: Some(trace),
        tool_calls: 2,
        ..Default::default()
    };
    (run, sample)
}

#[tokio::test]
async fn parquet_round_trip_with_tools() {
    use stellarator_export::records::sft_record;
    use stellarator_export::{files, schema};
    let (run, sample) = tool_fixture();
    let prompt = json!([{"role": "system", "content": "Be brief."}, {"role": "user", "content": "q é"}]);
    let dir = tempfile::tempdir().unwrap();
    for fmt in ["messages", "sharegpt", "raw"] {
        for placement in ["think_tags", "reasoning_content", "thinking", "omit"] {
            let options = serde_json::from_value(json!({"format": fmt, "reasoning": placement, "file_type": "parquet"})).unwrap();
            let recs = vec![sft_record(&options, &run, &sample, &prompt, None)];
            let path = dir.path().join("t.parquet");
            files::write_parquet(&path, schema::sft_schema_for(&options, true), channel(recs.clone())).await.unwrap();
            let mut back = read_parquet(&std::fs::read(&path).unwrap());
            if fmt == "raw" {
                for k in ["params", "meta", "trace", "tools"] {
                    let t = back[0][k].as_str().unwrap().to_string();
                    back[0][k] = serde_json::from_str(&t).unwrap();
                }
            }
            let want: Vec<Value> = recs.iter().map(strip_none).collect();
            assert_eq!(back.iter().map(strip_none).collect::<Vec<_>>(), want, "{fmt} {placement}");
        }
    }
}

#[tokio::test]
async fn pref_parquet_round_trip_with_tools() {
    use stellarator_export::records::{pref_records_with_tools, PairTools};
    use stellarator_export::{files, schema};
    let (run, chosen) = tool_fixture();
    let mut rejected = chosen.clone();
    rejected.id = 2;
    rejected.trace = None;
    let defs = run.tools.clone().unwrap().definitions;
    let prompt = json!([{"role": "user", "content": "q"}]);
    let dir = tempfile::tempdir().unwrap();
    for fmt in ["dpo", "kto"] {
        let o = json!({"format": fmt, "reasoning": "reasoning_content", "file_type": "parquet"});
        let options = serde_json::from_value(o).unwrap();
        let pair = Some(PairTools { chosen: &defs, rejected: &defs });
        let recs = pref_records_with_tools(&options, &chosen, &rejected, &prompt, &json!({}), pair);
        let path = dir.path().join("p.parquet");
        files::write_parquet(&path, schema::pref_schema_for(&options, true), channel(recs.clone())).await.unwrap();
        let back = read_parquet(&std::fs::read(&path).unwrap());
        let want: Vec<Value> = recs.iter().map(strip_none).collect();
        assert_eq!(back.iter().map(strip_none).collect::<Vec<_>>(), want, "{fmt}");
    }
}
