//! `/api/mcp-servers`: create, read, update with secrets kept, import, check against the mock MCP
//! server, delete, and the server-mode rule for stdio servers.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::StatusCode;
use common::{env, env_stdio_allowed, env_with, ActiveRuns, Env};
use serde_json::{json, Value};
use stellarator_core::probes::{ActiveRun, Idle};

const STDIO_BLOCKED: &str = "Stdio MCP servers run commands on this machine, so they are off in server mode";

async fn mock(api_key: Option<&str>) -> (String, tokio::task::JoinHandle<()>) {
    let (addr, join) =
        stellarator_mock::serve_with_key(SocketAddr::from(([127, 0, 0, 1], 0)), 400.0, api_key.map(str::to_string))
            .await
            .unwrap();
    (format!("http://{addr}/mcp"), join)
}

async fn create(e: &Env, body: Value) -> Value {
    let (status, out) = e.post("/api/mcp-servers", body).await;
    assert_eq!(status, StatusCode::CREATED, "{out}");
    out
}

fn id(v: &Value) -> i64 {
    v["id"].as_i64().unwrap()
}

#[tokio::test]
async fn secrets_are_hints_and_edits_keep_them() {
    let e = env().await;
    let out = create(
        &e,
        json!({"name": "docs", "transport": "http", "url": " https://docs.example.com/mcp ",
               "headers": {"Authorization": "Bearer sk-secret-123456", "X-Short": "abc"}}),
    )
    .await;
    assert_eq!(out["url"], "https://docs.example.com/mcp");
    assert_eq!(out["headers"], json!([{"name": "Authorization", "hint": "…3456"}, {"name": "X-Short", "hint": "…"}]));
    assert_eq!((out["status"].clone(), out["enabled"].clone(), out["blocked"].clone()), (json!("unchecked"), json!(true), Value::Null));
    assert_eq!(out["tools"], json!([]));
    assert_eq!(out["run_count"], 0);
    let (_, list) = e.get("/api/mcp-servers").await;
    assert!(!list.to_string().contains("sk-secret"), "values never leave the server: {list}");
    let sid = id(&out);
    let stored = || async {
        let h: String = sqlx::query_scalar("SELECT headers FROM mcp_servers WHERE id = ?").bind(sid).fetch_one(&e.ctx.db).await.unwrap();
        serde_json::from_str::<Value>(&h).unwrap()
    };
    // null keeps the saved value, a string replaces, a name left out goes.
    let (status, out) =
        e.patch(&format!("/api/mcp-servers/{sid}"), json!({"headers": {"Authorization": null, "X-New": "value-0000001"}})).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(stored().await, json!({"Authorization": "Bearer sk-secret-123456", "X-New": "value-0000001"}));
    assert_eq!(out["headers"][1], json!({"name": "X-New", "hint": "…0001"}));
    // Renaming alone keeps every secret.
    let (_, out) = e.patch(&format!("/api/mcp-servers/{sid}"), json!({"name": "docs-2"})).await;
    assert_eq!(out["name"], "docs-2");
    assert_eq!(stored().await["Authorization"], "Bearer sk-secret-123456");
    let (status, err) = e.patch(&format!("/api/mcp-servers/{sid}"), json!({"headers": {"Nope": null}})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err["fields"]["headers"], "Nope has no saved value to keep. Enter a value.");
    let (_, out) = e.patch(&format!("/api/mcp-servers/{sid}"), json!({"headers": {}})).await;
    assert_eq!(out["headers"], json!([]));
    assert_eq!(stored().await, json!({}));
}

