//! The tool loop of a run with MCP tools.
//!
//! [`ToolRuntime`] holds the run's MCP sessions (one per server, shared by every sample, opened
//! when the run starts or resumes and closed when it pauses, finishes or is deleted). [`run_sample`]
//! sends one sample: request, run the tool calls (in parallel, at most [`PARALLEL_CALLS`] at a
//! time), send the results back, and repeat until an answer without tool calls or `max_rounds`.
//!
//! Rules:
//! - Earlier reasoning is not sent back to the model; it is kept in the trace.
//! - Arguments that are not a JSON object, an unknown tool, a tool's error result and a call that
//!   times out go back to the model as the tool's result (`is_error`): they are part of the trace.
//! - A connection to an MCP server that fails reconnects once (see `stellarator_mcp::Session`);
//!   when that fails too, the sample fails.
//! - An answer that still calls tools after `max_rounds` rounds ends the sample as truncated, with
//!   finish reason `tool_rounds`; those calls are not run.
//! - Token usage is summed over the sample's requests.
//!
//! Code mode (`params.tools.mode` = `code`): the request's one tool is the code tool; each of its
//! calls runs the model's JavaScript in a sandbox (`stellarator_codemode::execute`), whose
//! namespace methods call the MCP tools through [`CodeHost`]. The tool message is what Cloudflare's
//! Code Mode sends the model; the trace also keeps what happened inside (`TraceTool.code`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::StreamExt;
use parking_lot::Mutex;
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;
use stellarator_codemode::{execute, unknown_tool_message, unwrap_mcp_result, CallAnswer, Host, Limits};
use stellarator_core::config::STDIO_MCP_BLOCKED;
use stellarator_core::schemas::{
    LiveCode, McpTransport, Phase, RunParams, RunTools, ToolCall, ToolChoice, ToolMode, ToolSnapshot, TraceAssistant,
    TraceMessage, TraceTool,
};
use stellarator_mcp::result::truncate;
use stellarator_mcp::store::CHECK_TIMEOUT;
use stellarator_mcp::{RawOutcome, Session};

use crate::generation::{
    generate_round, Backoff, GenerateError, GenerationResult, Progress, ProgressSink, RequestTools, ServerCaps,
};

/// Tool calls of one answer run at the same time, at most this many.
pub const PARALLEL_CALLS: usize = 8;
/// The finish reason of a sample that hit `max_rounds` with tool calls left.
pub const TOOL_ROUNDS: &str = "tool_rounds";

struct Route {
    session: Arc<Session>,
    server: String,
    server_tool: String,
}

/// A run's open MCP sessions and the routes from tool names to them.
pub struct ToolRuntime {
    pub snapshot: ToolSnapshot,
    pub settings: RunTools,
    routes: HashMap<String, Route>,
    /// Code mode: namespace, then method.
    code_routes: HashMap<String, HashMap<String, Route>>,
    sessions: Vec<Arc<Session>>,
}

