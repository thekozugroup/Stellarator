//! Executions in the sandbox against a host with a few fake tools: what the model receives, and
//! what the trace records, in both formats. The byte-for-byte comparison with Cloudflare's package
//! is crates/engine/tests/conformance.rs; these tests cover what it does not: the trace's own
//! record, the limits, progress, and a failing server.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use serde_json::{json, Map, Value};
use stellarator_api::{CodeFormat, CodeStatus, LiveCode};
use stellarator_codemode::{execute, CallAnswer, Execution, Host, Limits, Setup, LATE_CALL_MESSAGE};

struct Fake;

fn text(t: impl Into<String>) -> CallAnswer {
    CallAnswer::Mcp(json!({"content": [{"type": "text", "text": t.into()}]}))
}

impl Host for Fake {
    fn route(&self, namespace: &str, method: &str) -> Option<(String, String)> {
        let known = namespace == "mock" && ["add", "lookup", "fail", "slow", "gone", "big", "huge"].contains(&method);
        known.then(|| ("mock-server".to_string(), method.replace('_', "-")))
    }

    fn call(&self, _namespace: String, method: String, arguments: Option<Map<String, Value>>) -> BoxFuture<'_, CallAnswer> {
        Box::pin(async move {
            let args = arguments.unwrap_or_default();
            let num = |k: &str| args.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            match method.as_str() {
                // The text "5" reaches the code as the number 5, as Cloudflare's MCP unwrapping reads it.
                "add" => text((num("a") + num("b")).to_string()),
                "lookup" => text(format!("Entry for {}", args.get("query").and_then(Value::as_str).unwrap_or("?"))),
                "fail" => CallAnswer::Mcp(json!({"content": [{"type": "text", "text": "The lookup failed: the record is locked."}], "isError": true})),
                "slow" => {
                    tokio::time::sleep(Duration::from_secs_f64(num("seconds"))).await;
                    text("done")
                }
                "big" => CallAnswer::Mcp(json!({"content": [], "structuredContent":
                    {"rows": (0..2000).map(|i| json!({"i": i, "text": "lorem ipsum dolor sit amet"})).collect::<Vec<_>>()}})),
                "huge" => text("0123456789".repeat(100_000)),
                _ => CallAnswer::Fatal("The MCP server \"mock-server\" stopped answering.".into()),
            }
        })
    }
}

fn limits() -> Limits {
    Limits {
        timeout: Duration::from_secs(5),
        memory_bytes: 64 << 20,
        budget: Some((24_000, 6000)),
        logs_budget: Some((24_000, 6000)),
        store_chars: 20_000,
    }
}

fn setup(format: CodeFormat, limits: Limits) -> Setup {
    let descriptions = json!([{"name": "mock", "instructions": "A mock.", "annotations": {}, "descriptors": {
        "add": {"description": "Add two numbers.", "inputSchema": {"type": "object", "properties": {"a": {"type": "number"}, "b": {"type": "number"}}, "required": ["a", "b"]}},
        "lookup": {"description": "Look up a term.", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}}}}
    }}]);
    Setup { format, tool_name: "codemode".into(), namespaces: vec!["mock".into()], descriptions: descriptions.to_string().into(), limits }
}

async fn run_with(code: &str, limits: &Limits) -> Execution {
    let args = json!({ "code": code }).to_string();
    execute(&setup(CodeFormat::CodeTool, limits.clone()), &args, &Fake, &|_: &LiveCode| {}).await.unwrap()
}

async fn run(code: &str) -> Execution {
    run_with(code, &limits()).await
}

/// The runtime format's answer, with the execution id checked and taken out.
async fn runtime(code: &str) -> (Value, Execution) {
    let args = json!({ "code": code }).to_string();
    let limits = Limits { budget: None, ..limits() };
    let e = execute(&setup(CodeFormat::Runtime, limits), &args, &Fake, &|_: &LiveCode| {}).await.unwrap();
    let mut v: Value = serde_json::from_str(&e.content).unwrap();
    let id = v["executionId"].as_str().unwrap().to_string();
    let (millis, uuid) = id.strip_prefix("exec_").unwrap().split_once('_').unwrap();
    assert!(millis.len() == 16 && millis.bytes().all(|b| b.is_ascii_digit()), "{id}");
    assert!(uuid.len() == 36 && uuid.as_bytes()[14] == b'4', "{id}");
    v["executionId"] = json!("exec");
    (v, e)
}

