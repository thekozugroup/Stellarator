//! Executions in the sandbox against a host with a few fake tools: what the model receives, and
//! what the trace records.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use serde_json::{json, Map, Value};
use stellarator_api::{CodeStatus, LiveCode};
use stellarator_codemode::{execute, CallAnswer, Execution, Host, Limits, LATE_CALL_MESSAGE};

struct Fake;

impl Host for Fake {
    fn route(&self, namespace: &str, method: &str) -> Option<(String, String)> {
        let known = namespace == "mock" && ["add", "lookup", "fail", "slow", "gone", "big"].contains(&method);
        known.then(|| ("mock-server".to_string(), method.replace('_', "-")))
    }

    fn call(&self, _namespace: String, method: String, arguments: Option<Map<String, Value>>) -> BoxFuture<'_, CallAnswer> {
        Box::pin(async move {
            let args = arguments.unwrap_or_default();
            let num = |k: &str| args.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            match method.as_str() {
                "add" => CallAnswer::Value(json!(num("a") + num("b"))),
                "lookup" => CallAnswer::Value(json!(format!("Entry for {}", args.get("query").and_then(Value::as_str).unwrap_or("?")))),
                "fail" => CallAnswer::Thrown("The lookup failed: the record is locked.".into()),
                "slow" => {
                    tokio::time::sleep(Duration::from_secs_f64(num("seconds"))).await;
                    CallAnswer::Value(json!("done"))
                }
                "big" => CallAnswer::Value(json!({"rows": (0..2000).map(|i| json!({"i": i, "text": "lorem ipsum dolor sit amet"})).collect::<Vec<_>>()})),
                _ => CallAnswer::Fatal("The MCP server \"mock-server\" stopped answering.".into()),
            }
        })
    }
}

fn limits() -> Limits {
    Limits { timeout: Duration::from_secs(5), memory_bytes: 64 << 20, budget: Some((24_000, 6000)), store_chars: 20_000 }
}

async fn run_with(code: &str, limits: &Limits) -> Execution {
    let args = json!({ "code": code }).to_string();
    execute("codemode", &args, &["mock".to_string()], limits, &Fake, &|_: &LiveCode| {}).await.unwrap()
}

async fn run(code: &str) -> Execution {
    run_with(code, &limits()).await
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
    let e = run("async () => await github.search({})").await;
    assert!(e.content.starts_with("Error: Code execution failed: ") && e.content.contains("github"), "{}", e.content);
    // fetch does not exist.
    let e = run("async () => await fetch(\"https://example.com\")").await;
    assert!(e.content.contains("fetch"), "{}", e.content);
}

#[tokio::test]
async fn a_syntax_error_is_the_executors_error() {
    let e = run("async () => { const = 1; }").await;
    assert!(e.content.starts_with("SyntaxError: "), "{}", e.content);
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
    assert_eq!(e.content, "Error: Code execution failed: Execution timed out\n\nConsole output:\nstart");
    assert_eq!(e.detail.unwrap().status, CodeStatus::Timeout);
    // A catch does not stop the interrupt either.
    let e = run_with("async () => { try { for (;;) {} } catch (e) { return \"caught\"; } }", &l).await;
    assert_eq!(e.detail.unwrap().status, CodeStatus::Timeout);
    // Waiting on a slow tool times out through the harness's own race.
    let e = run_with("async () => await mock.slow({ seconds: 3 })", &l).await;
    assert_eq!(e.content, "Error: Code execution failed: Execution timed out");
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
    let e = run_with("async () => await mock.big({})", &Limits { budget: None, ..limits() }).await;
    assert!(!e.truncated && e.content.len() > 60_000);
}

#[tokio::test]
async fn inputs_that_are_not_code_get_the_ai_sdks_error() {
    let live = Mutex::new(Vec::new());
    let e = execute("codemode", r#"{"code": 5}"#, &["mock".into()], &limits(), &Fake, &|l: &LiveCode| live.lock().unwrap().push(l.clone()))
        .await
        .unwrap();
    assert!(e.is_error && e.detail.is_none());
    assert_eq!(
        e.content,
        "AI_InvalidToolInputError: Invalid input for tool codemode: AI_TypeValidationError: Type validation failed: Value: {\"code\":5}.\n\
         Error message: [\n  {\n    \"expected\": \"string\",\n    \"code\": \"invalid_type\",\n    \"path\": [\n      \"code\"\n    ],\n    \
         \"message\": \"Invalid input: expected string, received number\"\n  }\n]"
    );
    let e = execute("codemode", r#"{"code": "unterminated"#, &["mock".into()], &limits(), &Fake, &|_: &LiveCode| {}).await.unwrap();
    assert!(e.content.starts_with("AI_InvalidToolInputError: Invalid input for tool codemode: AI_JSONParseError: JSON parsing failed: Text: {\"code\": \"unterminated.\nError message: SyntaxError: "), "{}", e.content);
    assert!(live.lock().unwrap().is_empty());
}

#[tokio::test]
async fn progress_reports_running_calls() {
    let seen = Mutex::new(Vec::new());
    let args = json!({"code": "async () => { await mock.slow({ seconds: 0.2 }); return await mock.add({ a: 1, b: 1 }); }"}).to_string();
    execute("codemode", &args, &["mock".into()], &limits(), &Fake, &|l: &LiveCode| seen.lock().unwrap().push(l.clone())).await.unwrap();
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.first().unwrap(), &LiveCode { calls: 1, running: vec!["mock.slow".into()] });
    assert_eq!(seen.last().unwrap(), &LiveCode { calls: 2, running: vec![] });
}

#[tokio::test]
async fn a_server_that_is_gone_fails_the_sample() {
    let args = json!({"code": "async () => await mock.gone({})"}).to_string();
    let err = execute("codemode", &args, &["mock".into()], &limits(), &Fake, &|_: &LiveCode| {}).await.unwrap_err();
    assert!(err.contains("stopped answering"), "{err}");
}

#[tokio::test]
async fn quirks_kept_from_cloudflares_executor() {
    // A thrown value without a message is not an error there: the result is undefined.
    assert_eq!(run("async () => { throw \"plain\"; }").await.content, "{}");
    // Executions do not share state.
    run("async () => { globalThis.leak = 1; }").await;
    assert_eq!(run("async () => typeof leak").await.content, r#"{"result":"undefined"}"#);
}
