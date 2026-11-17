//! generation::generate against a scripted HTTP server: streaming and non-streaming shapes,
//! reasoning extraction, retries and errors. Ported from backend/tests/test_generation.py.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use futures::future::BoxFuture;
use parking_lot::Mutex;
use serde_json::{json, Value};
use stellarator_core::schemas::{Phase, RunParams};
use stellarator_engine::generation::{
    generate, Backoff, GenerateError, GenerationResult, Progress, ProgressSink, ServerCaps,
};

// ------------------------------------------------------------------------------------ helpers

fn chunk(delta: Value, finish: Option<&str>) -> Value {
    json!({"id": "x", "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]})
}

fn usage(prompt: i64, completion: i64, reasoning: Option<i64>) -> Value {
    let mut u = json!({"prompt_tokens": prompt, "completion_tokens": completion, "total_tokens": prompt + completion});
    if let Some(r) = reasoning {
        u["completion_tokens_details"] = json!({"reasoning_tokens": r});
    }
    json!({"id": "x", "choices": [], "usage": u})
}

fn sse(events: &[Value], done: bool) -> String {
    let mut out: String = events.iter().map(|e| format!("data: {e}\n\n")).collect();
    if done {
        out.push_str("data: [DONE]\n\n");
    }
    out
}

#[derive(Clone)]
enum Reply {
    Json { status: u16, body: Value, retry_after: Option<&'static str> },
    Text { status: u16, body: String },
    Sse { body: String, split: Option<usize> },
    /// Sends the text, then the connection breaks.
    Broken(String),
    Slow(Duration, Box<Reply>),
}

fn stream_reply(events: &[Value], done: bool) -> Reply {
    Reply::Sse { body: sse(events, done), split: None }
}

fn message_reply(message: Value, finish: &str, u: Option<Value>) -> Reply {
    let mut body = json!({"id": "x", "choices": [{"index": 0, "message": message, "finish_reason": finish}]});
    if let Some(u) = u {
        body["usage"] = u["usage"].clone();
    }
    Reply::Json { status: 200, body, retry_after: None }
}

fn status_reply(status: u16, body: Value) -> Reply {
    Reply::Json { status, body, retry_after: None }
}

/// Replays replies in order (the last one repeats) and records request bodies.
struct Script {
    replies: Mutex<VecDeque<Reply>>,
    requests: Mutex<Vec<Value>>,
}

fn render(reply: Reply) -> BoxFuture<'static, Response> {
    Box::pin(async move {
        match reply {
            Reply::Json { status, body, retry_after } => {
                let mut r = (StatusCode::from_u16(status).unwrap(), axum::Json(body)).into_response();
                if let Some(ra) = retry_after {
                    r.headers_mut().insert("retry-after", HeaderValue::from_static(ra));
                }
                r
            }
            Reply::Text { status, body } => (StatusCode::from_u16(status).unwrap(), body).into_response(),
            Reply::Sse { body, split } => {
                let bytes = body.into_bytes();
                let size = split.unwrap_or(bytes.len().max(1));
                let pieces: Vec<Bytes> = bytes.chunks(size).map(Bytes::copy_from_slice).collect();
                let stream = futures::stream::unfold(pieces.into_iter(), |mut it| async move {
                    let next = it.next()?;
                    tokio::task::yield_now().await;
                    Some((Ok::<_, Infallible>(next), it))
                });
                let mut r = Body::from_stream(stream).into_response();
                r.headers_mut().insert("content-type", HeaderValue::from_static("text/event-stream"));
                r
            }
            Reply::Broken(prefix) => {
                let items: Vec<Result<Bytes, std::io::Error>> =
                    vec![Ok(Bytes::from(prefix)), Err(std::io::Error::other("connection reset"))];
                let stream = futures::stream::unfold(items.into_iter(), |mut it| async move {
                    let next = it.next()?;
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Some((next, it))
                });
                let mut r = Body::from_stream(stream).into_response();
                r.headers_mut().insert("content-type", HeaderValue::from_static("text/event-stream"));
                r
            }
            Reply::Slow(delay, then) => {
                tokio::time::sleep(delay).await;
                render(*then).await
            }
        }
    })
}

async fn handle(State(script): State<Arc<Script>>, body: Bytes) -> Response {
    script.requests.lock().push(serde_json::from_slice(&body).unwrap_or(Value::Null));
    let reply = {
        let mut q = script.replies.lock();
        if q.len() > 1 {
            q.pop_front().unwrap()
        } else {
            q.front().cloned().unwrap()
        }
    };
    render(reply).await
}

struct Server {
    url: String,
    script: Arc<Script>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    async fn start(replies: Vec<Reply>) -> Server {
        let script = Arc::new(Script { replies: Mutex::new(replies.into()), requests: Mutex::new(Vec::new()) });
        let app = Router::new().route("/v1/chat/completions", post(handle)).with_state(script.clone());
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Server { url: format!("http://{addr}/v1/chat/completions"), script, task }
    }
    fn requests(&self) -> Vec<Value> {
        self.script.requests.lock().clone()
    }
}

#[derive(Default)]
struct Sleeps(Mutex<Vec<f64>>);

impl Backoff for Sleeps {
    fn sleep(&self, seconds: f64) -> BoxFuture<'_, bool> {
        self.0.lock().push(seconds);
        Box::pin(async { true })
    }
}

#[derive(Default)]
struct Seen {
    chunks: Mutex<Vec<(Phase, i64)>>,
    last: Mutex<Option<Progress>>,
}

impl ProgressSink for Seen {
    fn on_chunk(&self, p: &Progress) {
        self.chunks.lock().push((p.phase, p.tokens));
        *self.last.lock() = Some(p.clone());
    }
}

impl Seen {
    fn phases(&self) -> Vec<Phase> {
        self.chunks.lock().iter().map(|(p, _)| *p).collect()
    }
}

fn msgs() -> Vec<Value> {
    vec![json!({"role": "user", "content": "What is 2 + 2?"})]
}

fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

struct Opts<'a> {
    params: RunParams,
    sample_idx: i64,
    caps: Option<&'a ServerCaps>,
    seen: Option<&'a Seen>,
    sleeps: Option<&'a Sleeps>,
}

