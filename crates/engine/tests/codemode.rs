//! Runs in code mode, end to end: the mock model writes JavaScript for the code tool (crates/mock,
//! `mock-tools` given a Code Mode tool), the code runs in the sandbox, and its namespace calls
//! reach the mock MCP servers (`/mcp` and `/tracker/mcp` on the mock's port). Both formats: the
//! code tool (`createCodeTool`) and the durable runtime's tool (`createCodemodeRuntime`).

mod common;

use axum::http::StatusCode;
use common::{assert_counters_match, env, i, s, Env};
use serde_json::{json, Value};
use stellarator_core::time;

async fn mcp_server(e: &Env, name: &str) -> i64 {
    mcp_server_at(e, name, "/mcp").await
}

async fn mcp_server_at(e: &Env, name: &str, path: &str) -> i64 {
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

async fn prompts(e: &Env, list: &[&str]) -> i64 {
    let list: Vec<String> = list.iter().map(|p| p.to_string()).collect();
    let n = list.len() as i64;
    e.make_prompt_set_with(n, "Code prompts", "ready", move |k| json!([{"role": "user", "content": list[k as usize]}]))
        .await
}

async fn samples(e: &Env, rid: i64) -> Vec<Value> {
    let page = e.get(&format!("/api/runs/{rid}/samples?limit=100")).await;
    let mut out = Vec::new();
    for item in page["items"].as_array().unwrap() {
        out.push(e.get(&format!("/api/samples/{}", i(&item["id"]))).await);
    }
    out.sort_by_key(|x| i(&x["prompt_idx"]));
    out
}

fn roles(trace: &Value) -> Vec<&str> {
    trace.as_array().unwrap().iter().map(|m| s(&m["role"])).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_code_mode_run_records_what_the_model_saw_and_what_ran() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let srv = mcp_server(&e, "mock-docs").await;
    let ps = prompts(
        &e,
        &[
            "Tide tables code:plain",
            "Add 4 and 5 code:fence",
            "Tides code:loop",
            "Check the lock code:catch",
            "Both at once 6 7 code:parallel",
            "Break it code:throw",
            "Typo code:syntax",
            "Bad input badargs",
        ],
    )
    .await;
    let params = json!({"tools": {"servers": [{"server_id": srv, "tools": ["lookup", "add", "fail", "slow"]}], "mode": "code"}});
    let run = e.start_run(ep, ps, "mock-tools", 4, params).await;
    let rid = i(&run["id"]);

    // The snapshot: one code tool, with the generated types, and the methods' MCP tools.
    let snap = &run["tools"];
    assert_eq!(snap["mode"], "code");
    let defs = snap["definitions"].as_array().unwrap();
    assert_eq!(defs.len(), 1);
    let f = &defs[0]["function"];
    assert_eq!(f["name"], "codemode");
    assert_eq!(
        f["parameters"],
        json!({"$schema": "http://json-schema.org/draft-07/schema#", "type": "object",
               "properties": {"code": {"type": "string", "description": "JavaScript async arrow function to execute"}},
               "required": ["code"], "additionalProperties": false})
    );
    let description = s(&f["description"]);
    assert!(description.starts_with("Execute code to achieve a goal.\n\nAvailable:\ntype LookupInput = {\n    /** The term to look up. */\n    query: string;\n}"), "{description}");
    assert!(description.contains("\ndeclare const mock_docs: {\n\t/**\n\t * Look up a term in the reference catalog and return what it says.\n\t * @param input.query - The term to look up.\n\t */\n\tlookup: (input: LookupInput) => Promise<LookupOutput>;\n"), "{description}");
    assert!(description.ends_with("Example: async () => { const r = await codemode.searchWeb({ query: \"test\" }); return r; }"));
    let code = &snap["code"];
    assert_eq!((code["tool_name"].clone(), code["format"].clone()), (json!("codemode"), json!("@cloudflare/codemode 0.5.6")));
    assert_eq!(code["namespaces"][0]["name"], "mock_docs");
    assert_eq!(code["namespaces"][0]["methods"][1], json!({"name": "add", "server_id": srv, "server": "mock-docs", "tool": "add"}));
    assert_eq!(snap["servers"][0]["tools"][0], json!({"name": "mock_docs.lookup", "server_tool": "lookup"}));
    assert!(s(&code["types"]).starts_with("type LookupInput = {"));
    assert_eq!(run["params"]["tools"]["mode"], "code");

    let run = e.wait_for(rid, |x| x["status"] == "completed", 90.0).await;
    assert_eq!((i(&run["done"]), i(&run["failed"]), i(&run["truncated"])), (8, 0, 0), "{run}");
    assert_counters_match(&run, &e.truth(rid).await);
    let all = samples(&e, rid).await;
    // One execution per sample; the inner calls counted apart.
    assert_eq!(i(&run["tool_calls"]), 8);
    let inner: i64 = all.iter().map(|x| i(&x["inner_tool_calls"])).sum();
    assert_eq!(i(&run["inner_tool_calls"]), inner);
    let page = e.get(&format!("/api/runs/{rid}/samples?limit=100")).await;
    let listed: i64 = page["items"].as_array().unwrap().iter().map(|x| i(&x["inner_tool_calls"])).sum();
    assert_eq!(listed, inner);

    for x in &all {
        let trace = &x["trace"];
        assert_eq!(roles(trace), ["assistant", "tool", "assistant"], "{trace}");
        let call = &trace[0]["tool_calls"][0];
        assert_eq!(call["name"], "codemode");
        assert_eq!(trace[1]["name"], "codemode");
        assert_eq!(trace[1]["server"], "");
        assert_eq!(trace[1]["tool_call_id"], call["id"]);
        assert!(s(&x["response"]).starts_with("Using the tool results: "), "{}", x["response"]);
        let executed = trace[1].get("code").map_or(0, |c| c["calls"].as_array().unwrap().len() as i64);
        assert_eq!(i(&x["inner_tool_calls"]), executed);
    }
    let tool = |k: usize| &all[k]["trace"][1];
    let code_of = |k: usize| &all[k]["trace"][1]["code"];

    // plain: the result as Cloudflare's tool output, JSON.
    assert_eq!(
        tool(0)["content"],
        r#"{"result":"Catalog entry for \"Tide tables\": filed under section 5, last reviewed in 2026."}"#
    );
    assert_eq!((tool(0)["is_error"].clone(), tool(0)["truncated"].clone()), (json!(false), json!(false)));
    let c = code_of(0);
    assert_eq!(c["status"], "ok");
    assert_eq!(c["calls"][0]["method"], "mock_docs.lookup");
    assert_eq!((c["calls"][0]["server"].clone(), c["calls"][0]["tool"].clone()), (json!("mock-docs"), json!("lookup")));
    assert_eq!(c["calls"][0]["arguments"], json!({"query": "Tide tables"}));
    assert_eq!(c["result"], r#""Catalog entry for \"Tide tables\": filed under section 5, last reviewed in 2026.""#);
    assert!(i(&c["latency_ms"]) >= i(&c["calls"][0]["latency_ms"]));

    // fence: normalizeCode stripped it and returned the last expression; "9" came back as a number.
    let c = code_of(1);
    assert!(s(&all[1]["trace"][0]["tool_calls"][0]["arguments"]).contains("```js"));
    assert!(s(&c["code"]).starts_with("async () => {\nconst first = await mock_docs.lookup("), "{}", c["code"]);
    assert!(s(&c["code"]).ends_with("return ({ first, second })\n}"), "{}", c["code"]);
    assert!(s(&tool(1)["content"]).ends_with(r#""second":9}}"#), "{}", tool(1)["content"]);

    // loop: two calls, and the log line.
    let c = code_of(2);
    assert_eq!(c["calls"].as_array().unwrap().len(), 2);
    assert!(s(&tool(2)["content"]).ends_with(r#""logs":["looked up 2"]}"#), "{}", tool(2)["content"]);

    // catch: the tool's error threw in the code, which caught it.
    let c = code_of(3);
    assert_eq!(c["calls"][0]["is_error"], true);
    assert_eq!(c["calls"][0]["result"], "The lookup failed: checking.");
    assert_eq!(c["logs"], json!(["failed as expected: The lookup failed: checking."]));
    assert_eq!(tool(3)["is_error"], false);
    assert!(s(&tool(3)["content"]).contains(r#""note":"The lookup failed: checking.""#));

    // parallel: both calls.
    assert_eq!(code_of(4)["calls"].as_array().unwrap().len(), 2);
    assert!(s(&tool(4)["content"]).ends_with(r#""b":13}}"#), "{}", tool(4)["content"]);

    // throw: the execution failed with the tool's error.
    assert_eq!(tool(5)["content"], "Error: Code execution failed: The lookup failed: checking.");
    assert_eq!((tool(5)["is_error"].clone(), code_of(5)["status"].clone()), (json!(true), json!("error")));
    assert_eq!(code_of(5)["error"], "The lookup failed: checking.");

    // syntax: nothing ran. The text is workerd's, around the parser's own message.
    assert!(
        s(&tool(6)["content"]).starts_with("Error: Failed to start Worker:\nUncaught SyntaxError: ") && s(&tool(6)["content"]).contains("\n  at executor.js:"),
        "{}",
        tool(6)["content"]
    );
    assert_eq!(tool(6)["is_error"], true);
    assert_eq!(code_of(6)["status"], "syntax_error");
    assert_eq!(code_of(6)["calls"], json!([]));

    // Bad input: the AI SDK's error, no execution.
    assert!(s(&tool(7)["content"]).starts_with("AI_InvalidToolInputError: Invalid input for tool codemode: AI_JSONParseError"));
    assert!(tool(7).get("code").is_none());
    assert_eq!(i(&all[7]["inner_tool_calls"]), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_live_view_shows_running_code_and_limits_end_executions() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let srv = mcp_server(&e, "mock").await;
    let ps = prompts(&e, &["Wait code:slow", "Spin code:infinite", "Grow code:memory"]).await;
    let params = json!({"tools": {"servers": [{"server_id": srv}], "mode": "code",
                        "code": {"tool_name": "run_js", "timeout_s": 2, "memory_limit_mb": 16, "namespaces": "codemode"}}});
    let run = e.start_run(ep, ps, "mock-tools", 3, params).await;
    let rid = i(&run["id"]);
    assert_eq!(run["tools"]["definitions"][0]["function"]["name"], "run_js");
    assert_eq!(run["tools"]["code"]["namespaces"][0]["name"], "codemode");
    let lv = e
        .wait_live(
            rid,
            |lv| {
                lv["sequences"].as_array().unwrap().iter().any(|q| q["phase"] == "code" && q["code"]["running"] == json!(["codemode.slow"]))
            },
            20.0,
        )
        .await;
    let q = lv["sequences"].as_array().unwrap().iter().find(|q| q["phase"] == "code" && q["code"]["calls"] == 1).unwrap();
    assert_eq!(q["tools"], json!(["run_js"]));
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!(i(&run["done"]), 3, "{run}");
    let all = samples(&e, rid).await;
    assert_eq!(all[0]["trace"][1]["content"], r#"{"result":"Waited 1 s."}"#);
    let spin = &all[1]["trace"][1];
    assert_eq!(spin["content"], "Error: Code execution failed: Execution timed out");
    assert_eq!(spin["code"]["status"], "timeout");
    assert!(i(&spin["code"]["latency_ms"]) < 5000, "{spin}");
    let grow = &all[2]["trace"][1];
    assert_eq!(grow["code"]["status"], "memory_limit", "{grow}");
    assert!(s(&grow["content"]).starts_with("Error: Code execution failed: ") && s(&grow["content"]).contains("out of memory"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_settings_are_checked() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = prompts(&e, &["a"]).await;
    let srv = mcp_server(&e, "console").await;
    let create = |params: Value| {
        json!({"endpoint_id": ep, "model": "mock-tools", "prompt_set_id": ps, "concurrency": 1, "params": params})
    };
    let (status, body) = e
        .post("/api/runs", create(json!({"tools": {"servers": [{"server_id": srv}], "mode": "code", "code": {"timeout_s": 0}}})))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["fields"]["params.tools.code.timeout_s"], "Use a number of seconds from 1 to 3600.");
    // A server whose name would hide a global the sandbox needs.
    let (status, body) = e.post("/api/runs", create(json!({"tools": {"servers": [{"server_id": srv}], "mode": "code"}}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(s(&body["detail"]).contains("`console`"), "{body}");
    // Under codemode the server's name does not matter.
    let (status, run) = e
        .post("/api/runs", create(json!({"tools": {"servers": [{"server_id": srv}], "mode": "code", "code": {"namespaces": "codemode"}}})))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    e.wait_for(i(&run["id"]), |x| x["status"] == "completed", 60.0).await;
}

/// A runtime-format tool message: the JSON the model saw, with the execution id checked and taken out.
fn runtime_output(tool: &Value) -> Value {
    let mut v: Value = serde_json::from_str(s(&tool["content"])).unwrap_or_else(|e| panic!("{e}: {tool}"));
    let id = s(&v["executionId"]).to_string();
    let parts: Vec<&str> = id.splitn(3, '_').collect();
    assert!(parts.len() == 3 && parts[0] == "exec" && parts[1].len() == 16 && parts[1].bytes().all(|b| b.is_ascii_digit()) && parts[2].len() == 36, "{id}");
    v.as_object_mut().unwrap().remove("executionId");
    v
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_runtime_format_run_searches_describes_then_calls() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let mock = mcp_server(&e, "mock").await;
    let tracker = mcp_server_at(&e, "issue-tracker", "/tracker/mcp").await;
    let ps = prompts(
        &e,
        &[
            "list issues code:plain",
            "add 4 and 5 code:discover",
            "look up tides code:throw",
            "chart the data code:syntax",
            "zzzz qqqq badargs code:catch",
        ],
    )
    .await;
    let params = json!({"tools": {"servers": [{"server_id": mock}, {"server_id": tracker}], "mode": "code",
                        "code": {"format": "runtime", "connector_hints": {"issue-tracker": "Issues of the project."}}}});
    let run = e.start_run(ep, ps, "mock-tools", 4, params).await;
    let rid = i(&run["id"]);

    // The snapshot: the runtime's tool, which lists the connectors and declares no types.
    let snap = &run["tools"];
    let f = &snap["definitions"][0]["function"];
    assert_eq!(f["name"], "codemode");
    assert_eq!(f["parameters"], json!({"type": "object", "properties": {"code": {"type": "string"}}, "required": ["code"], "additionalProperties": false}));
    let description = s(&f["description"]);
    assert!(description.starts_with("Execute JavaScript in a sandbox with access to connector SDKs.\n\n## Workflow\n\n1. `const matches = await codemode.search("), "{description}");
    assert!(description.ends_with("## Available connectors\n\n- `mock`\n- `issue_tracker` \u{2014} Issues of the project."), "{description}");
    assert!(!description.contains("declare const"));
    let code = &snap["code"];
    assert_eq!((code["kind"].clone(), code["format"].clone()), (json!("runtime"), json!("@cloudflare/codemode 0.5.6")));
    assert_eq!(code["namespaces"][1]["name"], "issue_tracker");
    assert_eq!(code["namespaces"][1]["methods"][0], json!({"name": "list_issues", "server_id": tracker, "server": "issue-tracker", "tool": "list-issues"}));
    let connectors = code["connectors"].as_array().unwrap();
    assert_eq!(connectors.len(), 2);
    assert_eq!((connectors[0]["name"].clone(), connectors[0].get("instructions").cloned().unwrap_or(Value::Null)), (json!("mock"), Value::Null));
    assert_eq!(connectors[1]["hint"], "Issues of the project.");
    assert!(s(&connectors[1]["instructions"]).starts_with("A mock issue tracker."), "{}", connectors[1]);
    assert_eq!(connectors[1]["tools"][0]["name"], "list_issues");
    assert!(connectors[1]["tools"][0]["output_schema"].is_object());
    assert_eq!(run["params"]["tools"]["code"]["format"], "runtime");
    // No result budget in this format, as the runtime sets none.
    assert_eq!(run["params"]["tools"]["code"].get("max_result_tokens").cloned().unwrap_or(Value::Null), Value::Null);

    let run = e.wait_for(rid, |x| x["status"] == "completed", 90.0).await;
    assert_eq!((i(&run["done"]), i(&run["failed"]), i(&run["truncated"])), (5, 0, 0), "{run}");
    assert_counters_match(&run, &e.truth(rid).await);
    let all = samples(&e, rid).await;
    for x in &all {
        assert!(s(&x["response"]).starts_with("Using the tool results: "), "{}", x["response"]);
        let executed: i64 = x["trace"].as_array().unwrap().iter().filter_map(|m| m.get("code")).map(|c| c["calls"].as_array().unwrap().len() as i64).sum();
        assert_eq!(i(&x["inner_tool_calls"]), executed);
    }
    // Executions are the tool calls; the MCP calls inside are counted apart (search and describe are the runtime's own).
    assert_eq!(i(&run["tool_calls"]), 3 + 2 + 3 + 3 + 4);
    assert_eq!(i(&run["inner_tool_calls"]), 1 + 1 + 1 + 0 + 2);
    let code_of = |k: usize, at: usize| s(&all[k]["trace"][at]["tool_calls"][0]["arguments"]).to_string();

    // Search, describe, call: three executions, then the answer.
    let t = &all[0]["trace"];
    assert_eq!(roles(t), ["assistant", "tool", "assistant", "tool", "assistant", "tool", "assistant"], "{t}");
    assert_eq!(code_of(0, 0), r#"{"code":"async () => codemode.search(\"list issues\")"}"#);
    let found = runtime_output(&t[1]);
    assert_eq!(found["status"], "completed");
    assert_eq!(found["result"]["results"][0]["path"], "issue_tracker.list_issues");
    assert_eq!((found["result"]["results"][0]["kind"].clone(), found["result"]["truncated"].clone()), (json!("method"), json!(false)));
    assert_eq!(t[1]["code"]["calls"], json!([]));
    assert_eq!(code_of(0, 2), r#"{"code":"async () => codemode.describe(\"issue_tracker.list_issues\")"}"#);
    let described = runtime_output(&t[3]);
    assert_eq!((described["result"]["path"].clone(), described["result"]["kind"].clone()), (json!("issue_tracker.list_issues"), json!("method")));
    assert!(s(&described["result"]["types"]).starts_with("type ListIssuesInput = {\n    /** Which issues to list (default open). */\n    state?: \"open\" | \"closed\" | \"all\";"), "{described}");
    assert!(code_of(0, 4).contains("await issue_tracker.list_issues({ limit: 3 });"), "{}", code_of(0, 4));
    let listed = runtime_output(&t[5]);
    assert_eq!((listed["status"].clone(), listed["result"]["issues"].as_array().unwrap().len(), listed["result"]["total"].clone()), (json!("completed"), 3, json!(333)));
    assert!(listed.get("logs").is_none(), "{listed}");
    let c = &t[5]["code"];
    assert_eq!(c["status"], "ok");
    assert_eq!(c["calls"][0], json!({"method": "issue_tracker.list_issues", "server": "issue-tracker", "tool": "list-issues", "arguments": {"limit": 3},
        "result": c["calls"][0]["result"], "is_error": false, "truncated": false, "latency_ms": c["calls"][0]["latency_ms"]}));
    assert_eq!((t[5]["is_error"].clone(), t[5]["name"].clone(), t[5]["server"].clone()), (json!(false), json!("codemode"), json!("")));

    // Search and describe in one execution, then the call.
    let t = &all[1]["trace"];
    assert_eq!(roles(t), ["assistant", "tool", "assistant", "tool", "assistant"], "{t}");
    assert_eq!(runtime_output(&t[1])["result"]["path"], "mock.add");
    assert!(code_of(1, 2).contains("await mock.add({ a: 4, b: 5 });"), "{}", code_of(1, 2));
    assert_eq!(runtime_output(&t[3]), json!({"status": "completed", "result": 9}));

    // A call that throws: the runtime's error result.
    let t = &all[2]["trace"];
    assert_eq!(runtime_output(&t[1])["result"]["results"][0]["path"], "mock.lookup");
    assert_eq!(runtime_output(&t[5]), json!({"status": "error", "error": "Code execution failed: Tool \"missing_tool\" not found on mock"}));
    assert_eq!((t[5]["is_error"].clone(), t[5]["code"]["status"].clone()), (json!(true), json!("error")));
    assert_eq!(t[5]["code"]["error"], "Code execution failed: Tool \"missing_tool\" not found on mock");

    // Code that does not parse.
    let t = &all[3]["trace"];
    let failed = runtime_output(&t[5]);
    assert_eq!(failed["status"], "error");
    assert!(s(&failed["error"]).starts_with("Failed to start Worker:\nUncaught SyntaxError: "), "{failed}");
    assert_eq!((t[5]["code"]["status"].clone(), t[5]["code"]["calls"].clone()), (json!("syntax_error"), json!([])));

    // Input that is not JSON: the AI SDK's error and no execution; then a search that finds nothing,
    // the connector's description, and a call with a caught error.
    let t = &all[4]["trace"];
    assert_eq!(roles(t), ["assistant", "tool", "assistant", "tool", "assistant", "tool", "assistant", "tool", "assistant"], "{t}");
    assert!(s(&t[1]["content"]).starts_with("AI_InvalidToolInputError: Invalid input for tool codemode: AI_JSONParseError"), "{}", t[1]["content"]);
    assert!(t[1].get("code").is_none());
    assert_eq!(runtime_output(&t[3]), json!({"status": "completed", "result": {"results": [], "total": 0, "truncated": false}}));
    assert_eq!(code_of(4, 4), r#"{"code":"async () => codemode.describe(\"mock\")"}"#);
    assert_eq!(runtime_output(&t[5])["result"]["kind"], "connector");
    let caught = runtime_output(&t[7]);
    assert_eq!(caught["logs"], json!(["failed as expected: Tool \"missing_tool\" not found on mock"]));
    assert_eq!(caught["result"]["note"], "Tool \"missing_tool\" not found on mock");
    assert!(s(&caught["result"]["value"]).starts_with("Catalog entry for \"zzzz qqqq\""), "{caught}");
    assert_eq!(t[7]["code"]["calls"].as_array().unwrap().len(), 2);
    assert_eq!(t[7]["code"]["calls"][0]["is_error"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_runtime_format_shows_in_the_live_view_and_keeps_its_limits() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let srv = mcp_server(&e, "mock").await;
    let ps = prompts(&e, &["waits some seconds code:slow", "add 1 and 2 code:infinite"]).await;
    // "waits seconds" finds mock.slow; "add" finds mock.add, whose call never comes: the code spins.
    let params = json!({"tools": {"servers": [{"server_id": srv}], "mode": "code",
                        "code": {"format": "runtime", "tool_name": "run_js", "timeout_s": 2, "server_instructions": false}}});
    let run = e.start_run(ep, ps, "mock-tools", 2, params).await;
    let rid = i(&run["id"]);
    assert_eq!(run["tools"]["definitions"][0]["function"]["name"], "run_js");
    let lv = e
        .wait_live(
            rid,
            |lv| lv["sequences"].as_array().unwrap().iter().any(|q| q["phase"] == "code" && q["code"]["running"] == json!(["mock.slow"])),
            20.0,
        )
        .await;
    let q = lv["sequences"].as_array().unwrap().iter().find(|q| q["phase"] == "code" && q["code"]["calls"] == 1).unwrap();
    assert_eq!(q["tools"], json!(["run_js"]));
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!(i(&run["done"]), 2, "{run}");
    let all = samples(&e, rid).await;
    assert_eq!(runtime_output(&all[0]["trace"][5]), json!({"status": "completed", "result": "Waited 1 s."}));
    let spin = &all[1]["trace"][5];
    assert_eq!(runtime_output(spin), json!({"status": "error", "error": "Code execution failed: Execution timed out"}));
    assert_eq!(spin["code"]["status"], "timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_format_settings_are_checked() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = prompts(&e, &["a"]).await;
    let srv = mcp_server(&e, "mock").await;
    let create = |code: Value| {
        json!({"endpoint_id": ep, "model": "mock-tools", "prompt_set_id": ps, "concurrency": 1,
               "params": {"tools": {"servers": [{"server_id": srv}], "mode": "code", "code": code}}})
    };
    let (status, body) = e.post("/api/runs", create(json!({"format": "durable"}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // The runtime has one global per server.
    let (status, body) = e.post("/api/runs", create(json!({"format": "runtime", "namespaces": "codemode"}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["fields"]["params.tools.code.namespaces"].is_string(), "{body}");
    // Hints belong to the runtime format, and to a server of the run.
    let (status, body) = e.post("/api/runs", create(json!({"connector_hints": {"mock": "Lookups."}}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["fields"]["params.tools.code.connector_hints"].is_string(), "{body}");
    let (status, body) = e.post("/api/runs", create(json!({"format": "runtime", "connector_hints": {"github": "Pull requests."}}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(s(&body["detail"]).contains("\"github\""), "{body}");
    // A server named as the runtime's own namespace.
    let own = mcp_server(&e, "codemode").await;
    let (status, body) = e
        .post(
            "/api/runs",
            json!({"endpoint_id": ep, "model": "mock-tools", "prompt_set_id": ps, "concurrency": 1,
                   "params": {"tools": {"servers": [{"server_id": own}], "mode": "code", "code": {"format": "runtime"}}}}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(s(&body["detail"]).contains("`codemode`"), "{body}");
}