impl ToolRuntime {
    /// Open a session with every server of `snapshot`, with each server's saved settings. Err is
    /// the reason the run cannot go on.
    pub async fn open(
        db: &SqlitePool,
        snapshot: ToolSnapshot,
        settings: RunTools,
        allow_stdio: bool,
    ) -> Result<ToolRuntime, String> {
        let mut configs = Vec::new();
        for s in &snapshot.servers {
            let row = stellarator_mcp::store::fetch(db, s.server_id).await.map_err(|e| e.to_string())?;
            let Some(row) = row else {
                return Err(format!(
                    "The MCP server \"{}\" this run uses was deleted. Add it again and start a new run with it, or \
                     start one with other servers.",
                    s.name
                ));
            };
            if !row.enabled {
                return Err(format!("The MCP server \"{}\" is turned off. Turn it on, then resume.", row.name));
            }
            if row.transport() == McpTransport::Stdio && !allow_stdio {
                return Err(STDIO_MCP_BLOCKED.to_string());
            }
            configs.push(row.config());
        }
        let opened =
            futures::future::join_all(configs.into_iter().map(|c| async move {
                let name = c.name.clone();
                Session::open(c, CHECK_TIMEOUT)
                    .await
                    .map_err(|e| format!("Couldn't start the MCP server \"{name}\": {} Resume to try again.", e.0.trim_end()))
            }))
            .await;
        let mut sessions = Vec::new();
        let mut failure = None;
        for o in opened {
            match o {
                Ok(s) => sessions.push(s),
                Err(e) => failure = failure.or(Some(e)),
            }
        }
        if let Some(e) = failure {
            for s in &sessions {
                s.close().await;
            }
            return Err(e);
        }
        let mut routes = HashMap::new();
        for (server, session) in snapshot.servers.iter().zip(&sessions) {
            for t in &server.tools {
                routes.insert(
                    t.name.clone(),
                    Route { session: session.clone(), server: server.name.clone(), server_tool: t.server_tool.clone() },
                );
            }
        }
        let by_id: HashMap<i64, &Arc<Session>> = snapshot.servers.iter().map(|s| s.server_id).zip(&sessions).collect();
        let mut code_routes: HashMap<String, HashMap<String, Route>> = HashMap::new();
        for ns in snapshot.code.iter().flat_map(|c| &c.namespaces) {
            for m in &ns.methods {
                if let Some(session) = by_id.get(&m.server_id) {
                    code_routes.entry(ns.name.clone()).or_default().insert(
                        m.name.clone(),
                        Route { session: (*session).clone(), server: m.server.clone(), server_tool: m.tool.clone() },
                    );
                }
            }
        }
        Ok(ToolRuntime { snapshot, settings, routes, code_routes, sessions })
    }

    /// True when the run uses code mode.
    pub fn code_mode(&self) -> bool {
        self.snapshot.mode == ToolMode::Code && self.snapshot.code.is_some()
    }

    fn tool_timeout(&self) -> Duration {
        Duration::from_secs_f64(self.settings.tool_timeout_s.max(0.001))
    }

