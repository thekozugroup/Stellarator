//! Runs with MCP tools, end to end: the mock tool-calling model (crates/mock, `mock-tools`) and the
//! mock MCP server (`/mcp` on a mock's port), in-process.

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;

use axum::http::StatusCode;
use common::{assert_counters_match, env, i, s, Env};
use serde_json::{json, Value};
use stellarator_core::time;

/// Save an HTTP MCP server pointing at `url`; returns its id.
async fn mcp_server(e: &Env, name: &str, url: &str) -> i64 {
    let now = time::now_db();
    sqlx::query(
        "INSERT INTO mcp_servers (name, transport, url, created_at, updated_at) VALUES (?, 'http', ?, ?, ?)",
    )
    .bind(name)
    .bind(url)
    .bind(&now)
    .bind(&now)
    .execute(&e.db)
    .await
    .unwrap()
    .last_insert_rowid()
}

/// The mock's MCP URL ("http://127.0.0.1:port/mcp").
fn mcp_url(e: &Env) -> String {
    e.mock.trim_end_matches("/v1").to_string() + "/mcp"
}

/// A TCP proxy in front of an MCP server that can be cut: every open connection is dropped and the
/// port stops listening, as when a server crashes.
struct Proxy {
    addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl Proxy {
    async fn start(addr: SocketAddr, target: SocketAddr) -> Proxy {
        let listener = TcpListener::bind(addr).await.expect("bind the proxy");
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut conns = JoinSet::new();
            while let Ok((mut inbound, _)) = listener.accept().await {
                conns.spawn(async move {
                    if let Ok(mut outbound) = TcpStream::connect(target).await {
                        let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                    }
                });
            }
        });
        Proxy { addr, task }
    }

    /// Drop the listener and every connection (the JoinSet goes with the task).
    async fn cut(self) {
        self.task.abort();
        let _ = self.task.await;
    }
}

async fn prompts(e: &Env, list: &[&str]) -> i64 {
    let list: Vec<String> = list.iter().map(|p| p.to_string()).collect();
    let n = list.len() as i64;
    e.make_prompt_set_with(n, "Tool prompts", "ready", move |k| json!([{"role": "user", "content": list[k as usize]}]))
        .await
}

async fn samples(e: &Env, rid: i64) -> Vec<Value> {
    let page = e.get(&format!("/api/runs/{rid}/samples?limit=100")).await;
    let mut out = Vec::new();
    for item in page["items"].as_array().unwrap() {
        out.push(e.get(&format!("/api/samples/{}", i(&item["id"]))).await);
    }
    out
}

fn roles(trace: &Value) -> Vec<&str> {
    trace.as_array().unwrap().iter().map(|m| s(&m["role"])).collect()
}

