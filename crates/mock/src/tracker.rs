//! A second mock MCP server, an issue tracker, over streamable HTTP at `/tracker/mcp` on the
//! mock's port. It differs from the first server ([`crate::mcp`]) in the ways that matter to code
//! mode: it sends `instructions`, two of its tools declare an output schema and answer with
//! `structuredContent`, one answers with JSON text, and two names are not JavaScript identifiers
//! (`list-issues`) or are a reserved word (`delete`).
//!
//! The data is fixed: issues 1 to 500, the first twelve written out and the rest generated.
//! Nothing is stored: `create_issue` and `delete` answer as if they had worked.
//!
//! | Tool | Arguments | Result |
//! | --- | --- | --- |
//! | `list-issues` | `state` (`open`, `closed`, `all`; default `open`), `label`, `limit` (default 20, at most 500) | `structuredContent` `{issues: [{id, title, state, labels}], total}`, newest first |
//! | `get_issue` | `id` (integer, required) | JSON text: the issue with `body` and `comments`; an error result for an unknown id |
//! | `create_issue` | `title` (required), `body`, `labels` | `structuredContent` `{id, title, url}` |
//! | `search` | `query` (required) | JSON text: up to 10 `{id, title}` of the written-out issues whose title contains the query |
//! | `delete` | `id` (integer, required) | Text: `Deleted issue <id>.` |

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{json, Map, Value};

/// What the server sends as `instructions` when a client initializes.
pub const INSTRUCTIONS: &str =
    "A mock issue tracker. Issue ids are integers from 1 to 500. list-issues returns the newest issues first.";

/// The issues in the tracker.
pub const ISSUES: i64 = 500;

const WRITTEN: [(&str, bool, &[&str]); 12] = [
    ("Export fails on empty runs", true, &["bug", "export"]),
    ("Add a dark theme", true, &["feature", "ui"]),
    ("Retry button does nothing after a pause", true, &["bug"]),
    ("Document the code mode settings", false, &["docs"]),
    ("Tool results are cut mid-word", true, &["bug", "tools"]),
    ("Show token usage per sample", false, &["feature"]),
    ("Parquet export loses the tools column", false, &["bug", "export"]),
    ("Search prompts by text", true, &["feature", "ui"]),
    ("Crash when the endpoint returns HTML", false, &["bug"]),
    ("Keyboard shortcuts for review", true, &["feature", "ui"]),
    ("Docs: how to run the mock", false, &["docs"]),
    ("Rate limit retries are too eager", true, &["bug", "tools"]),
];

/// The mock tracker.
#[derive(Clone, Debug, Default)]
pub struct Tracker;

fn schema(v: Value) -> Arc<Map<String, Value>> {
    Arc::new(v.as_object().cloned().unwrap_or_default())
}

fn tools() -> Vec<Tool> {
    let issue = json!({"type": "object", "properties": {
        "id": {"type": "integer"}, "title": {"type": "string"},
        "state": {"type": "string", "enum": ["open", "closed"]},
        "labels": {"type": "array", "items": {"type": "string"}}
    }, "required": ["id", "title", "state"]});
    vec![
        Tool::new(
            "list-issues",
            "List issues, newest first. Filter by state or label.",
            schema(json!({"type": "object", "properties": {
                "state": {"type": "string", "enum": ["open", "closed", "all"], "description": "Which issues to list (default open)."},
                "label": {"type": "string", "description": "Only issues with this label."},
                "limit": {"type": "integer", "description": "At most this many (default 20, at most 500)."}
            }})),
        )
        .with_raw_output_schema(schema(json!({"type": "object", "properties": {
            "issues": {"type": "array", "items": issue}, "total": {"type": "integer", "description": "How many issues match."}
        }, "required": ["issues", "total"]}))),
        Tool::new(
            "get_issue",
            "Get one issue by id, with its body and comments.",
            schema(json!({"type": "object", "properties": {"id": {"type": "integer", "description": "The issue's id."}}, "required": ["id"]})),
        ),
        Tool::new(
            "create_issue",
            "Create an issue.",
            schema(json!({"type": "object", "properties": {
                "title": {"type": "string"}, "body": {"type": "string"},
                "labels": {"type": "array", "items": {"type": "string"}}
            }, "required": ["title"]})),
        )
        .with_raw_output_schema(schema(json!({"type": "object", "properties": {
            "id": {"type": "integer"}, "title": {"type": "string"}, "url": {"type": "string"}
        }, "required": ["id", "url"]}))),
        Tool::new(
            "search",
            "Search issue titles.",
            schema(json!({"type": "object", "properties": {"query": {"type": "string", "description": "Words to look for in titles."}}, "required": ["query"]})),
        ),
        Tool::new(
            "delete",
            "Delete an issue.",
            schema(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "required": ["id"]})),
        ),
    ]
}

fn title(id: i64) -> String {
    match WRITTEN.get((id - 1) as usize) {
        Some((t, _, _)) => t.to_string(),
        None => format!("Generated issue {id} about {}", ["imports", "exports", "runs", "review", "tools"][(id % 5) as usize]),
    }
}