    /// Code mode: run one answer's calls of the code tool, at most [`PARALLEL_CALLS`] at a time.
    /// Err only when an MCP server's connection failed even after a reconnect.
    pub async fn run_code_calls(
        &self,
        calls: &[ToolCall],
        round: i64,
        sink: &dyn ProgressSink,
    ) -> Result<Vec<TraceTool>, String> {
        let Some(code) = self.snapshot.code.as_ref() else { return Ok(Vec::new()) };
        let options = self.settings.code.clone().unwrap_or_default();
        let budget = (options.max_result_tokens > 0)
            .then(|| (options.max_result_chars.unwrap_or(options.max_result_tokens * 4), options.max_result_tokens));
        let limits = Limits {
            timeout: Duration::from_secs_f64(options.timeout_s.max(0.001)),
            memory_bytes: (options.memory_limit_mb.max(1) as usize) << 20,
            budget,
            store_chars: self.settings.max_result_chars.max(1) as usize,
        };
        let namespaces: Vec<String> = code.namespaces.iter().map(|n| n.name.clone()).collect();
        let host = CodeHost { rt: self };
        // Each execution's calls, summed for the live view.
        let live = Mutex::new(vec![LiveCode::default(); calls.len()]);
        let report = |i: usize, l: &LiveCode| {
            let all = {
                let mut slots = live.lock();
                if let Some(slot) = slots.get_mut(i) {
                    *slot = l.clone();
                }
                LiveCode {
                    calls: slots.iter().map(|s| s.calls).sum(),
                    running: slots.iter().flat_map(|s| s.running.iter().cloned()).collect(),
                }
            };
            sink.on_reset(&Progress {
                phase: Phase::Code,
                round,
                tools: vec![code.tool_name.clone()],
                code: Some(all),
                ..Progress::default()
            });
        };
        report(0, &LiveCode::default());
        let (limits, namespaces, host, report) = (&limits, &namespaces, &host, &report);
        // Built first, as in run_sample: a closure inside the stream would not be Send.
        let runs: Vec<BoxFuture<'_, Result<TraceTool, String>>> = calls
            .iter()
            .enumerate()
            .map(|(i, c)| -> BoxFuture<'_, Result<TraceTool, String>> {
                Box::pin(async move {
                    let started = Instant::now();
                    if c.name != code.tool_name {
                        return Ok(TraceTool {
                            tool_call_id: c.id.clone(),
                            name: c.name.clone(),
                            server: String::new(),
                            content: unknown_tool_message(&c.name, &code.tool_name),
                            is_error: true,
                            truncated: false,
                            latency_ms: 0,
                            code: None,
                        });
                    }
                    let progress = move |l: &LiveCode| report(i, l);
                    let e = execute(&code.tool_name, &c.arguments, namespaces, limits, host, &progress).await?;
                    Ok(TraceTool {
                        tool_call_id: c.id.clone(),
                        name: c.name.clone(),
                        server: String::new(),
                        content: e.content,
                        is_error: e.is_error,
                        truncated: e.truncated,
                        latency_ms: started.elapsed().as_millis() as i64,
                        code: e.detail,
                    })
                })
            })
            .collect();
        futures::stream::iter(runs).buffered(PARALLEL_CALLS).collect::<Vec<_>>().await.into_iter().collect()
    }

    /// End every session (stops stdio servers).
    pub async fn close(&self) {
        futures::future::join_all(self.sessions.iter().map(|s| s.close())).await;
    }

    /// Run one call. Err only when the server's connection failed even after a reconnect.
    pub async fn call(&self, call: &ToolCall) -> Result<TraceTool, String> {
        let started = Instant::now();
        let timeout = self.tool_timeout();
        let (server, text, is_error) = match self.routes.get(&call.name) {
            None => {
                let mut names: Vec<&str> = self.routes.keys().map(String::as_str).collect();
                names.sort_unstable();
                (String::new(), format!("There is no tool named \"{}\". The tools are: {}.", call.name, names.join(", ")), true)
            }
            Some(route) => match parse_arguments(&call.arguments) {
                Err(e) => (route.server.clone(), e, true),
                Ok(args) => {
                    let answer = route.session.call(&route.server_tool, args, timeout).await?;
                    (route.server.clone(), answer.text, answer.is_error)
                }
            },
        };
        let (content, truncated) = truncate(text, self.settings.max_result_chars.max(1) as usize);
        Ok(TraceTool {
            tool_call_id: call.id.clone(),
            name: call.name.clone(),
            server,
            content,
            is_error,
            truncated,
            latency_ms: started.elapsed().as_millis() as i64,
            code: None,
        })
    }
}

/// The namespaces' side of code mode: each method calls its MCP tool on the run's session, and the
/// code gets what Cloudflare's MCP integration gives it (`unwrapMcpResult`); failures throw with
/// the MCP TypeScript SDK's messages (`MCP error <code>: <message>`).
pub struct CodeHost<'a> {
    rt: &'a ToolRuntime,
}

impl CodeHost<'_> {
    fn route(&self, namespace: &str, method: &str) -> Option<&Route> {
        self.rt.code_routes.get(namespace).and_then(|m| m.get(method))
    }
}

impl Host for CodeHost<'_> {
    fn route(&self, namespace: &str, method: &str) -> Option<(String, String)> {
        CodeHost::route(self, namespace, method).map(|r| (r.server.clone(), r.server_tool.clone()))
    }

    fn call(&self, namespace: String, method: String, arguments: Option<Map<String, Value>>) -> BoxFuture<'_, CallAnswer> {
        Box::pin(async move {
            let Some(route) = CodeHost::route(self, &namespace, &method) else {
                return CallAnswer::Thrown(format!("Tool \"{method}\" not found"));
            };
            match route.session.call_raw(&route.server_tool, arguments, self.rt.tool_timeout()).await {
                Err(e) => CallAnswer::Fatal(e),
                Ok(RawOutcome::Result(v)) => match unwrap_mcp_result(&v) {
                    Ok(value) => CallAnswer::Value(value),
                    Err(message) => CallAnswer::Thrown(message),
                },
                Ok(RawOutcome::TimedOut(_)) => CallAnswer::Thrown("MCP error -32001: Request timed out".into()),
                Ok(RawOutcome::ServerError { code, message }) => CallAnswer::Thrown(format!("MCP error {code}: {message}")),
                Ok(RawOutcome::Failed(text)) => CallAnswer::Thrown(text),
                Ok(RawOutcome::Transport(e)) => CallAnswer::Fatal(e),
            }
        })
    }
}