fn sum(trace: &Value, key: &str) -> i64 {
    trace.as_array().unwrap().iter().filter(|m| m["role"] == "assistant").map(|m| m[key].as_i64().unwrap_or(0)).sum()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_calls_tools_and_keeps_the_trace() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let srv = mcp_server(&e, "mock", &mcp_url(&e)).await;
    let ps = prompts(
        &e,
        &[
            "Look up tide tables tool:lookup",
            "Add 4 and 5 both",
            "Use the broken one tool:fail",
            "Send bad arguments tool:lookup badargs",
            "Draw it tool:chart",
            "Repeat 200 times tool:repeat",
        ],
    )
    .await;
    let params = json!({"tools": {"servers": [{"server_id": srv}], "max_rounds": 3, "max_result_chars": 1000}});
    let run = e.start_run(ep, ps, "mock-tools", 4, params).await;
    let rid = i(&run["id"]);
    // The snapshot: every tool of the server, as OpenAI functions, with their own names.
    let snap = &run["tools"];
    assert_eq!(snap["servers"][0]["name"], "mock");
    assert_eq!(snap["servers"][0]["server_id"], srv);
    let names: Vec<&str> = snap["definitions"].as_array().unwrap().iter().map(|d| s(&d["function"]["name"])).collect();
    assert_eq!(names, ["lookup", "add", "fail", "slow", "chart", "repeat"]);
    assert_eq!(snap["definitions"][0]["type"], "function");
    assert_eq!(snap["definitions"][0]["function"]["parameters"]["required"], json!(["query"]));

    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!((i(&run["done"]), i(&run["failed"]), i(&run["truncated"])), (6, 0, 0), "{run}");
    assert_counters_match(&run, &e.truth(rid).await);
    let all = samples(&e, rid).await;
    let total_calls: i64 = all.iter().map(|x| i(&x["tool_calls"])).sum();
    assert_eq!(i(&run["tool_calls"]), total_calls);
    assert_eq!(total_calls, 7, "one call each, two for \"both\"");

    for x in &all {
        let trace = &x["trace"];
        // Tokens are summed over the sample's requests.
        assert_eq!(i(&x["completion_tokens"]), sum(trace, "completion_tokens"));
        assert_eq!(i(&x["prompt_tokens"]), sum(trace, "prompt_tokens"));
        assert_eq!(i(&x["reasoning_tokens"]), sum(trace, "reasoning_tokens"));
        // reasoning and response are the final answer's.
        let last = trace.as_array().unwrap().last().unwrap();
        assert_eq!(x["response"], last["content"]);
        assert_eq!(x["reasoning"], last["reasoning"]);
        assert_eq!(x["finish_reason"], "stop");
        assert!(s(&x["response"]).starts_with("Using the tool results: "), "{}", x["response"]);
        // The first answer's reasoning is kept in the trace.
        assert!(s(&trace[0]["reasoning"]).contains("I should call"));
        assert_eq!(trace[0]["finish_reason"], "tool_calls");
    }
    let by_prompt = |p: i64| all.iter().find(|x| i(&x["prompt_idx"]) == p).unwrap();

    let lookup = by_prompt(0);
    assert_eq!(roles(&lookup["trace"]), ["assistant", "tool", "assistant"]);
    let call = &lookup["trace"][0]["tool_calls"][0];
    assert_eq!(call["name"], "lookup");
    assert_eq!(call["arguments"], r#"{"query":"Look up tide tables"}"#, "arguments arrive in pieces and are joined");
    let result = &lookup["trace"][1];
    assert_eq!(result["tool_call_id"], call["id"]);
    assert_eq!((result["name"].clone(), result["server"].clone()), (json!("lookup"), json!("mock")));
    assert_eq!(result["content"], "Catalog entry for \"Look up tide tables\": filed under section 6, last reviewed in 2026.");
    assert_eq!((result["is_error"].clone(), result["truncated"].clone()), (json!(false), json!(false)));

    let both = by_prompt(1);
    assert_eq!(roles(&both["trace"]), ["assistant", "tool", "tool", "assistant"]);
    assert_eq!(both["trace"][2]["content"], "9");
    assert_eq!(i(&both["tool_calls"]), 2);

    let fail = &by_prompt(2)["trace"][1];
    assert_eq!((fail["content"].clone(), fail["is_error"].clone()), (json!("The lookup failed: the record is locked."), json!(true)));
    let bad = &by_prompt(3)["trace"][1];
    assert_eq!(bad["is_error"], true);
    assert!(s(&bad["content"]).starts_with("The arguments are not valid JSON"), "{bad}");
    assert_eq!(by_prompt(4)["trace"][1]["content"], "Chart of the data:\n[image: image/png, 68 bytes]");
    let long = &by_prompt(5)["trace"][1];
    assert_eq!(long["truncated"], true);
    assert!(s(&long["content"]).ends_with("\n[truncated: 2200 more characters]"), "{}", long["content"]);

    // The samples table counts the calls.
    let page = e.get(&format!("/api/runs/{rid}/samples?limit=1")).await;
    assert_eq!(i(&page["items"][0]["tool_calls"]), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_streaming_and_required_tool_choice() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let srv = mcp_server(&e, "mock", &mcp_url(&e)).await;
    let ps = prompts(&e, &["What is 1 + 2? tool:add", "Find it tool:lookup"]).await;
    let params = json!({"stream": false, "tools": {"servers": [{"server_id": srv, "tools": ["add", "lookup"]}],
                        "tool_choice": "required"}});
    let run = e.start_run(ep, ps, "mock-tools", 2, params).await;
    let rid = i(&run["id"]);
    assert_eq!(run["tools"]["definitions"].as_array().unwrap().len(), 2, "only the picked tools are sent");
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!(i(&run["done"]), 2);
    let all = samples(&e, rid).await;
    assert_eq!(all[0]["trace"][1]["content"], "3");
    assert!(all.iter().all(|x| x["ttft_ms"].is_null() && roles(&x["trace"]) == ["assistant", "tool", "assistant"]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn max_rounds_truncates_with_tool_rounds() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let srv = mcp_server(&e, "mock", &mcp_url(&e)).await;
    let ps = prompts(&e, &["Keep going tool:add", "Again tool:add", "More tool:add"]).await;
    let params = json!({"tools": {"servers": [{"server_id": srv}], "max_rounds": 2}});
    let rid = i(&e.start_run(ep, ps, "mock-tools-loop", 3, params).await["id"]);
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert_eq!((i(&run["done"]), i(&run["truncated"]), i(&run["tool_calls"])), (3, 3, 6), "{run}");
    assert_counters_match(&run, &e.truth(rid).await);
    for x in samples(&e, rid).await {
        assert_eq!(x["finish_reason"], "tool_rounds");
        // Two rounds of calls ran; the third answer's calls did not.
        assert_eq!(roles(&x["trace"]), ["assistant", "tool", "assistant", "tool", "assistant"]);
        assert_eq!(x["trace"][4]["tool_calls"].as_array().unwrap().len(), 1);
        assert_eq!(i(&x["tool_calls"]), 2);
    }
    let page = e.get(&format!("/api/runs/{rid}/samples?status=truncated")).await;
    assert_eq!(i(&page["total"]), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_endpoint_without_tool_calling_pauses_the_run() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let srv = mcp_server(&e, "mock", &mcp_url(&e)).await;
    let ps = prompts(&e, &["a", "b", "c", "d"]).await;
    let params = json!({"tools": {"servers": [{"server_id": srv}]}});
    let rid = i(&e.start_run(ep, ps, "mock-no-tools", 2, params).await["id"]);
    let run = e.wait_for(rid, |x| x["status"] == "paused", 30.0).await;
    let reason = s(&run["status_reason"]);
    assert!(reason.starts_with("The endpoint refused the tool definitions"), "{reason}");
    assert!(reason.contains("--enable-auto-tool-choice") && reason.contains("docs/tools.md"), "{reason}");
    // No sample failed: they wait for an endpoint that takes tools.
    assert_eq!((i(&run["failed"]), i(&run["done"])), (0, 0));
    assert_eq!(e.count("SELECT COUNT(*) FROM samples WHERE run_id = ? AND status = 'pending'", rid).await, 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_crashed_server_fails_samples_and_retry_failed_recovers() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    // The MCP server is the mock's, behind a proxy that goes down while the model keeps answering.
    let target: SocketAddr = e.mock.trim_start_matches("http://").trim_end_matches("/v1").parse().unwrap();
    let proxy = Proxy::start(SocketAddr::from(([127, 0, 0, 1], 0)), target).await;
    let addr = proxy.addr;
    let srv = mcp_server(&e, "flaky-tools", &format!("http://{addr}/mcp")).await;
    let ps = prompts(&e, &["One tool:slow", "Two tool:slow", "Three tool:slow", "Four tool:slow"]).await;
    let params = json!({"tools": {"servers": [{"server_id": srv}], "tool_timeout_s": 20}, "pause_after_failures": 0});
    let rid = i(&e.start_run(ep, ps, "mock-tools", 4, params).await["id"]);
    // The samples wait in `slow` (2 s) when the server goes away.
    e.wait_live(rid, |lv| lv["sequences"].as_array().unwrap().iter().any(|q| q["phase"] == "tool"), 20.0).await;
    let lv = e.live(rid).await;
    let in_tool = lv["sequences"].as_array().unwrap().iter().find(|q| q["phase"] == "tool").unwrap();
    assert_eq!(in_tool["tools"], json!(["slow"]));
    assert_eq!(i(&in_tool["round"]), 1);
    proxy.cut().await;
    let run = e.wait_for(rid, |x| x["status"] == "completed", 60.0).await;
    assert!(i(&run["failed"]) > 0, "{run}");
    // Every sample that met the dead server failed with the reason, the one whose call was in
    // flight included: its lost answer is a failed connection, not a tool that was too slow. (A
    // sample is done only if its call had finished before the server went away.)
    for x in samples(&e, rid).await {
        if x["status"] == "failed" {
            let err = s(&x["error"]);
            assert!(err.starts_with("The MCP server \"flaky-tools\" stopped answering"), "{err}");
        } else {
            let tool = x["trace"].as_array().unwrap().iter().find(|m| m["role"] == "tool").unwrap();
            assert!(!s(&tool["content"]).contains("did not finish within"), "{x}");
        }
    }

    // The server is back on the same port: Retry failed runs them again.
    let back = Proxy::start(addr, target).await;
    let (status, _) = e.post(&format!("/api/runs/{rid}/retry-failed"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let run = e.wait_for(rid, |x| x["status"] == "completed" && x["failed"] == 0, 60.0).await;
    assert_eq!(i(&run["done"]), 4);
    assert_counters_match(&run, &e.truth(rid).await);
    back.cut().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_creation_checks_the_servers() {
    let e = env().await;
    let ep = e.make_endpoint(&e.mock, "Mock").await;
    let ps = prompts(&e, &["a"]).await;
    let srv = mcp_server(&e, "mock", &mcp_url(&e)).await;
    let create = |params: Value| {
        json!({"endpoint_id": ep, "model": "mock-tools", "prompt_set_id": ps, "concurrency": 1, "params": params})
    };
    let (status, body) = e.post("/api/runs", create(json!({"tools": {"servers": [{"server_id": 999}]}}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["fields"]["params.tools.servers.0.server_id"], "This MCP server does not exist. Pick another server.");
    let (status, body) =
        e.post("/api/runs", create(json!({"tools": {"servers": [{"server_id": srv, "tools": ["nope"]}]}}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(s(&body["detail"]).contains("has no tool named \"nope\""), "{body}");
    let (status, body) = e.post("/api/runs", create(json!({"tools": {"servers": [], "max_rounds": 40}}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["fields"].get("params.tools.servers").is_some() && body["fields"].get("params.tools.max_rounds").is_some());
    let dead = mcp_server(&e, "dead", &common::dead_url().replace("/v1", "/mcp")).await;
    let (status, body) = e.post("/api/runs", create(json!({"tools": {"servers": [{"server_id": dead}]}}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(s(&body["detail"]).starts_with("Couldn't reach the MCP server \"dead\""), "{body}");
    // The failed check is stored on the server.
    let status: String = sqlx::query_scalar("SELECT status FROM mcp_servers WHERE id = ?").bind(dead).fetch_one(&e.db).await.unwrap();
    assert_eq!(status, "error");
    // Two servers with the same tools: every clashing name is prefixed.
    let twin = mcp_server(&e, "twin", &mcp_url(&e)).await;
    let (status, run) = e
        .post(
            "/api/runs",
            create(json!({"tools": {"servers": [{"server_id": srv, "tools": ["lookup", "add"]}, {"server_id": twin, "tools": ["lookup"]}]}})),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let names: Vec<&str> = run["tools"]["definitions"].as_array().unwrap().iter().map(|d| s(&d["function"]["name"])).collect();
    assert_eq!(names, ["mock__lookup", "add", "twin__lookup"]);
    assert_eq!(run["tools"]["servers"][1]["tools"], json!([{"name": "twin__lookup", "server_tool": "lookup"}]));
    assert_eq!(run["params"]["tools"]["servers"][1], json!({"server_id": twin, "tools": ["lookup"]}));
    let run = e.wait_for(i(&run["id"]), |x| x["status"] == "completed", 60.0).await;
    assert_eq!(i(&run["done"]), 1);
    let x = &samples(&e, i(&run["id"])).await[0];
    assert_eq!(x["trace"][0]["tool_calls"][0]["name"], "mock__lookup");
    assert_eq!(x["trace"][1]["server"], "mock");
    // A paused run whose server was deleted cannot resume.
    sqlx::query("UPDATE runs SET status = 'paused' WHERE id = ?").bind(i(&run["id"])).execute(&e.db).await.unwrap();
    sqlx::query("UPDATE samples SET status = 'pending' WHERE run_id = ?").bind(i(&run["id"])).execute(&e.db).await.unwrap();
    sqlx::query("DELETE FROM mcp_servers WHERE id = ?").bind(twin).execute(&e.db).await.unwrap();
    let (status, _) = e.post(&format!("/api/runs/{}/resume", run["id"]), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let run = e.wait_for(i(&run["id"]), |x| x["status"] == "paused", 30.0).await;
    assert!(s(&run["status_reason"]).contains("\"twin\" this run uses was deleted"), "{run}");
    tokio::time::sleep(Duration::from_millis(10)).await;
}
