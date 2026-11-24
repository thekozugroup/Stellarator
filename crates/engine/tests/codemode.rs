//! Runs in code mode, end to end: the mock model writes JavaScript for the code tool (crates/mock,
//! `mock-tools` given a Code Mode tool), the code runs in the sandbox, and its namespace calls
//! reach the mock MCP server (`/mcp` on the mock's port).

mod common;

use axum::http::StatusCode;
use common::{assert_counters_match, env, i, s, Env};
use serde_json::{json, Value};
use stellarator_core::time;

async fn mcp_server(e: &Env, name: &str) -> i64 {
    let now = time::now_db();
    let url = e.mock.trim_end_matches("/v1").to_string() + "/mcp";
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

    // syntax: nothing ran.
    assert!(s(&tool(6)["content"]).starts_with("SyntaxError: "), "{}", tool(6)["content"]);
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