/// The arguments the model wrote, as the JSON object a tool takes. Err is the message for the model.
pub fn parse_arguments(raw: &str) -> Result<Option<Map<String, Value>>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(m)) => Ok(Some(m)),
        Ok(Value::Null) => Ok(None),
        Ok(_) => Err("The arguments must be a JSON object of the tool's parameters.".into()),
        Err(e) => Err(format!(
            "The arguments are not valid JSON ({e}). Send them as one JSON object of the tool's parameters."
        )),
    }
}

/// The assistant message sent back with the next request: the answer's text (without its
/// reasoning) and its tool calls.
pub fn assistant_request_message(content: &str, calls: &[ToolCall]) -> Value {
    let calls: Vec<Value> = calls
        .iter()
        .map(|c| json!({"id": c.id, "type": "function", "function": {"name": c.name, "arguments": c.arguments}}))
        .collect();
    json!({"role": "assistant", "content": content, "tool_calls": calls})
}

/// A sample run through the tool loop.
#[derive(Clone, Debug)]
pub struct SampleOutcome {
    /// The last answer's reasoning, response and finish reason (`tool_rounds` when the rounds ran
    /// out), with the token usage, attempts and latency of the whole sample.
    pub result: GenerationResult,
    pub trace: Vec<TraceMessage>,
    /// Tool calls run (code mode: executions).
    pub tool_calls: i64,
    /// Code mode: the MCP tool calls the code made.
    pub inner_tool_calls: i64,
}

fn add_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
    }
}

