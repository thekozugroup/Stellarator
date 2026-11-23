//! The mock MCP server through Stellarator's MCP client: streamable HTTP in-process, and stdio
//! with the built `stellarator-mock mcp-stdio`, including a crash and the reconnect.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

use serde_json::{json, Map, Value};
use stellarator_core::schemas::McpTransport;
use stellarator_mcp::{CallOutcome, Connection, ServerConfig, Session};

const T: Duration = Duration::from_secs(20);

fn args(v: Value) -> Option<Map<String, Value>> {
    v.as_object().cloned()
}

async fn http_server(api_key: Option<&str>) -> (String, tokio::task::JoinHandle<()>) {
    let (addr, join) =
        stellarator_mock::serve_with_key(SocketAddr::from(([127, 0, 0, 1], 0)), 400.0, api_key.map(str::to_string))
            .await
            .unwrap();
    (format!("http://{addr}/mcp"), join)
}

fn http_config(url: &str, headers: &[(&str, &str)]) -> ServerConfig {
    ServerConfig {
        id: 1,
        name: "mock".into(),
        transport: McpTransport::Http,
        url: Some(url.into()),
        headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        ..ServerConfig::default()
    }
}

fn stdio_config(env: &[(&str, &str)]) -> ServerConfig {
    ServerConfig {
        id: 2,
        name: "mock-stdio".into(),
        transport: McpTransport::Stdio,
        command: Some(env!("CARGO_BIN_EXE_stellarator-mock").into()),
        args: vec!["mcp-stdio".into()],
        env: env.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>(),
        ..ServerConfig::default()
    }
}

fn answered(o: CallOutcome) -> (String, bool) {
    match o {
        CallOutcome::Answered { text, is_error } => (text, is_error),
        CallOutcome::Transport(e) => panic!("transport failure: {e}"),
    }
}

#[tokio::test]
async fn http_tools_list_and_call() {
    let (url, _server) = http_server(None).await;
    let conn = Connection::connect(&http_config(&url, &[]), T).await.unwrap();
    assert_eq!(conn.server_info.as_deref(), Some(concat!("stellarator-mock-tools ", env!("CARGO_PKG_VERSION"))));
    let tools = conn.list_tools(T).await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["lookup", "add", "fail", "slow", "chart", "repeat"]);
    assert_eq!(tools[0].input_schema["required"], json!(["query"]));
    assert!(tools[0].description.as_deref().unwrap().starts_with("Look up"));

    let (text, err) = answered(conn.call("lookup", args(json!({"query": "tide tables"})), T).await);
    assert_eq!((text.as_str(), err), ("Catalog entry for \"tide tables\": filed under section 5, last reviewed in 2026.", false));
    assert_eq!(answered(conn.call("add", args(json!({"a": 2, "b": 2.5})), T).await), ("4.5".into(), false));
    assert_eq!(answered(conn.call("fail", None, T).await), ("The lookup failed: the record is locked.".into(), true));
    assert_eq!(
        answered(conn.call("chart", None, T).await),
        ("Chart of the data:\n[image: image/png, 68 bytes]".into(), false)
    );
    assert_eq!(answered(conn.call("repeat", args(json!({"text": "ab", "times": 3})), T).await).0, "ababab");
    // An unknown tool is a protocol error from the server: still an answer for the model.
    let (text, err) = answered(conn.call("nope", None, T).await);
    assert!(err && text.starts_with("The server could not run the tool: Unknown tool: nope"), "{text}");
    // A call slower than its timeout is an error answer.
    let (text, err) = answered(conn.call("slow", args(json!({"seconds": 3})), Duration::from_millis(300)).await);
    assert_eq!((text.as_str(), err), ("The tool did not finish within 0.3 s.", true));
    conn.close().await;
}

#[tokio::test]
async fn http_headers_reach_the_server() {
    let (url, _server) = http_server(Some("sk-mcp")).await;
    let err = Connection::connect(&http_config(&url, &[]), T).await.err().expect("no key, no session");
    assert!(err.0.contains("Couldn't open an MCP session"), "{}", err.0);
    let conn = Connection::connect(&http_config(&url, &[("Authorization", "Bearer sk-mcp")]), T).await.unwrap();
    assert_eq!(conn.list_tools(T).await.unwrap().len(), 6);
    conn.close().await;
}

#[tokio::test]
async fn http_errors_are_sentences() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("http://{}/mcp", listener.local_addr().unwrap());
    drop(listener);
    let err = Connection::connect(&http_config(&dead, &[]), T).await.err().unwrap();
    assert!(err.0.contains("nothing answered"), "{}", err.0);
}

#[tokio::test]
async fn stdio_env_and_tools() {
    let conn = Connection::connect(&stdio_config(&[("MOCK_SECRET", "s3cret-value")]), T).await.unwrap();
    let names: Vec<String> = conn.list_tools(T).await.unwrap().into_iter().map(|t| t.name).collect();
    assert!(names.contains(&"crash".to_string()) && names.contains(&"getenv".to_string()), "{names:?}");
    assert_eq!(answered(conn.call("getenv", args(json!({"name": "MOCK_SECRET"})), T).await), ("s3cret-value".into(), false));
    conn.close().await;
    assert!(conn.is_closed());
}

#[tokio::test]
async fn stdio_missing_command_and_failed_start() {
    let mut cfg = stdio_config(&[]);
    cfg.command = Some("stellarator-no-such-command-xyz".into());
    let err = Connection::connect(&cfg, T).await.err().unwrap();
    assert_eq!(err.0, "Couldn't find the command \"stellarator-no-such-command-xyz\". Install it, or enter its full path.");
    // A program that exits at once: the handshake fails.
    let mut cfg = stdio_config(&[]);
    cfg.args = vec!["--no-such-flag".into()];
    let err = Connection::connect(&cfg, T).await.err().unwrap();
    assert!(err.0.contains("did not complete the MCP handshake"), "{}", err.0);
    assert!(err.0.contains("Couldn't read the argument"), "the error output is quoted: {}", err.0);
}

#[tokio::test]
async fn stdio_crash_reconnects_once() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("crashed");
    let session = Session::open(stdio_config(&[]), T).await.unwrap();
    // The first process exits on the call; the session starts a new one, which answers.
    let answer = session.call("crash", args(json!({"marker": marker.to_str().unwrap()})), T).await.unwrap();
    assert_eq!(answer.text, "Recovered: this server process started after a crash.");
    assert!(marker.exists());
    // The new process serves later calls.
    assert_eq!(session.call("add", args(json!({"a": 1, "b": 1})), T).await.unwrap().text, "2");
    // A server that crashes again after the reconnect fails the call.
    let err = session.call("crash", None, T).await.unwrap_err();
    assert!(err.starts_with("The MCP server \"mock-stdio\" stopped answering"), "{err}");
    session.close().await;
    let err = session.call("add", args(json!({"a": 1, "b": 1})), T).await.unwrap_err();
    assert!(err.contains("was closed"), "{err}");
}
