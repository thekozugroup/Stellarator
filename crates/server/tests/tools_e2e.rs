//! The whole app with MCP tools, end to end: the mock model and the mock MCP server, an MCP server
//! added by pasting JSON, a run with tools, and the export records of its traces.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use stellarator_core::config::{Config, Mode};
use tower::ServiceExt;

struct App {
    router: Router,
    _dir: tempfile::TempDir,
}

impl App {
    async fn call(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, String) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(v) => {
                req = req.header("content-type", "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        let resp = self.router.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn json(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let (status, text) = self.call(method, uri, body).await;
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tool_run_exports_its_traces() {
    let (mock, _task) = stellarator_mock::serve(SocketAddr::from(([127, 0, 0, 1], 0)), 400.0).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let parts = stellarator_server::full_parts(Config::new(dir.path().to_path_buf(), Mode::Native)).await.unwrap();
    let app = App { router: stellarator_server::app(&parts), _dir: dir };

    let (status, ep) = app.json(Method::POST, "/api/endpoints", Some(json!({"name": "Mock", "base_url": format!("http://{mock}/v1")}))).await;
    assert_eq!(status, StatusCode::CREATED, "{ep}");
    let paste = json!({"mcpServers": {"tools": {"type": "http", "url": format!("http://{mock}/mcp")}}});
    let (_, imported) = app.json(Method::POST, "/api/mcp-servers/import", Some(json!({"text": paste.to_string(), "check": true}))).await;
    let server = &imported["created"][0];
    assert_eq!(server["status"], "ok", "{imported}");

    let text = "Find tide tables tool:lookup\n\nAdd 2 and 3 tool:add\n\nKeep going tool:add";
    let (status, set) = app
        .json(Method::POST, "/api/imports", Some(json!({"source": {"kind": "paste", "text": text}, "mapping": {"split_mode": "blank_line"}, "name": "Tools"})))
        .await;
    assert!(status.is_success(), "{set}");
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.json(Method::GET, &format!("/api/prompt-sets/{}", set["id"]), None).await.1["status"] != "ready" {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let params = json!({"tools": {"servers": [{"server_id": server["id"]}], "max_rounds": 2}});
    let (status, run) = app
        .json(Method::POST, "/api/runs", Some(json!({"endpoint_id": ep["id"], "model": "mock-tools", "prompt_set_id": set["id"], "concurrency": 3, "params": params})))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let rid = run["id"].as_i64().unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let run = loop {
        let (_, r) = app.json(Method::GET, &format!("/api/runs/{rid}"), None).await;
        if r["status"] == "completed" {
            break r;
        }
        assert!(Instant::now() < deadline, "{r}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!((run["done"].as_i64(), run["failed"].as_i64(), run["tool_calls"].as_i64()), (Some(3), Some(0), Some(3)));
    let (_, mcp) = app.json(Method::GET, "/api/mcp-servers", None).await;
    assert_eq!(mcp[0]["run_count"], 1);

    // messages (JSONL): the trajectory with tool_calls and tool messages, plus the tools column.
    let (status, body) = app.call(Method::GET, &format!("/api/runs/{rid}/export?format=messages&reasoning=reasoning_content"), None).await;
    assert_eq!(status, StatusCode::OK);
    let records: Vec<Value> = body.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(records.len(), 3);
    let first = &records[0];
    let roles: Vec<&str> = first["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["user", "assistant", "tool", "assistant"]);
    let call = &first["messages"][1]["tool_calls"][0];
    assert_eq!(call["type"], "function");
    assert_eq!(call["function"], json!({"name": "lookup", "arguments": {"query": "Find tide tables"}}));
    assert_eq!(first["messages"][2]["tool_call_id"], call["id"]);
    assert_eq!(first["messages"][2]["name"], "lookup");
    assert!(first["messages"][1]["reasoning_content"].as_str().unwrap().contains("I should call lookup"));
    assert!(first["messages"][3]["reasoning_content"].is_string());
    assert_eq!(first["tools"], run["tools"]["definitions"]);

    // ShareGPT: function_call and observation turns, tools as a JSON string.
    let (_, body) = app.call(Method::GET, &format!("/api/runs/{rid}/export?format=sharegpt"), None).await;
    let rec: Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
    let from: Vec<&str> = rec["conversations"].as_array().unwrap().iter().map(|m| m["from"].as_str().unwrap()).collect();
    assert_eq!(from, ["human", "function_call", "observation", "gpt"]);
    assert!(rec["tools"].is_string());

    // raw: the stored trace and the definitions.
    let (_, body) = app.call(Method::GET, &format!("/api/runs/{rid}/export?format=raw"), None).await;
    let rec: Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
    assert_eq!(rec["tool_calls"], 1);
    assert_eq!(rec["trace"].as_array().unwrap().len(), 3);

    // Formats that cannot hold tools say so, in the preview and the download.
    for fmt in ["alpaca", "prompt_completion"] {
        let (status, err) = app.json(Method::POST, &format!("/api/runs/{rid}/export/preview"), Some(json!({"format": fmt}))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(err["fields"]["format"].as_str().unwrap().contains("For a run with tools, export Chat messages"), "{err}");
        let (status, _) = app.call(Method::GET, &format!("/api/runs/{rid}/export?format={fmt}"), None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
    // The preview shows the first record exactly as written.
    let (_, preview) = app.json(Method::POST, &format!("/api/runs/{rid}/export/preview"), Some(json!({"format": "messages", "reasoning": "reasoning_content"}))).await;
    assert_eq!(preview["example"], records[0]);
    assert_eq!(preview["count"], 3);

    // Parquet and CSV write the same records (arguments and tools as JSON text).
    for ft in ["parquet", "csv"] {
        let (status, body) = app.call(Method::GET, &format!("/api/runs/{rid}/export?format=messages&file_type={ft}"), None).await;
        assert_eq!(status, StatusCode::OK, "{ft}");
        assert!(!body.is_empty());
    }
    for l in &parts.lifecycle {
        l.stop().await;
    }
}