/// Send one sample with tools: request, run the calls, send the results, until an answer without
/// tool calls or `max_rounds` rounds of calls.
#[allow(clippy::too_many_arguments)]
pub async fn run_sample(
    rt: &ToolRuntime,
    client: &reqwest::Client,
    url: &str,
    model: &str,
    prepared: &[Value],
    params: &RunParams,
    sample_idx: i64,
    caps: &ServerCaps,
    sink: &dyn ProgressSink,
    backoff: &dyn Backoff,
) -> Result<SampleOutcome, GenerateError> {
    let started = Instant::now();
    let max_rounds = rt.settings.max_rounds.max(1);
    let mut convo: Vec<Value> = prepared.to_vec();
    let mut trace: Vec<TraceMessage> = Vec::new();
    let mut executed: i64 = 0;
    let mut inner: i64 = 0;
    let (mut prompt_tokens, mut completion_tokens, mut reasoning_tokens) = (None, None, None);
    let (mut retries, mut chunks, mut ttft_ms) = (0_i64, 0_i64, None);
    let mut round: i64 = 1;
    loop {
        let choice = if round == 1 && rt.settings.tool_choice == ToolChoice::Required { "required" } else { "auto" };
        let tools = RequestTools { definitions: &rt.snapshot.definitions, choice };
        let r = generate_round(client, url, model, &convo, params, sample_idx, caps, sink, backoff, Some(tools), round)
            .await
            .map_err(|e| match e {
                GenerateError::Failed { message, attempts } => GenerateError::Failed { message, attempts: retries + attempts },
                other => other,
            })?;
        prompt_tokens = add_opt(prompt_tokens, r.prompt_tokens);
        completion_tokens = add_opt(completion_tokens, r.completion_tokens);
        reasoning_tokens = add_opt(reasoning_tokens, r.reasoning_tokens);
        retries += r.attempts - 1;
        chunks += r.chunks;
        if round == 1 {
            ttft_ms = r.ttft_ms;
        }
        trace.push(TraceMessage::Assistant(TraceAssistant {
            reasoning: r.reasoning.clone(),
            content: r.response.clone(),
            tool_calls: r.tool_calls.clone(),
            finish_reason: r.finish_reason.clone(),
            prompt_tokens: r.prompt_tokens,
            completion_tokens: r.completion_tokens,
            reasoning_tokens: r.reasoning_tokens,
            latency_ms: r.latency_ms,
        }));
        let cut_short = r.finish_reason.as_deref() == Some("length");
        let out_of_rounds = round > max_rounds;
        if r.tool_calls.is_empty() || cut_short || out_of_rounds {
            let finish_reason =
                if !r.tool_calls.is_empty() && !cut_short { Some(TOOL_ROUNDS.to_string()) } else { r.finish_reason.clone() };
            let result = GenerationResult {
                finish_reason,
                prompt_tokens,
                completion_tokens,
                reasoning_tokens,
                latency_ms: started.elapsed().as_millis() as i64,
                ttft_ms,
                attempts: retries + 1,
                chunks,
                ..r
            };
            return Ok(SampleOutcome { result, trace, tool_calls: executed, inner_tool_calls: inner });
        }
        convo.push(assistant_request_message(&r.response, &r.tool_calls));
        if rt.code_mode() {
            let results = rt
                .run_code_calls(&r.tool_calls, round, sink)
                .await
                .map_err(|message| GenerateError::Failed { message, attempts: retries + 1 })?;
            for t in results {
                inner += t.code.as_ref().map_or(0, |c| c.calls.len() as i64);
                convo.push(json!({"role": "tool", "tool_call_id": t.tool_call_id, "content": t.content}));
                trace.push(TraceMessage::Tool(t));
                executed += 1;
            }
            round += 1;
            continue;
        }
        sink.on_reset(&Progress {
            phase: Phase::Tool,
            round,
            tools: r.tool_calls.iter().map(|c| c.name.clone()).collect(),
            ..Progress::default()
        });
        // The futures are built first: a closure inside the stream would not be Send for every
        // lifetime, which the worker's tokio::spawn needs.
        let calls: Vec<_> = r.tool_calls.iter().map(|c| Box::pin(rt.call(c))).collect();
        let results: Vec<Result<TraceTool, String>> =
            futures::stream::iter(calls).buffered(PARALLEL_CALLS).collect().await;
        for res in results {
            let t = res.map_err(|message| GenerateError::Failed { message, attempts: retries + 1 })?;
            convo.push(json!({"role": "tool", "tool_call_id": t.tool_call_id, "content": t.content}));
            trace.push(TraceMessage::Tool(t));
            executed += 1;
        }
        round += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_parse_or_explain() {
        assert_eq!(parse_arguments(""), Ok(None));
        assert_eq!(parse_arguments(" null "), Ok(None));
        assert_eq!(parse_arguments(r#"{"q": "x"}"#).unwrap().unwrap()["q"], "x");
        assert!(parse_arguments("[1]").unwrap_err().contains("must be a JSON object"));
        let e = parse_arguments(r#"{"query": "unterminated"#).unwrap_err();
        assert!(e.starts_with("The arguments are not valid JSON (EOF while parsing"), "{e}");
    }

    #[test]
    fn request_messages_carry_no_reasoning() {
        let calls = vec![ToolCall { id: "c1".into(), name: "lookup".into(), arguments: r#"{"q":1}"#.into() }];
        assert_eq!(
            assistant_request_message("", &calls),
            json!({"role": "assistant", "content": "", "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "lookup", "arguments": "{\"q\":1}"}}]})
        );
    }

    #[test]
    fn usage_sums_keep_unknown_as_unknown() {
        assert_eq!(add_opt(None, None), None);
        assert_eq!(add_opt(Some(3), None), Some(3));
        assert_eq!(add_opt(Some(3), Some(4)), Some(7));
    }
}
