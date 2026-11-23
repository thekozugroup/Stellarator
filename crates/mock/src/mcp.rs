//! A tiny MCP server with a few tools, over streamable HTTP (`/mcp` on the mock's port) or stdio
//! (`stellarator-mock mcp-stdio`), built on the official Rust SDK (rmcp).
//!
//! | Tool | Arguments | Result |
//! | --- | --- | --- |
//! | `lookup` | `query` (string, required) | A short text naming the query |
//! | `add` | `a`, `b` (numbers, required) | The sum, as text |
//! | `fail` | `reason` (string) | An error result (`isError: true`) |
//! | `slow` | `seconds` (number, default 2, at most 30) | Waits, then says how long |
//! | `chart` | none | A text part and a small PNG image part |
//! | `repeat` | `text` (string), `times` (integer, at most 100000) | `text` repeated, for long results |
//! | `crash` | `marker` (string, a file path) | stdio only: the process exits. With `marker`, only when the file does not exist yet (it is created first), so a restarted server answers |
//! | `getenv` | `name` (string, required) | stdio only: the value of an environment variable of the process |

use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Map, Value};

/// A 1x1 transparent PNG.
const PIXEL_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";

/// The mock MCP server. `stdio`: offer `crash` too.
#[derive(Clone, Debug, Default)]
pub struct MockTools {
    pub stdio: bool,
}

fn schema(v: Value) -> Arc<Map<String, Value>> {
    Arc::new(v.as_object().cloned().unwrap_or_default())
}

fn tools(stdio: bool) -> Vec<Tool> {
    let mut t = vec![
        Tool::new(
            "lookup",
            "Look up a term in the reference catalog and return what it says.",
            schema(json!({"type": "object", "properties": {"query": {"type": "string", "description": "The term to look up."}},
                          "required": ["query"]})),
        ),
        Tool::new(
            "add",
            "Add two numbers.",
            schema(json!({"type": "object", "properties": {"a": {"type": "number"}, "b": {"type": "number"}}, "required": ["a", "b"]})),
        ),
        Tool::new(
            "fail",
            "Always fails, with an error result.",
            schema(json!({"type": "object", "properties": {"reason": {"type": "string"}}})),
        ),
        Tool::new(
            "slow",
            "Waits some seconds before answering.",
            schema(json!({"type": "object", "properties": {"seconds": {"type": "number", "description": "Seconds to wait (default 2)."}}})),
        ),
        Tool::new("chart", "Draws a chart of the data.", schema(json!({"type": "object", "properties": {}}))),
        Tool::new(
            "repeat",
            "Repeats a text.",
            schema(json!({"type": "object", "properties": {"text": {"type": "string"}, "times": {"type": "integer"}},
                          "required": ["text", "times"]})),
        ),
    ];
    if stdio {
        t.push(Tool::new(
            "crash",
            "Makes the server process exit (to test reconnecting).",
            schema(json!({"type": "object", "properties": {"marker": {"type": "string"}}})),
        ));
        t.push(Tool::new(
            "getenv",
            "Reads an environment variable of the server process.",
            schema(json!({"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]})),
        ));
    }
    t
}

fn text(s: impl Into<String>) -> Result<CallToolResponse, ErrorData> {
    Ok(CallToolResponse::Complete(CallToolResult::success(vec![ContentBlock::text(s.into())])))
}

fn error(s: impl Into<String>) -> Result<CallToolResponse, ErrorData> {
    Ok(CallToolResponse::Complete(CallToolResult::error(vec![ContentBlock::text(s.into())])))
}

fn number(args: &Map<String, Value>, key: &str) -> Option<f64> {
    match args.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A number as `{:g}` would print it: 4 -> "4", 2.5 -> "2.5".
fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

impl MockTools {
    async fn run(&self, name: &str, args: Map<String, Value>) -> Result<CallToolResponse, ErrorData> {
        match name {
            "lookup" => match args.get("query").and_then(Value::as_str).map(str::trim).filter(|q| !q.is_empty()) {
                None => error("Missing argument: query."),
                Some(q) => {
                    let n = q.chars().count() % 7 + 1;
                    text(format!("Catalog entry for \"{q}\": filed under section {n}, last reviewed in 2026."))
                }
            },
            "add" => match (number(&args, "a"), number(&args, "b")) {
                (Some(a), Some(b)) => text(fmt_num(a + b)),
                _ => error("Both a and b must be numbers."),
            },
            "fail" => {
                let reason = args.get("reason").and_then(Value::as_str).filter(|r| !r.is_empty()).unwrap_or("the record is locked");
                error(format!("The lookup failed: {reason}."))
            }
            "slow" => {
                let secs = number(&args, "seconds").unwrap_or(2.0).clamp(0.0, 30.0);
                tokio::time::sleep(Duration::from_secs_f64(secs)).await;
                text(format!("Waited {} s.", fmt_num(secs)))
            }
            "chart" => Ok(CallToolResponse::Complete(CallToolResult::success(vec![
                ContentBlock::text("Chart of the data:"),
                ContentBlock::image(PIXEL_PNG, "image/png"),
            ]))),
            "repeat" => {
                let t = args.get("text").and_then(Value::as_str).unwrap_or("");
                let times = number(&args, "times").unwrap_or(1.0).clamp(0.0, 100_000.0) as usize;
                text(t.repeat(times))
            }
            "crash" if self.stdio => {
                if let Some(marker) = args.get("marker").and_then(Value::as_str).filter(|m| !m.is_empty()) {
                    if std::path::Path::new(marker).exists() {
                        return text("Recovered: this server process started after a crash.");
                    }
                    let _ = std::fs::write(marker, b"crashed");
                }
                std::process::exit(3);
            }
            "getenv" if self.stdio => {
                let name = args.get("name").and_then(Value::as_str).unwrap_or("");
                match std::env::var(name) {
                    Ok(v) => text(v),
                    Err(_) => error(format!("{name} is not set.")),
                }
            }
            other => Err(ErrorData::invalid_params(format!("Unknown tool: {other}"), None)),
        }
    }
}

impl ServerHandler for MockTools {
    fn get_info(&self) -> ServerConfig {
        let mut c = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        c.server_info = Implementation::new("stellarator-mock-tools", env!("CARGO_PKG_VERSION"));
        c
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(tools(self.stdio)))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.run(&request.name, request.arguments.clone().unwrap_or_default()).await
    }
}

/// The streamable HTTP service, to mount at `/mcp`. Any Host header is accepted: in Docker the
/// mock is reached as `mock:9100`.
pub fn http_service() -> StreamableHttpService<MockTools, LocalSessionManager> {
    StreamableHttpService::new(
        || Ok(MockTools { stdio: false }),
        Default::default(),
        StreamableHttpServerConfig::default().disable_allowed_hosts().with_sse_keep_alive(None),
    )
}

/// Serve the tools on stdin and stdout until the client goes away.
pub async fn serve_stdio() -> Result<(), String> {
    let running = MockTools { stdio: true }.serve(rmcp::transport::stdio()).await.map_err(|e| e.to_string())?;
    running.waiting().await.map_err(|e| e.to_string())?;
    Ok(())
}
