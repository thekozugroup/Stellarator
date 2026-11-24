//! A fake OpenAI-compatible server for trying Stellarator without a GPU, and for the engine's
//! tests. A port of `tools/mock_server.py`: the same models, answers, usage and failure rates.
//!
//! Each model returns its thinking trace a different way:
//!
//! | Model | Thinking trace |
//! | --- | --- |
//! | `mock-reasoning-content` | `delta.reasoning_content` (vLLM < 0.10, SGLang, DeepSeek API) |
//! | `mock-reasoning` | `delta.reasoning` (vLLM >= 0.10, OpenRouter, Ollama) |
//! | `mock-think-tags` | inline `<think>...</think>` in content (no reasoning parser) |
//! | `mock-open-think` | content starts mid-thought and only has `</think>` (template opened it) |
//! | `mock-plain` | no thinking at all |
//! | `mock-flaky` | like `mock-reasoning-content`, but 20% of requests fail with 500 or 429 |
//! | `mock-truncating` | stops with finish_reason "length" |
//! | `mock-tools` | calls a tool from the request's `tools`, then answers using the results ([`toolmodel`]); given only a Code Mode tool, writes JavaScript that calls its namespaces ([`codemodel`]) |
//! | `mock-tools-loop` | calls a tool on every request and never answers |
//! | `mock-no-tools` | answers 400 to requests with `tools`, as vLLM without a tool parser |
//!
//! The same port also serves a small MCP server over streamable HTTP at `/mcp` ([`mcp`]), and
//! `stellarator-mock mcp-stdio` serves it on stdin and stdout.
//!
//! Speed: `tps` tokens per second per sequence. Failures and delays are random.
//!
//! A key: [`router_with_key`] and [`serve_with_key`] (`stellarator-mock --api-key KEY`) answer
//! every request whose `Authorization` header is not `Bearer KEY` with 401 and an OpenAI-style
//! error, as a server that rejects a missing or wrong key. Stellarator reports it as "The server
//! rejected the API key (401)", which is how its key-rejected states are tried.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Map, Value};
use tokio::task::JoinHandle;
use tokio::time::Instant;

pub mod mcp;
pub mod codemodel;
pub mod toolmodel;

pub const MODELS: [&str; 10] = [
    "mock-reasoning-content",
    "mock-reasoning",
    "mock-think-tags",
    "mock-open-think",
    "mock-plain",
    "mock-flaky",
    "mock-truncating",
    "mock-tools",
    "mock-tools-loop",
    "mock-no-tools",
];

const STEPS: [&str; 6] = [
    "Checking the arithmetic again.",
    "That matches the earlier step.",
    "Another approach would be to work backwards.",
    "So the intermediate value holds.",
    "I should double-check the units.",
    "Combining these gives the total.",
];

struct MockState {
    tps: f64,
}

/// The mock's routes: `GET /v1/models`, `POST /v1/chat/completions`, and the MCP server at `/mcp`.
pub fn router(tps: f64) -> Router {
    let tps = if tps.is_finite() && tps > 0.0 { tps } else { 60.0 };
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat))
        .with_state(Arc::new(MockState { tps }))
        .nest_service("/mcp", mcp::http_service())
}

/// The routes of [`router`] behind a key: a request without `Authorization: Bearer <api_key>` gets
/// 401. `None` (or an empty key) is [`router`] itself.
pub fn router_with_key(tps: f64, api_key: Option<String>) -> Router {
    match api_key.filter(|k| !k.is_empty()) {
        Some(key) => router(tps).layer(middleware::from_fn_with_state(Arc::new(key), require_key)),
        None => router(tps),
    }
}

/// 401 unless the request carries the key, as OpenAI answers a missing or wrong one.
async fn require_key(State(key): State<Arc<String>>, request: Request, next: Next) -> Response {
    let sent = request.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    if sent.and_then(|v| v.strip_prefix("Bearer ")) == Some(key.as_str()) {
        next.run(request).await
    } else {
        error(StatusCode::UNAUTHORIZED, "Incorrect API key provided.")
    }
}

/// Bind `addr` (port 0 picks a free port) and serve in the background. Returns the bound address
/// and the server task; abort the task to stop it.
pub async fn serve(addr: SocketAddr, tps: f64) -> std::io::Result<(SocketAddr, JoinHandle<()>)> {
    serve_with_key(addr, tps, None).await
}

/// [`serve`] behind a key ([`router_with_key`]).
pub async fn serve_with_key(addr: SocketAddr, tps: f64, api_key: Option<String>) -> std::io::Result<(SocketAddr, JoinHandle<()>)> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    let app = router_with_key(tps, api_key);
    let join = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok((bound, join))
}

