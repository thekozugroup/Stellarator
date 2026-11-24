//! Conformance with Cloudflare's Code Mode.
//!
//! crates/codemode/tests/golden/fixtures holds what an OpenAI-compatible model server received
//! from the published `@cloudflare/codemode` running on workerd, for every case of the reference
//! corpus and both setups (see crates/codemode/tests/golden/README.md). This test runs the same
//! tool calls through Stellarator's engine, with the same MCP servers (the mock's, in-process),
//! and compares, byte for byte:
//!
//! - the request's `tools` array, per setup;
//! - each tool message's content;
//! - each assistant message as it is sent back with the next request.
//!
//! Cases that differ are listed, with their cause, in
//! crates/codemode/tests/golden/known-differences.json. The test fails on a difference that is
//! not listed, and on a listed case that matches (so the list stays the statement of the gap).
//!
//! `CONFORMANCE_REPORT=<file>` writes every difference with both texts, grouped by cause.
//! `CONFORMANCE_ONLY=<text>` runs the cases whose id contains the text (the list is not checked).

mod common;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use common::{env, Env};
use futures::StreamExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use stellarator_core::schemas::{CodeToolSnapshot, RunTools, ToolCall};
use stellarator_core::time;
use stellarator_engine::generation::NoProgress;
use stellarator_engine::tools::{code_assistant_request_message, ToolRuntime};

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../codemode/tests/golden");

fn load(name: &str) -> Value {
    let path = format!("{GOLDEN}/{name}");
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).unwrap()
}

/// `exec_<16 digits>_<uuid>`, the one part of a result that differs between any two runs.
fn mask_execution_ids(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("exec_") {
        let (before, tail) = rest.split_at(at);
        out.push_str(before);
        let id = &tail[..tail.len().min(58)];
        let bytes = id.as_bytes();
        let shaped = id.len() == 58
            && bytes[5..21].iter().all(u8::is_ascii_digit)
            && bytes[21] == b'_'
            && bytes[22..].iter().enumerate().all(|(i, b)| if [8, 13, 18, 23].contains(&i) { *b == b'-' } else { b.is_ascii_hexdigit() && !b.is_ascii_uppercase() });
        if shaped {
            out.push_str("exec_<id>");
            rest = &tail[58..];
        } else {
            out.push_str("exec_");
            rest = &tail[5..];
        }
    }
    out.push_str(rest);
    out
}

async fn mcp_server(e: &Env, name: &str, path: &str) -> i64 {
    let now = time::now_db();
    let url = e.mock.trim_end_matches("/v1").to_string() + path;
    sqlx::query("INSERT INTO mcp_servers (name, transport, url, created_at, updated_at) VALUES (?, 'http', ?, ?, ?)")
        .bind(name)
        .bind(url)
        .bind(&now)
        .bind(&now)
        .execute(&e.db)
        .await
        .unwrap()
        .last_insert_rowid()
}

/// The run settings that reproduce a fixture's setup.
fn run_tools(format: &str, setup: &Value, servers: &HashMap<String, i64>) -> RunTools {
    let mut code = serde_json::Map::new();
    code.insert("format".into(), json!(format));
    if let Some(ms) = setup["timeout_ms"].as_f64() {
        code.insert("timeout_s".into(), json!(ms / 1000.0));
    }
    if format == "code_tool" {
        // createCodeTool sends results as they are.
        code.insert("max_result_tokens".into(), json!(0));
        if let Some(ns) = setup["namespaces"].as_str() {
            code.insert("namespaces".into(), json!(ns));
        }
        if let Some(d) = setup["description"].as_str() {
            code.insert("description".into(), json!(d));
        }
    } else {
        // transformResult: truncateResult(result, options).
        match &setup["transform_result"] {
            Value::Bool(true) => drop(code.insert("max_result_tokens".into(), json!(6000))),
            Value::Object(o) => {
                code.insert("max_result_tokens".into(), json!(o.get("maxTokens").and_then(Value::as_i64).unwrap_or(6000)));
                if let Some(chars) = o.get("maxChars") {
                    code.insert("max_result_chars".into(), chars.clone());
                }
            }
            _ => {}
        }
        if let Some(d) = setup["tool_options"]["description"].as_str() {
            code.insert("description".into(), json!(d));
        }
        if let Some(h) = setup["tool_options"].get("connectorHints") {
            code.insert("connector_hints".into(), h.clone());
        }
    }
    let picked: Vec<Value> = setup["servers"].as_array().unwrap().iter().map(|s| json!({"server_id": servers[s.as_str().unwrap()]})).collect();
    serde_json::from_value(json!({"servers": picked, "mode": "code", "code": code})).unwrap()
}