impl Default for Opts<'_> {
    fn default() -> Self {
        Self { params: RunParams::default(), sample_idx: 0, caps: None, seen: None, sleeps: None }
    }
}

async fn run(server: &Server, o: Opts<'_>) -> Result<GenerationResult, GenerateError> {
    let caps = ServerCaps::default();
    let seen = Seen::default();
    let sleeps = Sleeps::default();
    generate(
        &client(),
        &server.url,
        "m",
        &msgs(),
        &o.params,
        o.sample_idx,
        o.caps.unwrap_or(&caps),
        o.seen.unwrap_or(&seen),
        o.sleeps.unwrap_or(&sleeps),
    )
    .await
}

async fn run_ok(replies: Vec<Reply>) -> GenerationResult {
    let server = Server::start(replies).await;
    run(&server, Opts::default()).await.expect("generation")
}

fn failed(r: Result<GenerationResult, GenerateError>) -> (String, i64) {
    match r {
        Err(GenerateError::Failed { message, attempts }) => (message, attempts),
        other => panic!("expected a failure, got {other:?}"),
    }
}

// ------------------------------------------------------------------------------------ streaming shapes

#[tokio::test]
async fn stream_reasoning_fields() {
    for field in ["reasoning_content", "reasoning", "thinking"] {
        let server = Server::start(vec![stream_reply(
            &[
                chunk(json!({"role": "assistant", "content": ""}), None),
                chunk(json!({field: "Two plus "}), None),
                chunk(json!({field: "two."}), None),
                chunk(json!({"content": "\n\nThe answer"}), None),
                chunk(json!({"content": " is 4."}), None),
                chunk(json!({}), Some("stop")),
                usage(5, 7, Some(3)),
            ],
            true,
        )])
        .await;
        let seen = Seen::default();
        let r = run(&server, Opts { seen: Some(&seen), ..Opts::default() }).await.unwrap();
        assert_eq!(r.reasoning.as_deref(), Some("Two plus two."), "{field}");
        assert_eq!(r.response, "\n\nThe answer is 4."); // raw when the server split it
        assert_eq!(r.finish_reason.as_deref(), Some("stop"));
        assert_eq!((r.prompt_tokens, r.completion_tokens, r.reasoning_tokens), (Some(5), Some(7), Some(3)));
        assert_eq!((r.attempts, r.chunks), (1, 4));
        assert!(r.ttft_ms.is_some() && r.latency_ms >= r.ttft_ms.unwrap());
        assert_eq!(seen.phases(), [Phase::Thinking, Phase::Thinking, Phase::Answering, Phase::Answering]);
        assert_eq!(seen.last.lock().as_ref().unwrap().tail, "The answer is 4."); // tail of the current phase
        let req = &server.requests()[0];
        assert_eq!(req["stream"], true);
        assert_eq!(req["stream_options"], json!({"include_usage": true}));
    }
}