async fn models() -> Json<Value> {
    let data: Vec<Value> = MODELS.iter().map(|m| json!({"id": m, "object": "model", "owned_by": "mock"})).collect();
    Json(json!({"object": "list", "data": data}))
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error": {"message": message}}))).into_response()
}

fn last_user(messages: &Value) -> String {
    let Some(list) = messages.as_array() else { return String::new() };
    for m in list.iter().rev() {
        if m.get("role").and_then(Value::as_str) == Some("user") {
            return match m.get("content") {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => "null".into(),
            };
        }
    }
    String::new()
}

/// (thinking words, answer words), split on single spaces like Python's `str.split(" ")`.
fn compose(prompt: &str, rng: &mut fastrand::Rng) -> (Vec<String>, Vec<String>) {
    let head = prompt.split_whitespace().take(12).collect::<Vec<_>>().join(" ");
    let mut thinking = format!(
        "The question asks: {head}. Let me break it down. First I identify what is given and what \
         is asked. Then I work through each step and check the result against the constraints. "
    );
    let n = rng.usize(6..=24);
    let steps: Vec<&str> = (0..n).map(|_| STEPS[rng.usize(..STEPS.len())]).collect();
    thinking.push_str(&steps.join(" "));
    let answer = format!(
        "Here is the answer to \u{201c}{head}\u{201d}. Working step by step, the key quantities combine to give \
         the result. Final answer: {}.",
        rng.i64(2..=999)
    );
    (
        thinking.split(' ').map(str::to_string).collect(),
        answer.split(' ').map(str::to_string).collect(),
    )
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn chunk(model: &str, delta: Value, finish: Option<&str>, usage: Option<&Value>) -> Bytes {
    let mut obj = json!({
        "id": "chatcmpl-mock",
        "object": "chat.completion.chunk",
        "created": now_secs(),
        "model": model,
        "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
    });
    if let Some(u) = usage {
        obj["usage"] = u.clone();
        obj["choices"] = json!([]);
    }
    Bytes::from(format!("data: {obj}\n\n"))
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

fn positive_int(v: Option<&Value>) -> Option<usize> {
    let v = v?;
    let n = v.as_u64().or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))?;
    (n > 0).then_some(n as usize)
}

/// Seconds from an exponential distribution with rate `tps` (Python's `random.expovariate`).
fn expovariate(rng: &mut fastrand::Rng, rate: f64) -> f64 {
    -(1.0 - rng.f64()).ln() / rate
}