fn open(id: i64) -> bool {
    match WRITTEN.get((id - 1) as usize) {
        Some((_, open, _)) => *open,
        None => id % 3 != 0,
    }
}

fn labels(id: i64) -> Vec<&'static str> {
    match WRITTEN.get((id - 1) as usize) {
        Some((_, _, l)) => l.to_vec(),
        None => vec!["generated", ["bug", "feature", "docs"][(id % 3) as usize]],
    }
}

fn summary(id: i64) -> Value {
    json!({"id": id, "title": title(id), "state": if open(id) { "open" } else { "closed" }, "labels": labels(id)})
}

fn text(s: impl Into<String>) -> Result<CallToolResponse, ErrorData> {
    Ok(CallToolResponse::Complete(CallToolResult::success(vec![ContentBlock::text(s.into())])))
}

fn error(s: impl Into<String>) -> Result<CallToolResponse, ErrorData> {
    Ok(CallToolResponse::Complete(CallToolResult::error(vec![ContentBlock::text(s.into())])))
}

fn structured(v: Value) -> Result<CallToolResponse, ErrorData> {
    Ok(CallToolResponse::Complete(CallToolResult::structured(v)))
}

fn integer(args: &Map<String, Value>, key: &str) -> Option<i64> {
    match args.get(key)? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

impl Tracker {
    /// Run one tool.
    pub fn run(&self, name: &str, args: &Map<String, Value>) -> Result<CallToolResponse, ErrorData> {
        match name {
            "list-issues" => {
                let state = args.get("state").and_then(Value::as_str).unwrap_or("open");
                if !["open", "closed", "all"].contains(&state) {
                    return error(format!("Unknown state \"{state}\". Use open, closed or all."));
                }
                let label = args.get("label").and_then(Value::as_str).filter(|l| !l.is_empty());
                let limit = integer(args, "limit").unwrap_or(20).clamp(0, ISSUES) as usize;
                let matching: Vec<i64> = (1..=ISSUES)
                    .rev()
                    .filter(|id| state == "all" || open(*id) == (state == "open"))
                    .filter(|id| label.is_none_or(|l| labels(*id).contains(&l)))
                    .collect();
                let issues: Vec<Value> = matching.iter().take(limit).map(|id| summary(*id)).collect();
                structured(json!({"issues": issues, "total": matching.len()}))
            }
            "get_issue" => match integer(args, "id") {
                None => error("Missing argument: id."),
                Some(id) if !(1..=ISSUES).contains(&id) => error(format!("Issue {id} not found.")),
                Some(id) => {
                    let mut issue = summary(id);
                    issue["body"] = json!(format!("Steps to reproduce issue {id}, and what was expected instead."));
                    let comments: Vec<Value> = (0..id % 3)
                        .map(|k| {
                            let author = ["ana", "ben", "chloe"][((id + k) % 3) as usize];
                            json!({"author": author, "text": format!("Comment {} on issue {id}.", k + 1)})
                        })
                        .collect();
                    issue["comments"] = Value::Array(comments);
                    text(issue.to_string())
                }
            },
            "create_issue" => match args.get("title").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()) {
                None => error("Missing argument: title."),
                Some(t) => {
                    let id = ISSUES + 1 + (t.chars().count() as i64 % 100);
                    structured(json!({"id": id, "title": t, "url": format!("https://tracker.example/issues/{id}")}))
                }
            },
            "search" => match args.get("query").and_then(Value::as_str).map(str::trim).filter(|q| !q.is_empty()) {
                None => error("Missing argument: query."),
                Some(q) => {
                    let q = q.to_lowercase();
                    let hits: Vec<Value> = (1..=WRITTEN.len() as i64)
                        .filter(|id| title(*id).to_lowercase().contains(&q))
                        .take(10)
                        .map(|id| json!({"id": id, "title": title(id)}))
                        .collect();
                    text(Value::Array(hits).to_string())
                }
            },
            "delete" => match integer(args, "id") {
                None => error("Missing argument: id."),
                Some(id) if !(1..=ISSUES).contains(&id) => error(format!("Issue {id} not found.")),
                Some(id) => text(format!("Deleted issue {id}.")),
            },
            other => Err(ErrorData::invalid_params(format!("Unknown tool: {other}"), None)),
        }
    }
}

impl ServerHandler for Tracker {
    fn get_info(&self) -> ServerConfig {
        let mut c = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        c.server_info = Implementation::new("stellarator-mock-tracker", env!("CARGO_PKG_VERSION"));
        c.instructions = Some(INSTRUCTIONS.to_string());
        c
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.run(&request.name, &request.arguments.clone().unwrap_or_default())
    }
}

/// The streamable HTTP service, to mount at `/tracker/mcp`.
pub fn http_service() -> StreamableHttpService<Tracker, LocalSessionManager> {
    StreamableHttpService::new(
        || Ok(Tracker),
        Default::default(),
        StreamableHttpServerConfig::default().disable_allowed_hosts().with_sse_keep_alive(None),
    )
}