struct Prepared {
    rt: ToolRuntime,
    code: CodeToolSnapshot,
}

#[derive(Default)]
struct Compared {
    /// (what differs, Cloudflare's text, Stellarator's text)
    differences: Vec<(String, String, String)>,
}

async fn run_case(prepared: &Prepared, case: &Value) -> Compared {
    let mut out = Compared::default();
    let recorded = case["messages"].as_array().cloned().unwrap_or_default();
    let no_messages = case.get("outcome").is_some();
    let mut at = 0usize;
    for (round, step) in case["steps"].as_array().unwrap().iter().enumerate() {
        let calls: Vec<ToolCall> = step["calls"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, c)| ToolCall {
                id: c["id"].as_str().map(str::to_string).unwrap_or_else(|| {
                    if step["calls"].as_array().unwrap().len() > 1 { format!("call_{}_{}", round + 1, i + 1) } else { format!("call_{}", round + 1) }
                }),
                name: c["name"].as_str().unwrap_or("codemode").to_string(),
                arguments: c["arguments"].as_str().unwrap().to_string(),
            })
            .collect();
        let results = match prepared.rt.run_code_calls(&calls, round as i64 + 1, &NoProgress).await {
            Ok(r) => r,
            Err(e) => {
                out.differences.push((format!("round {}", round + 1), "(a tool message)".into(), format!("the sample fails: {e}")));
                return out;
            }
        };
        if no_messages {
            // Cloudflare's side sent the model nothing (the request failed or never returned).
            let ours: Vec<String> = results.iter().map(|t| mask_execution_ids(&t.content)).collect();
            out.differences.push((
                format!("round {}", round + 1),
                format!("no tool message: {} ({})", case["outcome"].as_str().unwrap_or(""), case["detail"].as_str().unwrap_or("")),
                ours.join("\n"),
            ));
            continue;
        }
        // The assistant message as sent back.
        let text = step["content"].as_str().unwrap_or("").to_string();
        let ours = code_assistant_request_message(&prepared.code, &text, &calls).to_string();
        let theirs = recorded.get(at).map(Value::to_string).unwrap_or_default();
        if ours != theirs {
            out.differences.push((format!("assistant message, round {}", round + 1), theirs, ours));
        }
        at += 1;
        for t in &results {
            let Some(theirs) = recorded.get(at) else {
                out.differences.push((format!("tool message {}", t.tool_call_id), "(none)".into(), t.content.clone()));
                continue;
            };
            at += 1;
            let ours = mask_execution_ids(&t.content);
            if theirs["tool_call_id"].as_str() != Some(&t.tool_call_id) {
                out.differences.push(("tool_call_id".into(), theirs["tool_call_id"].to_string(), t.tool_call_id.clone()));
            }
            match theirs["content"].as_str() {
                Some(content) => {
                    let content = mask_execution_ids(content);
                    if content != ours {
                        out.differences.push((format!("tool message {}", t.tool_call_id), content, ours));
                    }
                }
                None => {
                    // A long content, stored as its length, its SHA-256 and both ends.
                    let (length, sha) = (theirs["content_length"].as_u64().unwrap_or(0) as usize, theirs["content_sha256"].as_str().unwrap_or(""));
                    let raw = &t.content;
                    let same_length = raw.encode_utf16().count() == length;
                    // The fixture's hash is of the text with its execution ids masked, as `ours` is.
                    let hash = format!("{:x}", Sha256::digest(ours.as_bytes()));
                    let head: String = raw.chars().take(300).collect();
                    let chars: Vec<char> = raw.chars().collect();
                    let tail: String = chars[chars.len().saturating_sub(300)..].iter().collect();
                    let ends = mask_execution_ids(theirs["content_head"].as_str().unwrap_or("")) == mask_execution_ids(&head)
                        && mask_execution_ids(theirs["content_tail"].as_str().unwrap_or("")) == mask_execution_ids(&tail);
                    if !(same_length && ends && hash == sha) {
                        out.differences.push((
                            format!("tool message {} (long)", t.tool_call_id),
                            format!("{length} characters, sha256 {sha}, starts {}", theirs["content_head"].as_str().unwrap_or("")),
                            format!("{} characters, sha256 {hash}, starts {head}", raw.encode_utf16().count()),
                        ));
                    }
                }
            }
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_sandbox_answers_as_cloudflares_code_mode_does() {
    let e = env().await;
    let mut servers = HashMap::new();
    servers.insert("mock".to_string(), mcp_server(&e, "mock", "/mcp").await);
    servers.insert("tracker".to_string(), mcp_server(&e, "tracker", "/tracker/mcp").await);
    let only = std::env::var("CONFORMANCE_ONLY").ok();
    let known = load("known-differences.json");
    let mut report = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut summary = Vec::new();

    for format in ["code_tool", "runtime"] {
        let fixture = load(&format!("fixtures/{format}.json"));
        // The cause each listed case is under.
        let mut listed: BTreeMap<String, String> = BTreeMap::new();
        for cause in known["causes"].as_array().unwrap() {
            for id in cause[format].as_array().into_iter().flatten() {
                if let Some(first) = listed.insert(id.as_str().unwrap().to_string(), cause["id"].as_str().unwrap().to_string()) {
                    failures.push(format!("{format} {id}: listed under two causes ({first} and {})", cause["id"]));
                }
            }
        }
        let cases: Vec<&Value> =
            fixture["cases"].as_array().unwrap().iter().filter(|c| only.as_deref().is_none_or(|o| c["id"].as_str().unwrap().contains(o))).collect();

        // One open runtime per setup; the tools arrays compared as sent.
        let mut prepared: HashMap<String, Arc<Prepared>> = HashMap::new();
        let mut definition_differences = 0;
        for case in &cases {
            let key = case["setup"].to_string();
            if prepared.contains_key(&key) {
                continue;
            }
            let tools = run_tools(format, &case["setup"], &servers);
            let snapshot = stellarator_engine::routes::tool_snapshot(&e.engine, &tools).await.unwrap_or_else(|err| panic!("{}: {err:?}", case["id"]));
            let theirs = fixture["definitions"][case["definitions"].as_str().unwrap()].to_string();
            let ours = Value::Array(snapshot.definitions.clone()).to_string();
            if theirs != ours && theirs != "null" {
                definition_differences += 1;
                failures.push(format!("{format}: the tools of \"{}\" differ\n  Cloudflare:  {theirs}\n  Stellarator: {ours}", case["definitions"]));
            }
            let code = snapshot.code.clone().expect("a code mode snapshot");
            let rt = ToolRuntime::open(&e.db, snapshot, tools, true).await.unwrap();
            prepared.insert(key, Arc::new(Prepared { rt, code }));
        }

        let results: Vec<(String, Compared)> = futures::stream::iter(cases.iter().map(|case| {
            let prepared = prepared[&case["setup"].to_string()].clone();
            async move { (case["id"].as_str().unwrap().to_string(), run_case(&prepared, case).await) }
        }))
        .buffer_unordered(8)
        .collect()
        .await;
        for p in prepared.values() {
            p.rt.close().await;
        }

        let (mut matching, mut differing) = (0, 0);
        for (id, compared) in &results {
            let cause = listed.get(id);
            if compared.differences.is_empty() {
                matching += 1;
                if let Some(cause) = cause.filter(|_| only.is_none()) {
                    failures.push(format!("{format} {id}: listed under \"{cause}\" in known-differences.json, but it matches now. Remove it from the list."));
                }
                continue;
            }
            differing += 1;
            for (what, theirs, ours) in &compared.differences {
                report.push(json!({"format": format, "case": id, "cause": cause, "what": what, "cloudflare": theirs, "stellarator": ours}));
                if cause.is_none() {
                    let short = |s: &str| if s.chars().count() > 600 { format!("{}…", s.chars().take(600).collect::<String>()) } else { s.to_string() };
                    failures.push(format!("{format} {id}: {what} differs, and the case is not in known-differences.json\n  Cloudflare:  {}\n  Stellarator: {}", short(theirs), short(ours)));
                }
            }
        }
        if only.is_none() {
            let ids: Vec<&str> = results.iter().map(|(id, _)| id.as_str()).collect();
            for id in listed.keys().filter(|id| !ids.contains(&id.as_str())) {
                failures.push(format!("{format} {id}: listed in known-differences.json, but the fixtures have no such case"));
            }
        }
        summary.push(format!("{format}: {} cases, {matching} match byte for byte, {differing} differ; {} setups, {definition_differences} tools arrays differ", results.len(), prepared.len()));
    }

    if let Ok(path) = std::env::var("CONFORMANCE_REPORT") {
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
    for line in &summary {
        println!("{line}");
    }
    assert!(failures.is_empty(), "{} problems:\n\n{}\n\n{}", failures.len(), failures.join("\n\n"), summary.join("\n"));
}