async fn chat(State(state): State<Arc<MockState>>, raw: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&raw).unwrap_or(Value::Null);
    let model = body.get("model").and_then(Value::as_str).unwrap_or("").to_string();
    if !MODELS.contains(&model.as_str()) {
        return error(StatusCode::NOT_FOUND, &format!("The model `{model}` does not exist."));
    }
    let mut rng = fastrand::Rng::new();
    if toolmodel::TOOL_MODELS.contains(&model.as_str()) {
        return tool_chat(&state, &model, &body, rng).await;
    }
    if model == "mock-flaky" && rng.f64() < 0.2 {
        if rng.f64() < 0.5 {
            let mut r = error(StatusCode::TOO_MANY_REQUESTS, "Rate limited");
            r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
            return r;
        }
        return error(StatusCode::INTERNAL_SERVER_ERROR, "Internal error");
    }

    let prompt = last_user(body.get("messages").unwrap_or(&Value::Null));
    let (mut think, answer) = compose(&prompt, &mut rng);
    let mut max_tokens = positive_int(body.get("max_tokens"))
        .or_else(|| positive_int(body.get("max_completion_tokens")))
        .unwrap_or(100_000);
    let mut finish = "stop";
    if model == "mock-truncating" {
        max_tokens = max_tokens.min(think.len() / 2);
    }
    if model == "mock-plain" {
        think.clear();
    }

    // The token stream as (field, text) pairs.
    let field = match model.as_str() {
        "mock-reasoning-content" | "mock-flaky" => "reasoning_content",
        _ => "reasoning",
    };
    let mut tokens: Vec<(&'static str, String)> = Vec::new();
    if model == "mock-think-tags" || model == "mock-open-think" {
        if model == "mock-think-tags" {
            tokens.push(("content", "<think>\n".into()));
        }
        tokens.extend(think.iter().map(|w| ("content", format!("{w} "))));
        tokens.push(("content", "\n</think>\n\n".into()));
    } else {
        tokens.extend(think.iter().map(|w| (field, format!("{w} "))));
    }
    let last = answer.len().saturating_sub(1);
    tokens.extend(
        answer.iter().enumerate().map(|(i, w)| ("content", if i == last { w.clone() } else { format!("{w} ") })),
    );
    if tokens.len() > max_tokens {
        tokens.truncate(max_tokens);
        finish = "length";
    }

    let prompt_tokens = prompt.split_whitespace().count().max(1);
    let reasoning_tokens = tokens.iter().filter(|(f, _)| *f != "content").count();
    let mut usage = json!({
        "prompt_tokens": prompt_tokens,
        "completion_tokens": tokens.len(),
        "total_tokens": prompt_tokens + tokens.len(),
    });
    if reasoning_tokens > 0 {
        usage["completion_tokens_details"] = json!({"reasoning_tokens": reasoning_tokens});
    }

    // Queueing / prefill.
    tokio::time::sleep(Duration::from_secs_f64(0.05 + rng.f64() * 0.35)).await;

    if !truthy(body.get("stream")) {
        tokio::time::sleep(Duration::from_secs_f64(tokens.len() as f64 / state.tps)).await;
        let content: String = tokens.iter().filter(|(f, _)| *f == "content").map(|(_, t)| t.as_str()).collect();
        let reasoning: String = tokens.iter().filter(|(f, _)| *f != "content").map(|(_, t)| t.as_str()).collect();
        let mut msg = Map::new();
        msg.insert("role".into(), json!("assistant"));
        msg.insert("content".into(), json!(content));
        if !reasoning.is_empty() {
            msg.insert(field.into(), json!(reasoning));
        }
        return Json(json!({
            "id": "chatcmpl-mock",
            "object": "chat.completion",
            "created": now_secs(),
            "model": model,
            "choices": [{"index": 0, "message": msg, "finish_reason": finish}],
            "usage": usage,
        }))
        .into_response();
    }

    let include_usage = body.get("stream_options").and_then(|o| o.get("include_usage")).is_some_and(|v| truthy(Some(v)));
    let tps = state.tps;
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(32);
    tokio::spawn(async move {
        if tx.send(chunk(&model, json!({"role": "assistant", "content": ""}), None, None)).await.is_err() {
            return;
        }
        // Keep the average rate exact even when the timer is coarser than one token's gap.
        let mut next = Instant::now();
        for (f, t) in tokens {
            next += Duration::from_secs_f64(expovariate(&mut rng, tps));
            if next > Instant::now() {
                tokio::time::sleep_until(next).await;
            }
            let mut delta = Map::new();
            delta.insert(f.into(), Value::String(t));
            if tx.send(chunk(&model, Value::Object(delta), None, None)).await.is_err() {
                return;
            }
        }
        if tx.send(chunk(&model, json!({}), Some(finish), None)).await.is_err() {
            return;
        }
        if include_usage && tx.send(chunk(&model, json!({}), None, Some(&usage))).await.is_err() {
            return;
        }
        let _ = tx.send(Bytes::from_static(b"data: [DONE]\n\n")).await;
    });
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|b| (Ok::<_, Infallible>(b), rx))
    });
    let mut resp = Body::from_stream(stream).into_response();
    resp.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    resp.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp
}