#[tokio::test]
async fn validation_and_names() {
    let e = env().await;
    let bad = [
        (json!({"name": "has space", "transport": "http", "url": "http://x"}), "name", "Use only letters, digits, _ and -."),
        (json!({"name": "a", "transport": "http"}), "url", "Enter the server's URL, for example https://example.com/mcp."),
        (json!({"name": "a", "transport": "http", "url": "ftp://x"}), "url", "The URL must start with http:// or https://"),
        (json!({"name": "a", "transport": "http", "url": "http://x", "headers": {"bad header": "v"}}), "headers", "\"bad header\" is not a valid HTTP header name."),
        (json!({"name": "a".repeat(65), "transport": "http", "url": "http://x"}), "name", "At most 64 characters."),
        (json!({"name": "a", "transport": "websocket", "url": "http://x"}), "transport", "\"websocket\" is not one of: stdio or http."),
    ];
    for (body, field, message) in bad {
        let (status, err) = e.post("/api/mcp-servers", body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(err["fields"][field], message, "{body}: {err}");
    }
    create(&e, json!({"name": "GitHub", "transport": "http", "url": "http://x"})).await;
    let (status, err) = e.post("/api/mcp-servers", json!({"name": "github", "transport": "http", "url": "http://y"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err["fields"]["name"], "A server named \"github\" already exists. Pick another name.");
    let (status, _) = e.get("/api/mcp-servers/99").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stdio_servers_need_permission_in_server_mode() {
    let e = env().await;
    let (_, settings) = e.get("/api/settings").await;
    assert_eq!(settings["stdio_mcp_allowed"], false);
    let stdio = json!({"name": "fs", "transport": "stdio", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem"]});
    let (status, err) = e.post("/api/mcp-servers", stdio.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(err["detail"].as_str().unwrap().starts_with(STDIO_BLOCKED), "{err}");
    assert!(err["detail"].as_str().unwrap().contains("STELLARATOR_ALLOW_STDIO_MCP=1"));
    let paste = json!({"mcpServers": {"fs": {"command": "npx"}, "web": {"url": "http://127.0.0.1:1/mcp"}}});
    let (status, out) = e.post("/api/mcp-servers/import", json!({"text": paste.to_string()})).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["created"].as_array().unwrap().len(), 1);
    assert_eq!(out["errors"][0]["entry"], "fs");
    assert!(out["errors"][0]["message"].as_str().unwrap().starts_with(STDIO_BLOCKED));

    // Allowed (the desktop app, or the variable set): saved, and not blocked.
    let a = env_stdio_allowed().await;
    let (_, settings) = a.get("/api/settings").await;
    assert_eq!(settings["stdio_mcp_allowed"], true);
    let out = create(&a, json!({"name": "fs", "transport": "stdio", "command": "npx", "args": ["-y", "x"], "env": {"TOKEN": "abcdefghijk"}})).await;
    assert_eq!(out["blocked"], Value::Null);
    assert_eq!(out["env"], json!([{"name": "TOKEN", "hint": "…hijk"}]));
    // The same row in server mode is listed with the reason it cannot run, and cannot be checked.
    let now = stellarator_core::time::now_db();
    sqlx::query("INSERT INTO mcp_servers (name, transport, command, created_at, updated_at) VALUES ('old', 'stdio', 'x', ?, ?)")
        .bind(&now)
        .bind(&now)
        .execute(&e.ctx.db)
        .await
        .unwrap();
    let (_, list) = e.get("/api/mcp-servers").await;
    let old = list.as_array().unwrap().iter().find(|s| s["name"] == "old").unwrap();
    assert!(old["blocked"].as_str().unwrap().starts_with(STDIO_BLOCKED));
    let (status, _) = e.post(&format!("/api/mcp-servers/{}/check", id(old)), json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Renaming it is fine; changing its command is not.
    let (status, _) = e.patch(&format!("/api/mcp-servers/{}", id(old)), json!({"name": "older"})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = e.patch(&format!("/api/mcp-servers/{}", id(old)), json!({"command": "y"})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn check_lists_and_caches_the_tools() {
    let (url, _mock) = mock(Some("sk-mcp-key-0001")).await;
    let e = env().await;
    let out = create(&e, json!({"name": "mock", "transport": "http", "url": url, "headers": {"Authorization": "Bearer sk-mcp-key-0001"}})).await;
    let sid = id(&out);
    let (status, out) = e.post(&format!("/api/mcp-servers/{sid}/check"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out["status"], "ok", "{out}");
    assert_eq!(out["error"], Value::Null);
    assert!(out["checked_at"].is_string());
    assert!(out["server_info"].as_str().unwrap().starts_with("stellarator-mock-tools "));
    let names: Vec<&str> = out["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["lookup", "add", "fail", "slow", "chart", "repeat"]);
    assert_eq!(out["tools"][0]["input_schema"]["required"], json!(["query"]));
    // A wrong key: the check fails, says why, and keeps the tools found before.
    let (_, out) = e.patch(&format!("/api/mcp-servers/{sid}"), json!({"headers": {"Authorization": "Bearer wrong"}})).await;
    assert_eq!(out["status"], "unchecked", "an edit of how the server is reached clears the status");
    let (_, out) = e.post(&format!("/api/mcp-servers/{sid}/check"), json!({})).await;
    assert_eq!(out["status"], "error");
    assert!(out["error"].as_str().unwrap().starts_with("Couldn't open an MCP session at"), "{out}");
    assert_eq!(out["tools"].as_array().unwrap().len(), 6);
}

#[tokio::test]
async fn import_reads_pasted_configurations() {
    let (url, _mock) = mock(None).await;
    let e = env_stdio_allowed().await;
    create(&e, json!({"name": "docs", "transport": "http", "url": url})).await;
    let paste = format!(
        r#"{{
          // Claude Desktop / Cursor style
          "mcpServers": {{
            "docs": {{"url": "{url}"}},
            "fetch": {{"command": "uvx", "args": ["mcp-server-fetch"], "env": {{"KEY": "k-0000000001"}}}},
            "old": {{"type": "sse", "url": "{url}"}},
            "broken": {{"type": "websocket"}},
          }}
        }}"#
    );
    let (status, out) = e.post("/api/mcp-servers/import", json!({"text": paste, "check": true})).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let created: Vec<(&str, &str)> = out["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["name"].as_str().unwrap(), s["status"].as_str().unwrap()))
        .collect();
    // The HTTP servers were checked; the stdio one could not start uvx here (or did), either way it was tried.
    assert_eq!(created[0], ("docs-2", "ok"));
    assert_eq!(created[1].0, "fetch");
    assert_ne!(created[1].1, "unchecked");
    assert_eq!(created[2], ("old", "ok"), "the mock answers streamable HTTP at the same URL");
    assert_eq!(out["created"][1]["env"], json!([{"name": "KEY", "hint": "…0001"}]));
    assert_eq!(out["errors"], json!([{"entry": "broken", "message": "The transport \"websocket\" is not supported; use stdio or http."}]));
    let notes: Vec<&str> = out["notes"].as_array().unwrap().iter().map(|n| n["entry"].as_str().unwrap()).collect();
    assert_eq!(notes, ["docs", "old"]);
    assert_eq!(out["notes"][0]["message"], "Saved as \"docs-2\": the name \"docs\" is taken.");
    // VS Code's shape and a single object.
    let (_, out) = e.post("/api/mcp-servers/import", json!({"text": r#"{"servers": {"api": {"type": "http", "url": "https://a.example.com/mcp"}}}"#})).await;
    assert_eq!(out["created"][0]["name"], "api");
    assert_eq!(out["created"][0]["status"], "unchecked");
    let (_, out) = e.post("/api/mcp-servers/import", json!({"text": r#"{"command": "npx", "args": ["-y", "@acme/weather-mcp"]}"#})).await;
    assert_eq!(out["created"][0]["name"], "weather-mcp");
    // Text that holds no servers.
    let (status, err) = e.post("/api/mcp-servers/import", json!({"text": "{\"a\": 1}"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(err["fields"]["text"].as_str().unwrap().starts_with("No MCP servers found."));
    let (status, err) = e.post("/api/mcp-servers/import", json!({"text": "  "})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err["fields"]["text"], "Paste the JSON of one or more MCP servers.");
}

#[tokio::test]
async fn delete_is_refused_while_a_run_uses_the_server() {
    let e = env().await;
    let sid = id(&create(&e, json!({"name": "docs", "transport": "http", "url": "http://127.0.0.1:1/mcp"})).await);
    let (ps, _) = e.prompt_set("P", 1).await;
    let run = e.run_on(ps, "going", "m", None, "running").await;
    let snapshot = json!({"servers": [{"server_id": sid, "name": "docs", "transport": "http", "tools": []}], "definitions": []});
    sqlx::query("UPDATE runs SET tools = ? WHERE id = ?").bind(snapshot.to_string()).bind(run).execute(&e.ctx.db).await.unwrap();
    let (_, out) = e.get(&format!("/api/mcp-servers/{sid}")).await;
    assert_eq!(out["run_count"], 1);
    let (status, err) = e.delete(&format!("/api/mcp-servers/{sid}")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["detail"], "A run is using this MCP server. Pause it before deleting the server.");
    sqlx::query("UPDATE runs SET status = 'paused' WHERE id = ?").bind(run).execute(&e.ctx.db).await.unwrap();
    assert_eq!(e.delete(&format!("/api/mcp-servers/{sid}")).await.0, StatusCode::NO_CONTENT);
    assert_eq!(e.get(&format!("/api/mcp-servers/{sid}")).await.0, StatusCode::NOT_FOUND);
    // A run the engine reports as active counts too.
    let probe = Arc::new(ActiveRuns(vec![ActiveRun { run_id: 9, endpoint_id: None, prompt_set_id: 1, mcp_server_ids: vec![1] }]));
    let e2 = env_with(probe, Arc::new(Idle)).await;
    let sid = id(&create(&e2, json!({"name": "x", "transport": "http", "url": "http://127.0.0.1:1/mcp"})).await);
    assert_eq!(sid, 1);
    assert_eq!(e2.delete("/api/mcp-servers/1").await.0, StatusCode::CONFLICT);
}