#[tokio::test]
async fn a_result_and_its_logs_go_back_as_json() {
    let e = run(
        "async () => {\n  const s = await mock.add({ a: 2, b: 3 });\n  console.log(\"sum\", s, { x: 1 });\n  console.warn(\"careful\");\n  \
         console.error(\"bad\", [1, 2]);\n  console.info(\"not captured\");\n  return { s, note: \"é \\u2028\" };\n}",
    )
    .await;
    assert_eq!(
        e.content,
        "{\"result\":{\"s\":5,\"note\":\"é \u{2028}\"},\"logs\":[\"sum 5 [object Object]\",\"[warn] careful\",\"[error] bad 1,2\"]}"
    );
    assert!(!e.is_error && !e.truncated);
    let d = e.detail.unwrap();
    assert_eq!(d.status, CodeStatus::Ok);
    assert_eq!(d.result.as_deref(), Some("{\"s\":5,\"note\":\"é \u{2028}\"}"));
    assert_eq!(d.logs, ["sum 5 [object Object]", "[warn] careful", "[error] bad 1,2"]);
    assert_eq!(d.calls.len(), 1);
    let c = &d.calls[0];
    assert_eq!((c.method.as_str(), c.server.as_str(), c.tool.as_str()), ("mock.add", "mock-server", "add"));
    assert_eq!((c.arguments.clone(), c.result.as_str(), c.is_error), (json!({"a": 2, "b": 3}), "5", false));
}