#[tokio::test]
async fn stream_reasoning_details_only_when_nothing_else() {
    let details = json!([{"type": "reasoning.text", "text": "Thinking hard."}]);
    let r = run_ok(vec![stream_reply(
        &[chunk(json!({"reasoning_details": details}), None), chunk(json!({"content": "4"}), None), chunk(json!({}), Some("stop"))],
        true,
    )])
    .await;
    assert_eq!((r.reasoning.as_deref(), r.response.as_str()), (Some("Thinking hard."), "4"));
    // OpenRouter sends both: reasoning wins, details are not appended twice.
    let r = run_ok(vec![stream_reply(
        &[
            chunk(json!({"reasoning": "Thinking hard.", "reasoning_details": details}), None),
            chunk(json!({"content": "4"}), None),
            chunk(json!({}), Some("stop")),
        ],
        true,
    )])
    .await;
    assert_eq!((r.reasoning.as_deref(), r.response.as_str()), (Some("Thinking hard."), "4"));
}

#[tokio::test]
async fn stream_inline_tags_split_across_chunks() {
    let pieces = ["<th", "ink>\nLet me ", "think.\n</th", "ink>\n\nThe answer", " is 4."];
    let mut events: Vec<Value> = pieces.iter().map(|p| chunk(json!({"content": p}), None)).collect();
    events.push(chunk(json!({}), Some("length")));
    let server = Server::start(vec![Reply::Sse { body: sse(&events, true), split: Some(7) }]).await;
    let seen = Seen::default();
    let r = run(&server, Opts { seen: Some(&seen), ..Opts::default() }).await.unwrap();
    assert_eq!(r.reasoning.as_deref(), Some("Let me think."));
    assert_eq!(r.response, "The answer is 4.");
    assert_eq!(r.finish_reason.as_deref(), Some("length"));
    let phases = seen.phases();
    assert!(phases.contains(&Phase::Thinking));
    assert_eq!(phases.last(), Some(&Phase::Answering));
}

#[tokio::test]
async fn stream_whole_think_block_in_one_chunk() {
    let server = Server::start(vec![stream_reply(
        &[chunk(json!({"content": "<think>X</think>\n\nY"}), None), chunk(json!({"content": "Z"}), None), chunk(json!({}), Some("stop"))],
        true,
    )])
    .await;
    let seen = Seen::default();
    let r = run(&server, Opts { seen: Some(&seen), ..Opts::default() }).await.unwrap();
    assert_eq!((r.reasoning.as_deref(), r.response.as_str()), (Some("X"), "YZ"));
    assert_eq!(seen.phases(), [Phase::Answering, Phase::Answering]);
}

#[tokio::test]
async fn stream_open_think_and_truncated_think() {
    let r = run_ok(vec![stream_reply(
        &[chunk(json!({"content": "Let me think."}), None), chunk(json!({"content": "\n</think>\n\n4"}), None), chunk(json!({}), Some("stop"))],
        true,
    )])
    .await;
    assert_eq!((r.reasoning.as_deref(), r.response.as_str()), (Some("Let me think."), "4"));
    let r = run_ok(vec![stream_reply(
        &[chunk(json!({"content": "<think>\nLet me"}), None), chunk(json!({"content": " think"}), None), chunk(json!({}), Some("length"))],
        true,
    )])
    .await;
    assert_eq!((r.reasoning.as_deref(), r.response.as_str(), r.finish_reason.as_deref()), (Some("Let me think"), "", Some("length")));
}

