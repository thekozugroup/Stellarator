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

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;
use stellarator_core::config::STDIO_MCP_BLOCKED;
use stellarator_core::schemas::{
    McpTransport, Phase, RunParams, RunTools, ToolCall, ToolChoice, ToolSnapshot, TraceAssistant, TraceMessage,
    TraceTool,
};
use stellarator_mcp::result::truncate;
use stellarator_mcp::store::CHECK_TIMEOUT;
use stellarator_mcp::Session;

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
        Ok(ToolRuntime { snapshot, settings, routes, sessions })
    }

    /// End every session (stops stdio servers).
    pub async fn close(&self) {
        futures::future::join_all(self.sessions.iter().map(|s| s.close())).await;
    }

    /// Run one call. Err only when the server's connection failed even after a reconnect.
    pub async fn call(&self, call: &ToolCall) -> Result<TraceTool, String> {
        let started = Instant::now();
        let timeout = Duration::from_secs_f64(self.settings.tool_timeout_s.max(0.001));
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
    /// Tool calls run.
    pub tool_calls: i64,
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
            return Ok(SampleOutcome { result, trace, tool_calls: executed });
        }
        convo.push(assistant_request_message(&r.response, &r.tool_calls));
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