#[tokio::test]
async fn fenced_code_is_normalized_and_its_last_expression_returned() {
    let e = run("```js\nconst r = await mock.lookup({ query: \"tides\" });\nr\n```").await;
    assert_eq!(e.content, r#"{"result":"Entry for tides"}"#);
    assert_eq!(e.detail.unwrap().code, "async () => {\nconst r = await mock.lookup({ query: \"tides\" });\nreturn (r)\n}");
    // Nothing returned: an empty object.
    assert_eq!(run("async () => { await mock.add({ a: 1, b: 1 }); }").await.content, "{}");
}

#[tokio::test]
async fn a_tool_error_throws_in_the_code_and_can_be_caught() {
    let e = run(
        "async () => {\n  try {\n    await mock.fail({ reason: \"x\" });\n  } catch (e) {\n    console.log(\"caught: \" + e.message);\n  }\n  \
         return await mock.add({ a: 1, b: 2 });\n}",
    )
    .await;
    assert_eq!(e.content, r#"{"result":3,"logs":["caught: The lookup failed: the record is locked."]}"#);
    let d = e.detail.unwrap();
    assert!(d.calls[0].is_error && !d.calls[1].is_error);
    assert_eq!(d.calls[0].result, "The lookup failed: the record is locked.");
}

#[tokio::test]
async fn an_uncaught_error_is_the_tools_error_text() {
    let e = run("async () => { console.log(\"before\"); return await mock.fail({}); }").await;
    assert_eq!(e.content, "Error: Code execution failed: The lookup failed: the record is locked.\n\nConsole output:\nbefore");
    assert!(e.is_error);
    let d = e.detail.unwrap();
    assert_eq!((d.status, d.error.as_deref()), (CodeStatus::Error, Some("The lookup failed: the record is locked.")));
    // An unknown method throws as Cloudflare's dispatcher does; an unknown namespace is a ReferenceError.
    let e = run("async () => await mock.nope({})").await;
    assert_eq!(e.content, "Error: Code execution failed: Tool \"nope\" not found");
    assert_eq!(e.detail.unwrap().calls[0].server, "");
    assert_eq!(run("async () => await github.search({})").await.content, "Error: Code execution failed: github is not defined");
    // The engine's errors read as V8's where V8's wording is fixed.
    assert_eq!(
        run("async () => { const rows = undefined; return rows.length; }").await.content,
        "Error: Code execution failed: Cannot read properties of undefined (reading 'length')"
    );
    // fetch exists and refuses, as a Worker without outbound access.
    let e = run("async () => await fetch(\"https://example.com\")").await;
    assert!(e.content.starts_with("Error: Code execution failed: This worker is not permitted to access the internet"), "{}", e.content);
}

#[tokio::test]
async fn a_syntax_error_is_the_executors_error() {
    let e = run("async () => { const = 1; }").await;
    assert!(e.content.starts_with("Error: Failed to start Worker:\nUncaught SyntaxError: "), "{}", e.content);
    // One provider: the code is line 68 of the module, which workerd calls line 70.
    assert!(e.content.ends_with("\n  at executor.js:70:20"), "{}", e.content);
    assert!(e.is_error);
    let d = e.detail.unwrap();
    assert_eq!(d.status, CodeStatus::SyntaxError);
    assert!(d.calls.is_empty());
}

#[tokio::test]
async fn an_infinite_loop_times_out() {
    let l = Limits { timeout: Duration::from_millis(500), ..limits() };
    let started = Instant::now();
    let e = run_with("async () => { console.log(\"start\"); while (true) {} }", &l).await;
    assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    assert_eq!(e.content, "Error: Code execution failed: Execution timed out");
    assert_eq!(e.detail.unwrap().status, CodeStatus::Timeout);
    // A catch does not stop the interrupt either.
    let e = run_with("async () => { try { for (;;) {} } catch (e) { return \"caught\"; } }", &l).await;
    assert_eq!(e.detail.unwrap().status, CodeStatus::Timeout);
    // Waiting on a slow tool times out through the module's own race, which keeps the console output.
    let e = run_with("async () => { console.log(\"waiting\"); await mock.slow({ seconds: 3 }); }", &l).await;
    assert_eq!(e.content, "Error: Code execution failed: Execution timed out\n\nConsole output:\nwaiting");
    let d = e.detail.unwrap();
    assert_eq!(d.status, CodeStatus::Timeout);
    assert_eq!((d.calls[0].result.as_str(), d.calls[0].is_error), (LATE_CALL_MESSAGE, true));
}

#[tokio::test]
async fn the_memory_limit_stops_the_code() {
    let l = Limits { memory_bytes: 16 << 20, ..limits() };
    let e = run_with("async () => { const a = []; for (;;) a.push(\"x\".repeat(1000) + a.length); }", &l).await;
    let d = e.detail.unwrap();
    assert_eq!(d.status, CodeStatus::MemoryLimit, "{}", e.content);
    assert!(e.content.starts_with("Error: Code execution failed: ") && e.content.contains("out of memory"), "{}", e.content);
}

#[tokio::test]
async fn process_exit_ends_the_execution() {
    let e = run("async () => { try { process.exit(3); } finally { console.log(\"not reached\"); } }").await;
    assert_eq!(e.content, "Error: The Node.js process.exit(3) API was called. Canceling the request.");
    assert!(e.is_error);
}

#[tokio::test]
async fn parallel_calls_timers_and_unawaited_calls() {
    let e = run(
        "async () => {\n  const sums = await Promise.all([1, 2, 3].map((n) => mock.add({ a: n, b: n })));\n  \
         await new Promise((r) => setTimeout(r, 20));\n  mock.slow({ seconds: 1 });\n  return sums;\n}",
    )
    .await;
    assert_eq!(e.content, r#"{"result":[2,4,6]}"#);
    let d = e.detail.unwrap();
    assert_eq!(d.calls.len(), 4);
    assert_eq!(d.calls[3].method, "mock.slow");
    assert_eq!(d.calls[3].result, LATE_CALL_MESSAGE);
}

#[tokio::test]
async fn large_results_are_shrunk_as_truncate_result_does() {
    let e = run("async () => await mock.big({})").await;
    assert!(e.truncated && !e.is_error);
    assert!(e.content.len() < 25_000, "{}", e.content.len());
    assert!(e.content.contains("--- TRUNCATED ---"));
    let d = e.detail.unwrap();
    assert!(d.result.unwrap().len() > 60_000);
    assert!(d.calls[0].truncated && d.calls[0].result.ends_with("more characters]"));
    // No budget: createCodeTool alone.
    let none = Limits { budget: None, logs_budget: None, ..limits() };
    let e = run_with("async () => await mock.big({})", &none).await;
    assert!(!e.truncated && e.content.len() > 60_000);
    // The console output of an error is never cut: it is part of the error's message.
    let e = run("async () => { for (let i = 0; i < 2000; i++) console.log(\"a line of console output\", i); throw new Error(\"after\"); }").await;
    assert!(e.is_error && !e.truncated && e.content.len() > 50_000, "{}", e.content.len());
}

#[tokio::test]
async fn inputs_that_are_not_code_get_the_ai_sdks_error() {
    let live = Mutex::new(Vec::new());
    let code_tool = setup(CodeFormat::CodeTool, limits());
    let e = execute(&code_tool, r#"{"code": 5}"#, &Fake, &|l: &LiveCode| live.lock().unwrap().push(l.clone())).await.unwrap();
    assert!(e.is_error && e.detail.is_none());
    assert_eq!(
        e.content,
        "AI_InvalidToolInputError: Invalid input for tool codemode: AI_TypeValidationError: Type validation failed: Value: {\"code\":5}.\n\
         Error message: [\n  {\n    \"expected\": \"string\",\n    \"code\": \"invalid_type\",\n    \"path\": [\n      \"code\"\n    ],\n    \
         \"message\": \"Invalid input: expected string, received number\"\n  }\n]"
    );
    // Arguments that are not JSON: the SyntaxError is worded as V8 words it.
    let e = execute(&code_tool, r#"{"code": "unterminated"#, &Fake, &|_: &LiveCode| {}).await.unwrap();
    assert_eq!(
        e.content,
        "AI_InvalidToolInputError: Invalid input for tool codemode: AI_JSONParseError: JSON parsing failed: Text: {\"code\": \"unterminated.\n\
         Error message: SyntaxError: Unterminated string in JSON at position 22 (line 1 column 23)"
    );
    assert!(live.lock().unwrap().is_empty());
    // The runtime's tool checks with its own schema.
    let e = execute(&setup(CodeFormat::Runtime, limits()), r#"{"code": 5}"#, &Fake, &|_: &LiveCode| {}).await.unwrap();
    assert_eq!(
        e.content,
        "AI_InvalidToolInputError: Invalid input for tool codemode: AI_TypeValidationError: Type validation failed: Value: {\"code\":5}.\n\
         Error message: [{\"message\":\"Expected an object with a string code property\"}]"
    );
}

#[tokio::test]
async fn progress_reports_running_calls() {
    let seen = Mutex::new(Vec::new());
    let args = json!({"code": "async () => { await mock.slow({ seconds: 0.2 }); return await mock.add({ a: 1, b: 1 }); }"}).to_string();
    execute(&setup(CodeFormat::CodeTool, limits()), &args, &Fake, &|l: &LiveCode| seen.lock().unwrap().push(l.clone())).await.unwrap();
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.first().unwrap(), &LiveCode { calls: 1, running: vec!["mock.slow".into()] });
    assert_eq!(seen.last().unwrap(), &LiveCode { calls: 2, running: vec![] });
}

#[tokio::test]
async fn a_server_that_is_gone_fails_the_sample() {
    let args = json!({"code": "async () => await mock.gone({})"}).to_string();
    let err = execute(&setup(CodeFormat::CodeTool, limits()), &args, &Fake, &|_: &LiveCode| {}).await.unwrap_err();
    assert!(err.contains("stopped answering"), "{err}");
}

#[tokio::test]
async fn quirks_kept_from_cloudflares_executor() {
    // A thrown value without a message is not an error there: the result is undefined.
    assert_eq!(run("async () => { throw \"plain\"; }").await.content, "{}");
    // Executions do not share state.
    run("async () => { globalThis.leak = 1; }").await;
    assert_eq!(run("async () => typeof leak").await.content, r#"{"result":"undefined"}"#);
    // What cannot cross Workers RPC fails the executor, not the code.
    let e = run("async () => { class Point { x = 1; } return new Point(); }").await;
    assert_eq!(e.content, "DataCloneError: Could not serialize object of type \"Point\". This type does not support serialization.");
    assert_eq!(e.detail.unwrap().status, CodeStatus::Error);
}

#[tokio::test]
async fn the_runtime_format_finds_tools_in_the_sandbox() {
    let (v, e) = runtime(
        "async () => {\n  const matches = await codemode.search(\"add numbers\");\n  const docs = await codemode.describe(matches.results[0].path);\n  \
         const sum = await mock.add({ a: 4, b: 5 });\n  console.log(\"sum\", sum);\n  return { path: matches.results[0].path, types: docs.types, sum };\n}",
    )
    .await;
    assert_eq!(
        v,
        json!({"status": "completed", "executionId": "exec", "logs": ["sum 9"], "result": {
            "path": "mock.add", "types": "type AddInput = {\n    a: number;\n    b: number;\n}\ntype AddOutput = unknown", "sum": 9}})
    );
    assert!(!e.is_error);
    // Only the MCP call is a tool call of the trace; search and describe are the runtime's own.
    let d = e.detail.unwrap();
    assert_eq!((d.status, d.calls.len(), d.calls[0].method.as_str()), (CodeStatus::Ok, 1, "mock.add"));

    // A connector's description carries the server's instructions; an unknown target says so.
    let (v, _) = runtime("async () => [await codemode.describe(\"mock\"), await codemode.describe(\"nope\")]").await;
    assert_eq!(v["result"][0]["description"], "A mock.");
    assert!(v["result"][0]["types"].as_str().unwrap().starts_with("A mock.\n\ntype AddInput = {"));
    assert_eq!(v["result"][1], json!({"path": "nope", "types": "\"nope\" not found.", "kind": "method"}));

    // Steps run, no snippet is saved, and an unknown method names its connector.
    let (v, _) = runtime("async () => [await codemode.step(\"n\", () => 41 + 1), await codemode.run(\"weekly\")]").await;
    assert_eq!(v["result"], json!([42, {"error": "Snippet \"weekly\" not found."}]));
    let (v, e) = runtime("async () => mock.nope({})").await;
    assert_eq!(v, json!({"status": "error", "executionId": "exec", "error": "Code execution failed: Tool \"nope\" not found on mock"}));
    assert!(e.is_error);
    assert_eq!(e.detail.unwrap().error.as_deref(), Some("Code execution failed: Tool \"nope\" not found on mock"));

    // A global the code invented: the runtime names the ones that exist.
    let (v, _) = runtime("async () => host.readFile(\"x\")").await;
    assert_eq!(
        v["error"],
        "Code execution failed: host is not defined (the only globals available in the sandbox are: mock, codemode)"
    );
}

#[tokio::test]
async fn the_runtime_format_records_what_the_durable_log_would() {
    // The result is not truncated without a budget; the console output always is.
    let (v, e) = runtime("async () => { for (let i = 0; i < 2000; i++) console.log(\"a line of console output\", i); return \"x\".repeat(30000); }").await;
    assert_eq!(v["result"].as_str().unwrap().len(), 30_000);
    assert!(v["logs"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().contains("--- TRUNCATED ---")));
    assert!(e.truncated);
    // A tool result over a million characters cannot be recorded: the execution fails, as the runtime's does.
    let (v, _) = runtime("async () => (await mock.huge({})).length").await;
    assert_eq!(
        v,
        json!({"status": "error", "executionId": "exec", "error": "The result of mock.huge is too large to record durably (1000002 bytes > \
            1000000 byte limit). Write large data to a file or workspace instead and pass/return a small reference (such as a path)."})
    );
}