/// A tool-calling model's answer: a thinking trace in `reasoning`, then tool calls (each call's
/// arguments in pieces of 5 characters) or content, streamed at `tps` or sent whole.
async fn tool_chat(state: &MockState, model: &str, body: &Value, mut rng: fastrand::Rng) -> Response {
    let plan = toolmodel::plan(model, body, &mut rng);
    let (reasoning, calls, content) = match plan {
        toolmodel::Plan::Reject(message) => return error(StatusCode::BAD_REQUEST, &message),
        toolmodel::Plan::Calls { reasoning, calls } => (reasoning, calls, String::new()),
        toolmodel::Plan::Answer { reasoning, content } => (reasoning, Vec::new(), content),
    };
    let model = model.to_string();
    let finish = if calls.is_empty() { "stop" } else { "tool_calls" };
    // The deltas, in order: reasoning words, then the calls or the content words.
    let mut deltas: Vec<Value> = reasoning.split(' ').map(|w| json!({"reasoning": format!("{w} ")})).collect();
    for (index, call) in calls.iter().enumerate() {
        deltas.push(json!({"tool_calls": [{"index": index, "id": call.id, "type": "function",
                                            "function": {"name": call.name, "arguments": ""}}]}));
        let chars: Vec<char> = call.arguments.chars().collect();
        for piece in chars.chunks(5) {
            let piece: String = piece.iter().collect();
            deltas.push(json!({"tool_calls": [{"index": index, "function": {"arguments": piece}}]}));
        }
    }
    let words: Vec<&str> = if content.is_empty() { Vec::new() } else { content.split(' ').collect() };
    let last = words.len().saturating_sub(1);
    for (i, w) in words.iter().enumerate() {
        deltas.push(json!({"content": if i == last { w.to_string() } else { format!("{w} ") }}));
    }
    let prompt_tokens: usize = body
        .get("messages")
        .and_then(Value::as_array)
        .map(|m| m.iter().map(|x| x.to_string().split_whitespace().count()).sum())
        .unwrap_or(0)
        .max(1);
    let reasoning_tokens = reasoning.split(' ').count();
    let usage = json!({
        "prompt_tokens": prompt_tokens,
        "completion_tokens": deltas.len(),
        "total_tokens": prompt_tokens + deltas.len(),
        "completion_tokens_details": {"reasoning_tokens": reasoning_tokens},
    });
    tokio::time::sleep(Duration::from_secs_f64(0.02 + rng.f64() * 0.08)).await;
    if !truthy(body.get("stream")) {
        tokio::time::sleep(Duration::from_secs_f64(deltas.len() as f64 / state.tps)).await;
        let mut msg = Map::new();
        msg.insert("role".into(), json!("assistant"));
        msg.insert("content".into(), if content.is_empty() && !calls.is_empty() { Value::Null } else { json!(content) });
        msg.insert("reasoning".into(), json!(reasoning));
        if !calls.is_empty() {
            let list: Vec<Value> = calls
                .iter()
                .map(|c| json!({"id": c.id, "type": "function", "function": {"name": c.name, "arguments": c.arguments}}))
                .collect();
            msg.insert("tool_calls".into(), Value::Array(list));
        }
        return Json(json!({
            "id": "chatcmpl-mock", "object": "chat.completion", "created": now_secs(), "model": model,
            "choices": [{"index": 0, "message": msg, "finish_reason": finish}], "usage": usage,
        }))
        .into_response();
    }
    let include_usage = body.get("stream_options").and_then(|o| o.get("include_usage")).is_some_and(|v| truthy(Some(v)));
    let tps = state.tps;
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(32);
    tokio::spawn(async move {
        if tx.send(chunk(&model, json!({"role": "assistant", "content": ""}), None, None)).await.is_err() {
            return;
        }
        let mut next = Instant::now();
        for delta in deltas {
            next += Duration::from_secs_f64(expovariate(&mut rng, tps));
            if next > Instant::now() {
                tokio::time::sleep_until(next).await;
            }
            if tx.send(chunk(&model, delta, None, None)).await.is_err() {
                return;
            }
        }
        if tx.send(chunk(&model, json!({}), Some(finish), None)).await.is_err() {
            return;
        }
        if include_usage && tx.send(chunk(&model, json!({}), None, Some(&usage))).await.is_err() {
            return;
        }
        let _ = tx.send(Bytes::from_static(b"data: [DONE]\n\n")).await;
    });
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|b| (Ok::<_, Infallible>(b), rx))
    });
    let mut resp = Body::from_stream(stream).into_response();
    resp.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    resp.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    async fn status(app: Router, auth: Option<&str>) -> StatusCode {
        let mut request = axum::http::Request::builder().uri("/v1/models");
        if let Some(a) = auth {
            request = request.header(header::AUTHORIZATION, a);
        }
        app.oneshot(request.body(Body::empty()).unwrap()).await.unwrap().status()
    }

    #[tokio::test]
    async fn a_key_turns_away_requests_without_it() {
        let keyed = || router_with_key(60.0, Some("sk-mock".into()));
        assert_eq!(status(keyed(), None).await, StatusCode::UNAUTHORIZED);
        assert_eq!(status(keyed(), Some("Bearer sk-wrong")).await, StatusCode::UNAUTHORIZED);
        assert_eq!(status(keyed(), Some("Bearer sk-mock")).await, StatusCode::OK);
        // Without a key (or an empty one) every request is served.
        assert_eq!(status(router_with_key(60.0, None), None).await, StatusCode::OK);
        assert_eq!(status(router_with_key(60.0, Some(String::new())), None).await, StatusCode::OK);
        let chat = axum::http::Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"model":"mock-plain","messages":[{"role":"user","content":"hi"}]}"#))
            .unwrap();
        assert_eq!(keyed().oneshot(chat).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn compose_matches_python_shape() {
        let mut rng = fastrand::Rng::with_seed(1);
        let (think, answer) = compose("What is  2 + 2?", &mut rng);
        assert_eq!(think[0], "The");
        assert!(think.join(" ").starts_with("The question asks: What is 2 + 2?. Let me break it down."));
        assert!(answer.join(" ").starts_with("Here is the answer to \u{201c}What is 2 + 2?\u{201d}."));
        assert!(answer.last().is_some_and(|w| w.ends_with('.')));
    }
}