#[tokio::test]
async fn stream_harmony_leak() {
    let pieces = [
        "<|channel|>analysis<|message|>Add",
        " them.<|end|><|start|>assistant",
        "<|channel|>final<|message|>4",
        "<|return|>",
    ];
    let mut events: Vec<Value> = pieces.iter().map(|p| chunk(json!({"content": p}), None)).collect();
    events.push(chunk(json!({}), Some("stop")));
    let server = Server::start(vec![stream_reply(&events, true)]).await;
    let seen = Seen::default();
    let r = run(&server, Opts { seen: Some(&seen), ..Opts::default() }).await.unwrap();
    assert_eq!((r.reasoning.as_deref(), r.response.as_str()), (Some("Add them."), "4"));
    let phases = seen.phases();
    assert_eq!(phases.first(), Some(&Phase::Thinking));
    assert_eq!(phases.last(), Some(&Phase::Answering));
}

#[tokio::test]
async fn stream_plain_without_usage_or_done() {
    let r = run_ok(vec![stream_reply(&[chunk(json!({"content": "Hi"}), None), chunk(json!({}), Some("stop"))], false)]).await;
    assert_eq!((r.reasoning, r.response.as_str(), r.completion_tokens), (None, "Hi", None));
}

#[tokio::test]
async fn stream_that_ends_early_is_retried() {
    let server = Server::start(vec![
        stream_reply(&[chunk(json!({"content": "Hi"}), None)], false),
        stream_reply(&[chunk(json!({"content": "Hello"}), None), chunk(json!({}), Some("stop"))], true),
    ])
    .await;
    let sleeps = Sleeps::default();
    let r = run(&server, Opts { sleeps: Some(&sleeps), ..Opts::default() }).await.unwrap();
    assert_eq!((r.response.as_str(), r.attempts), ("Hello", 2));
    assert_eq!(sleeps.0.lock().len(), 1);
}

#[tokio::test]
async fn sse_comments_events_and_ndjson() {
    let body = format!(
        ": keep-alive\nevent: message\nid: 1\ndata: {}\r\n\r\n{}\ndata: {}\n\ndata: [DONE]\n\n",
        chunk(json!({"content": "a"}), None),
        chunk(json!({"content": "b"}), None),
        chunk(json!({}), Some("stop")),
    );
    let r = run_ok(vec![Reply::Sse { body, split: Some(3) }]).await;
    assert_eq!(r.response, "ab");
}

// ------------------------------------------------------------------------------------ non-streaming shapes

#[tokio::test]
async fn non_streaming_shapes() {
    let cases: Vec<(Value, Option<&str>, &str)> = vec![
        (json!({"role": "assistant", "content": "4", "reasoning_content": "R"}), Some("R"), "4"),
        (json!({"role": "assistant", "content": "4", "reasoning": "R"}), Some("R"), "4"),
        (json!({"role": "assistant", "content": "4", "thinking": "R"}), Some("R"), "4"),
        (json!({"role": "assistant", "content": "4", "reasoning_details": [{"type": "reasoning.text", "text": "R"}]}), Some("R"), "4"),
        (json!({"role": "assistant", "content": "<think>R</think>\n\n4"}), Some("R"), "4"),
        (json!({"role": "assistant", "content": "R</think>4"}), Some("R"), "4"),
        (json!({"role": "assistant", "content": "<think>R"}), Some("R"), ""),
        (json!({"role": "assistant", "content": "4"}), None, "4"),
        (json!({"role": "assistant", "content": null, "reasoning_content": "R"}), Some("R"), ""),
    ];
    for (message, reasoning, response) in cases {
        let server = Server::start(vec![message_reply(message.clone(), "stop", Some(usage(3, 9, Some(2))))]).await;
        let params = RunParams { stream: false, ..RunParams::default() };
        let r = run(&server, Opts { params, ..Opts::default() }).await.unwrap();
        assert_eq!((r.reasoning.as_deref(), r.response.as_str()), (reasoning, response), "{message}");
        assert_eq!((r.prompt_tokens, r.completion_tokens, r.reasoning_tokens), (Some(3), Some(9), Some(2)));
        let req = &server.requests()[0];
        assert_eq!(req["stream"], false);
        assert!(req.get("stream_options").is_none());
        assert!(r.ttft_ms.is_none() && r.latency_ms >= 0);
    }
}

