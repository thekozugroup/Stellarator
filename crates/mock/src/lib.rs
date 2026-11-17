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
//!
//! Speed: `tps` tokens per second per sequence. Failures and delays are random.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Map, Value};
use tokio::task::JoinHandle;
use tokio::time::Instant;

pub const MODELS: [&str; 7] = [
    "mock-reasoning-content",
    "mock-reasoning",
    "mock-think-tags",
    "mock-open-think",
    "mock-plain",
    "mock-flaky",
    "mock-truncating",
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

/// The mock's routes: `GET /v1/models` and `POST /v1/chat/completions`.
pub fn router(tps: f64) -> Router {
    let tps = if tps.is_finite() && tps > 0.0 { tps } else { 60.0 };
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat))
        .with_state(Arc::new(MockState { tps }))
}

/// Bind `addr` (port 0 picks a free port) and serve in the background. Returns the bound address
/// and the server task; abort the task to stop it.
pub async fn serve(addr: SocketAddr, tps: f64) -> std::io::Result<(SocketAddr, JoinHandle<()>)> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    let app = router(tps);
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

#[cfg(test)]
mod tests {
    use super::*;

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