#[tokio::test]
async fn stream_requested_but_server_answers_json() {
    let r = run_ok(vec![message_reply(json!({"role": "assistant", "content": "4", "reasoning": "R"}), "stop", None)]).await;
    assert_eq!((r.reasoning.as_deref(), r.response.as_str(), r.finish_reason.as_deref()), (Some("R"), "4", Some("stop")));
}

#[tokio::test]
async fn non_json_answer_is_retried() {
    let server = Server::start(vec![
        Reply::Text { status: 200, body: "not json".into() },
        message_reply(json!({"role": "assistant", "content": "ok"}), "stop", None),
    ])
    .await;
    let params = RunParams { stream: false, ..RunParams::default() };
    let r = run(&server, Opts { params, ..Opts::default() }).await.unwrap();
    assert_eq!((r.response.as_str(), r.attempts), ("ok", 2));
}

// ------------------------------------------------------------------------------------ retries and errors

#[tokio::test]
async fn retries_then_succeeds() {
    let server = Server::start(vec![
        status_reply(500, json!({"error": {"message": "Internal error"}})),
        Reply::Json { status: 429, body: json!({"error": {"message": "Slow down"}}), retry_after: Some("3") },
        stream_reply(&[chunk(json!({"content": "ok"}), None), chunk(json!({}), Some("stop"))], true),
    ])
    .await;
    let sleeps = Sleeps::default();
    let r = run(&server, Opts { sleeps: Some(&sleeps), ..Opts::default() }).await.unwrap();
    assert_eq!((r.response.as_str(), r.attempts), ("ok", 3));
    let calls = sleeps.0.lock().clone();
    assert!((0.75..=1.25).contains(&calls[0]), "{calls:?}");
    assert_eq!(calls[1], 3.0);
}

#[tokio::test]
async fn gives_up_after_max_attempts() {
    let server = Server::start(vec![status_reply(503, json!({"error": {"message": "Overloaded"}}))]).await;
    let sleeps = Sleeps::default();
    let params = RunParams { max_attempts: 3, ..RunParams::default() };
    let (message, attempts) = failed(run(&server, Opts { params, sleeps: Some(&sleeps), ..Opts::default() }).await);
    assert_eq!(attempts, 3);
    assert_eq!(message, "The server answered 503: Overloaded.");
    assert_eq!(server.requests().len(), 3);
    assert_eq!(sleeps.0.lock().len(), 2);
}

#[tokio::test]
async fn client_errors_fail_at_once() {
    let server = Server::start(vec![status_reply(404, json!({"error": {"message": "The model `m` does not exist."}}))]).await;
    let (message, attempts) = failed(run(&server, Opts::default()).await);
    assert_eq!(attempts, 1);
    assert_eq!(message, "The server answered 404: The model `m` does not exist.");
}

#[tokio::test]
async fn connect_errors_are_retried() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let sleeps = Sleeps::default();
    let params = RunParams { max_attempts: 2, ..RunParams::default() };
    let r = generate(
        &client(),
        &format!("http://127.0.0.1:{port}/v1/chat/completions"),
        "m",
        &msgs(),
        &params,
        0,
        &ServerCaps::default(),
        &Seen::default(),
        &sleeps,
    )
    .await;
    let (message, attempts) = failed(r);
    assert_eq!(attempts, 2);
    assert!(message.starts_with("Could not connect."), "{message}");
}

#[tokio::test]
async fn read_error_mid_stream_is_retried() {
    let server = Server::start(vec![
        Reply::Broken(sse(&[chunk(json!({"content": "partial"}), None)], false)),
        stream_reply(&[chunk(json!({"content": "full"}), None), chunk(json!({}), Some("stop"))], true),
    ])
    .await;
    let r = run(&server, Opts::default()).await.unwrap();
    assert_eq!((r.response.as_str(), r.attempts), ("full", 2));
}

#[tokio::test]
async fn dropped_connection_sentence() {
    let server = Server::start(vec![Reply::Broken(sse(&[chunk(json!({"content": "partial"}), None)], false))]).await;
    let params = RunParams { max_attempts: 1, ..RunParams::default() };
    let (message, _) = failed(run(&server, Opts { params, ..Opts::default() }).await);
    assert_eq!(message, "The connection to the server dropped before the answer finished.");
}

#[tokio::test]
async fn total_timeout_per_attempt() {
    let ok = stream_reply(&[chunk(json!({"content": "late"}), None), chunk(json!({}), Some("stop"))], true);
    let server = Server::start(vec![Reply::Slow(Duration::from_secs(5), Box::new(ok))]).await;
    // Below the API minimum, for the test.
    let params = RunParams { max_attempts: 1, request_timeout_s: 0.05, ..RunParams::default() };
    let started = std::time::Instant::now();
    let (message, _) = failed(run(&server, Opts { params, ..Opts::default() }).await);
    assert!(message.contains("request timeout"), "{message}");
    assert_eq!(message, "The request did not finish within the run's request timeout of 0.05 s.");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn error_event_mid_stream() {
    let server = Server::start(vec![stream_reply(
        &[chunk(json!({"content": "a"}), None), json!({"error": {"message": "boom", "code": 400}})],
        true,
    )])
    .await;
    let (message, _) = failed(run(&server, Opts::default()).await);
    assert_eq!(message, "The server reported an error mid-answer: boom.");
}

#[tokio::test]
async fn stream_options_rejected_is_remembered() {
    let ok = stream_reply(&[chunk(json!({"content": "ok"}), None), chunk(json!({}), Some("stop"))], true);
    let server = Server::start(vec![
        status_reply(400, json!({"error": {"message": "Unknown field: stream_options"}})),
        ok.clone(),
    ])
    .await;
    let caps = ServerCaps::default();
    let r = run(&server, Opts { caps: Some(&caps), ..Opts::default() }).await.unwrap();
    assert_eq!((r.response.as_str(), r.attempts), ("ok", 1));
    let reqs = server.requests();
    assert!(reqs[0].get("stream_options").is_some() && reqs[1].get("stream_options").is_none());
    assert!(!caps.stream_options());
    // The next request of the run goes without it.
    let server2 = Server::start(vec![ok]).await;
    run(&server2, Opts { caps: Some(&caps), ..Opts::default() }).await.unwrap();
    assert!(server2.requests()[0].get("stream_options").is_none());
}

#[tokio::test]
async fn stream_options_remembered_when_probe_works() {
    // A 400 that does not name the field: the probe without it works, so the run stops sending it.
    let ok = stream_reply(&[chunk(json!({"content": "ok"}), None), chunk(json!({}), Some("stop"))], true);
    let server = Server::start(vec![status_reply(400, json!({"error": {"message": "Bad request"}})), ok]).await;
    let caps = ServerCaps::default();
    let r = run(&server, Opts { caps: Some(&caps), ..Opts::default() }).await.unwrap();
    assert_eq!(r.attempts, 1);
    assert!(!caps.stream_options());
}

#[tokio::test]
async fn unrelated_400_is_not_blamed_on_stream_options() {
    let server = Server::start(vec![status_reply(400, json!({"error": {"message": "messages must not be empty"}}))]).await;
    let caps = ServerCaps::default();
    let (message, attempts) = failed(run(&server, Opts { caps: Some(&caps), ..Opts::default() }).await);
    assert_eq!(message, "The server answered 400: messages must not be empty.");
    assert_eq!(attempts, 1);
    assert_eq!(server.requests().len(), 2); // one try without stream_options, then give up
    assert!(caps.stream_options());
}

#[tokio::test]
async fn seed_per_sample() {
    let server =
        Server::start(vec![stream_reply(&[chunk(json!({"content": "ok"}), None), chunk(json!({}), Some("stop"))], true)]).await;
    let params = RunParams { seed: Some(100), ..RunParams::default() };
    run(&server, Opts { params, sample_idx: 2, ..Opts::default() }).await.unwrap();
    assert_eq!(server.requests()[0]["seed"], 102);
}

#[tokio::test]
async fn interrupted_backoff_gives_the_slot_up() {
    struct Halt;
    impl Backoff for Halt {
        fn sleep(&self, _seconds: f64) -> BoxFuture<'_, bool> {
            Box::pin(async { false })
        }
    }
    let server = Server::start(vec![status_reply(503, json!({"error": {"message": "Overloaded"}}))]).await;
    let r = generate(&client(), &server.url, "m", &msgs(), &RunParams::default(), 0, &ServerCaps::default(), &Seen::default(), &Halt).await;
    assert_eq!(r, Err(GenerateError::Interrupted));
}
