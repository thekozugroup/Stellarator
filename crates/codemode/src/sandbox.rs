//! One code execution: a fresh QuickJS runtime and context on a thread of its own, running the
//! module Cloudflare's `DynamicWorkerExecutor` generates for the model's normalized code
//! (`executor.js`, built by the package's own code in js/cloudflare-runtime.js), with the
//! namespaces dispatching to the run's MCP tools through [`Host`]. Then what the model receives is
//! rendered as Cloudflare's `runCode` or `runPass` returns it and the AI SDK sends it.
//!
//! Around the module stand js/workers-globals.js (the globals of a Worker that model code reaches
//! for) and js/sandbox.js (what Workers RPC and the host Worker do in Cloudflare's setup).
//!
//! Limits: the execution's timeout (the module's own `setTimeout` race, as Cloudflare's, plus an
//! interrupt shortly after for code that never yields, such as `while (true) {}`), a memory limit,
//! a stack limit, at most [`MAX_CALLS`] tool calls, at most [`PARALLEL_CALLS`] of them at a time.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::future::BoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::module::Declared;
use rquickjs::promise::PromiseState;
use rquickjs::{Context, Ctx, Function, Module, Object, Promise, Runtime, Value as JsValue};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use stellarator_api::{CodeExecution, CodeFormat, CodeStatus, InnerToolCall, LiveCode};

use crate::mcp::unwrap_mcp_result;
use crate::util::{self, describe};

/// The prelude: one function of `host`, made of the package's runtime code (for the value codec,
/// the MCP unwrapping and the durable log's size rule), V8's JSON messages, the Workers globals and
/// the host side.
const PRELUDE: &str = concat!(
    "(function (host) {\n\"use strict\";\n",
    include_str!("../js/cloudflare-runtime.js"),
    "\n",
    include_str!("../js/v8-json.js"),
    "\n",
    include_str!("../js/workers-globals.js"),
    "\n",
    include_str!("../js/sandbox.js"),
    "\n})"
);

/// `cloudflare:workers`, as far as the executor module needs it: the base class it extends.
const WORKERS_MODULE: &str = "export class WorkerEntrypoint { constructor(ctx, env) { this.ctx = ctx; this.env = env; } }\n\
    export class DurableObject { constructor(ctx, env) { this.ctx = ctx; this.env = env; } }\n\
    export class RpcTarget {}\nexport const env = {};\n";

/// Tool calls one execution may make.
pub const MAX_CALLS: usize = 1000;
/// Tool calls of one execution that run at the same time; later ones wait.
pub const PARALLEL_CALLS: usize = 8;
/// What a call still running when the execution ends answers (Cloudflare's `LATE_CALL_MESSAGE`).
pub const LATE_CALL_MESSAGE: &str =
    "The code execution finished before this tool call settled. Await every tool call before returning.";
/// Code that does not yield is interrupted this long after the timeout.
const GRACE: Duration = Duration::from_millis(250);
/// The thread's stack and QuickJS's share of it.
const THREAD_STACK: usize = 8 * 1024 * 1024;
const JS_STACK: usize = 1024 * 1024;
/// Extra memory for reading the outcome after the code ran out.
const RENDER_MEMORY: usize = 256 * 1024 * 1024;
/// workerd names a place in `executor.js` two lines below the module's own line, with the column
/// counted from 1. QuickJS's line is one above the module's and its column one further right, so
/// the same token is one line down and one column left of where QuickJS puts it.
const WORKERD_LINE_OFFSET: u32 = 1;

/// An execution's limits.
#[derive(Clone, Debug)]
pub struct Limits {
    pub timeout: Duration,
    pub memory_bytes: usize,
    /// truncateResult's budget for the result: Some((max_chars, max_tokens)); None for no truncation.
    pub budget: Option<(i64, i64)>,
    /// The same for the console output.
    pub logs_budget: Option<(i64, i64)>,
    /// Inner call results are stored cut to this many characters.
    pub store_chars: usize,
}

/// What every execution of a run shares: the format, the code tool's name and the namespaces.
#[derive(Clone, Debug)]
pub struct Setup {
    pub format: CodeFormat,
    pub tool_name: String,
    /// The globals the code calls tools through: providers (`code_tool`) or connectors (`runtime`).
    pub namespaces: Vec<String>,
    /// `runtime`: the connectors as `connector.describe()` returns them, as a JSON array, for
    /// `codemode.search` and `codemode.describe`.
    pub descriptions: Arc<str>,
    pub limits: Limits,
}

/// The answer to one tool call of the code.
#[derive(Clone, Debug, PartialEq)]
pub enum CallAnswer {
    /// The MCP server's `tools/call` result, as JSON. The code gets it unwrapped as Cloudflare's
    /// `McpConnector` unwraps it; an error result throws.
    Mcp(Value),
    /// The call throws this message in the code.
    Thrown(String),
    /// The call could not be made at all (the MCP server is gone): the execution stops and the
    /// sample fails with this message.
    Fatal(String),
}

/// The run's side of the namespaces.
pub trait Host: Send + Sync {
    /// The MCP server and tool behind `namespace.method`, or None.
    fn route(&self, namespace: &str, method: &str) -> Option<(String, String)>;
    /// Call `namespace.method` with the code's first argument.
    fn call(&self, namespace: String, method: String, arguments: Option<Map<String, Value>>) -> BoxFuture<'_, CallAnswer>;
}

/// A finished code tool call.
#[derive(Clone, Debug, PartialEq)]
pub struct Execution {
    /// What the model receives as the tool's result.
    pub content: String,
    pub is_error: bool,
    /// truncateResult shrank the result or the logs.
    pub truncated: bool,
    /// What happened inside; None when no code ran (the input was not `{"code": string}`).
    pub detail: Option<CodeExecution>,
}

// ------------------------------------------------------------------------------ async side

enum Event {
    Call { id: f64, namespace: String, name: String, args: String },
}

struct Reply {
    id: f64,
    json: String,
}

/// What the thread hands back.
struct Outcome {
    /// None: the input was invalid (content holds the AI SDK's error).
    normalized: Option<String>,
    status: CodeStatus,
    content: String,
    is_error: bool,
    truncated: bool,
    result: Option<String>,
    error: Option<String>,
    logs: Vec<String>,
}

/// A call waiting for a free slot: its index, the code's call id, namespace, method, arguments.
type Queued = (usize, f64, String, String, Option<Map<String, Value>>);

/// A reply on its way: the tool call's record index (None for a platform call), the code's call
/// id, and the answer.
type Pending<'a> = BoxFuture<'a, (Option<usize>, f64, Answered)>;

enum Answered {
    Tool(CallAnswer),
    /// A platform call's dispatcher JSON, or why it failed.
    Platform(Result<String, String>),
}

struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn cut(text: String, max: usize) -> (String, bool) {
    let count = text.chars().count();
    if count <= max {
        return (text, false);
    }
    let kept: String = text.chars().take(max).collect();
    (format!("{kept}\n[truncated: {} more characters]", count - max), true)
}

/// A new execution id, as the durable runtime writes them: `exec_<ms since 1970, 16 digits>_<uuid>`.
pub fn execution_id() -> String {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let mut b = [0u8; 16];
    let _ = getrandom::fill(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("exec_{ms:016}_{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

/// Run one call of the code tool: `arguments` is the JSON text the model wrote for
/// `{"code": string}`. `progress` hears the tool calls as they start and finish. Err: the sample
/// must fail (an MCP server could not be reached, or the sandbox could not start).
pub async fn execute(
    setup: &Setup,
    arguments: &str,
    host: &dyn Host,
    progress: &(dyn Fn(&LiveCode) + Sync),
) -> Result<Execution, String> {
    let started = Instant::now();
    let limits = &setup.limits;
    let runtime = setup.format == CodeFormat::Runtime;
    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let (reply_tx, reply_rx) = mpsc::channel::<Reply>();
    let (done_tx, mut done_rx) = tokio::sync::oneshot::channel::<Result<Outcome, String>>();
    let cancel = Arc::new(AtomicBool::new(false));
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    {
        let (setup, arguments, cancel) = (setup.clone(), arguments.to_string(), cancel.clone());
        std::thread::Builder::new()
            .name("codemode".into())
            .stack_size(THREAD_STACK)
            .spawn(move || {
                let out = run_thread(&setup, &arguments, ev_tx, reply_rx, cancel);
                let _ = done_tx.send(out);
            })
            .map_err(|e| format!("Couldn't start the code sandbox: {e}."))?;
    }

    // Each call: its record, when it started, and whether it finished.
    let mut calls: Vec<(InnerToolCall, Instant, bool)> = Vec::new();
    let mut live = LiveCode::default();
    let mut running = FuturesUnordered::<Pending<'_>>::new();
    let mut tools_running = 0usize;
    let mut queued: VecDeque<Queued> = VecDeque::new();
    let start_call = |i: usize, id: f64, ns: String, name: String, args: Option<Map<String, Value>>| -> Pending<'_> {
        Box::pin(async move { (Some(i), id, Answered::Tool(host.call(ns, name, args).await)) })
    };
    let not_found = |namespace: &str, name: &str| {
        if runtime {
            format!("Tool \"{name}\" not found on {namespace}")
        } else {
            format!("Tool \"{name}\" not found")
        }
    };
    let error_json = |e: &str| json!({"error": e}).to_string();
    let outcome = loop {
        tokio::select! {
            biased;
            Some(Event::Call { id, namespace, name, args }) = ev_rx.recv() => {
                if runtime && namespace == "codemode" {
                    // codemode.search and codemode.describe: answered by the package's own code.
                    let descriptions = setup.descriptions.clone();
                    running.push(Box::pin(async move {
                        let done = tokio::task::spawn_blocking(move || util::platform(&name, &descriptions, &args)).await;
                        (None, id, Answered::Platform(done.unwrap_or_else(|e| Err(e.to_string()))))
                    }));
                    continue;
                }
                let i = calls.len();
                let method = format!("{namespace}.{name}");
                let first: Value = serde_json::from_str(&args).unwrap_or(Value::Null);
                let (server, tool) = host.route(&namespace, &name).unwrap_or_default();
                calls.push((
                    InnerToolCall { method: method.clone(), server, tool, arguments: first.clone(), result: String::new(), is_error: false, truncated: false, latency_ms: 0 },
                    Instant::now(),
                    false,
                ));
                live.calls += 1;
                // Answered at once: too many calls, an unknown method, arguments that are not an object.
                let early: Option<String> = if i >= MAX_CALLS {
                    Some(format!("Too many tool calls in one execution (the limit is {MAX_CALLS})."))
                } else if host.route(&namespace, &name).is_none() {
                    Some(not_found(&namespace, &name))
                } else {
                    match first {
                        Value::Null | Value::Object(_) => None,
                        _ => Some(format!("The arguments of {method} must be an object of the tool's parameters.")),
                    }
                };
                if let Some(message) = early {
                    finish(&mut calls[i], &Err(message.clone()), limits.store_chars);
                    let _ = reply_tx.send(Reply { id, json: error_json(&message) });
                } else {
                    let args = match first { Value::Object(m) => Some(m), _ => None };
                    live.running.push(method);
                    if tools_running < PARALLEL_CALLS {
                        tools_running += 1;
                        running.push(start_call(i, id, namespace, name, args));
                    } else {
                        queued.push_back((i, id, namespace, name, args));
                    }
                }
                progress(&live);
            }
            Some((i, id, answer)) = running.next(), if !running.is_empty() => {
                let (i, answer) = match (i, answer) {
                    (_, Answered::Platform(done)) => {
                        let json = match done {
                            Ok(dispatcher) => json!({"json": dispatcher}).to_string(),
                            Err(e) => json!({"json": error_json(&e)}).to_string(),
                        };
                        let _ = reply_tx.send(Reply { id, json });
                        continue;
                    }
                    (Some(i), Answered::Tool(answer)) => (i, answer),
                    (None, Answered::Tool(_)) => continue,
                };
                tools_running = tools_running.saturating_sub(1);
                let (json, recorded) = match answer {
                    CallAnswer::Mcp(raw) => {
                        let recorded = unwrap_mcp_result(&raw);
                        (json!({"raw": raw}).to_string(), recorded)
                    }
                    CallAnswer::Thrown(e) => (error_json(&e), Err(e)),
                    CallAnswer::Fatal(e) => {
                        cancel.store(true, Ordering::SeqCst);
                        return Err(e);
                    }
                };
                finish(&mut calls[i], &recorded, limits.store_chars);
                let _ = reply_tx.send(Reply { id, json });
                if let Some(pos) = live.running.iter().position(|m| *m == calls[i].0.method) {
                    live.running.remove(pos);
                }
                if let Some((i, id, ns, name, args)) = queued.pop_front() {
                    tools_running += 1;
                    running.push(start_call(i, id, ns, name, args));
                }
                progress(&live);
            }
            out = &mut done_rx => {
                break out.unwrap_or_else(|_| Err("The code sandbox stopped unexpectedly.".to_string()));
            }
        }
    };
    let outcome = outcome?;
    // Calls the code made but did not wait for.
    while let Ok(Event::Call { namespace, name, args, .. }) = ev_rx.try_recv() {
        if runtime && namespace == "codemode" {
            continue;
        }
        let first: Value = serde_json::from_str(&args).unwrap_or(Value::Null);
        let (server, tool) = host.route(&namespace, &name).unwrap_or_default();
        calls.push((
            InnerToolCall { method: format!("{namespace}.{name}"), server, tool, arguments: first, result: String::new(), is_error: false, truncated: false, latency_ms: 0 },
            Instant::now(),
            false,
        ));
    }
    for c in calls.iter_mut().filter(|c| !c.2) {
        finish(c, &Err(LATE_CALL_MESSAGE.to_string()), limits.store_chars);
    }
    let detail = outcome.normalized.map(|code| CodeExecution {
        code,
        status: outcome.status,
        calls: calls.into_iter().map(|c| c.0).collect(),
        logs: outcome.logs,
        result: outcome.result,
        error: outcome.error,
        latency_ms: started.elapsed().as_millis() as i64,
    });
    Ok(Execution { content: outcome.content, is_error: outcome.is_error, truncated: outcome.truncated, detail })
}

/// `v` with whole floats as integers, so its JSON text reads as JavaScript writes it (5, not 5.0).
fn js_numbers(v: &Value) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 => json!(f as i64),
            _ => v.clone(),
        },
        Value::Array(a) => Value::Array(a.iter().map(js_numbers).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), js_numbers(v))).collect()),
        _ => v.clone(),
    }
}

fn finish(call: &mut (InnerToolCall, Instant, bool), answer: &Result<Value, String>, store_chars: usize) {
    let (text, is_error) = match answer {
        Ok(Value::String(s)) => (s.clone(), false),
        Ok(v) => (js_numbers(v).to_string(), false),
        Err(e) => (e.clone(), true),
    };
    let (text, truncated) = cut(text, store_chars.max(1));
    call.0.result = text;
    call.0.is_error = is_error;
    call.0.truncated = truncated;
    call.0.latency_ms = call.1.elapsed().as_millis() as i64;
    call.2 = true;
}

// ------------------------------------------------------------------------------ the thread

/// An error with its name, as the AI SDK and the runtime read it.
#[derive(Clone, Debug, Deserialize)]
struct Thrown {
    name: String,
    message: String,
}

/// What js/sandbox.js's `outcome` reads from the module's answer.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Answer {
    threw: Option<Thrown>,
    error: Option<String>,
    result_json: Option<String>,
    #[serde(default)]
    logs: Vec<String>,
    unserializable: Option<String>,
}

/// The durable runtime's record of the execution (`runtime` format).
#[derive(Clone, Debug, Deserialize, serde::Serialize)]
struct Record {
    status: String,
    error: Option<String>,
}

/// How the code ended, before rendering.
enum Ended {
    /// `evaluate()` returned and its answer was read.
    Answered(Answer),
    /// The executor threw (Cloudflare: `executor.execute` rejects).
    Threw(Thrown),
    TimedOut,
    Cancelled,
}

struct Modules;

impl Resolver for Modules {
    fn resolve<'js>(&mut self, _ctx: &Ctx<'js>, _base: &str, name: &str, _attributes: Option<ImportAttributes<'js>>) -> rquickjs::Result<String> {
        Ok(name.to_string())
    }
}

impl Loader for Modules {
    fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str, _attributes: Option<ImportAttributes<'js>>) -> rquickjs::Result<Module<'js, Declared>> {
        if name == "cloudflare:workers" {
            return Module::declare(ctx.clone(), name, WORKERS_MODULE);
        }
        // workerd's answer to an import of a module the Worker does not have.
        let message = Value::String(format!("No such module \"{name}\"."));
        Module::declare(ctx.clone(), name.replace('\0', ""), format!("throw new Error({message});"))
    }
}

fn run_thread(
    setup: &Setup,
    arguments: &str,
    ev_tx: tokio::sync::mpsc::UnboundedSender<Event>,
    reply_rx: mpsc::Receiver<Reply>,
    cancel: Arc<AtomicBool>,
) -> Result<Outcome, String> {
    let limits = &setup.limits;
    let runtime_format = setup.format == CodeFormat::Runtime;
    let refused = |content: String| Outcome {
        normalized: None,
        status: CodeStatus::Error,
        content,
        is_error: true,
        truncated: false,
        result: None,
        error: None,
        logs: Vec::new(),
    };
    if let Some(message) = util::invalid_input(setup.format, &setup.tool_name, arguments) {
        return Ok(refused(message));
    }
    let code: Value = serde_json::from_str(if arguments.trim().is_empty() { "{}" } else { arguments }).unwrap_or(Value::Null);
    let code = code.get("code").and_then(Value::as_str).unwrap_or_default().to_string();
    if runtime_format {
        // createProxyTool refuses code the durable log could not hold, before anything runs.
        let length = code.encode_utf16().count();
        if length > 1_000_000 {
            return Ok(refused(util::code_too_large(length)?));
        }
    }
    let normalized = util::normalize_code(&code);
    let timeout_ms = limits.timeout.as_millis().min(u128::from(u32::MAX)) as u64;
    let (providers, connectors): (Vec<String>, Vec<String>) = match setup.format {
        CodeFormat::CodeTool => (setup.namespaces.clone(), Vec::new()),
        CodeFormat::Runtime => (vec!["codemode".to_string()], setup.namespaces.clone()),
    };
    let built = util::executor_module(&json!({
        "normalized": normalized, "timeoutMs": timeout_ms, "providers": providers, "connectors": connectors, "runtime": runtime_format,
    }))?;

    let rt = Runtime::new().map_err(|e| format!("Couldn't start the code sandbox: {e}."))?;
    rt.set_memory_limit(limits.memory_bytes);
    rt.set_max_stack_size(JS_STACK);
    rt.set_loader(Modules, Modules);
    let started = Instant::now();
    let hard_deadline = started + limits.timeout + GRACE;
    // Interrupts end code that never yields, after the deadline or when the sample is cancelled.
    let armed = Arc::new(AtomicBool::new(true));
    let interrupted = Arc::new(AtomicBool::new(false));
    // `process.exit(code)` sets this and spins: the interrupt ends the code where it stands.
    let exited = Arc::new(parking_lot::Mutex::new(None::<i64>));
    {
        let (armed, interrupted, cancel, exited) = (armed.clone(), interrupted.clone(), cancel.clone(), exited.clone());
        rt.set_interrupt_handler(Some(Box::new(move || {
            let stop = armed.load(Ordering::Relaxed)
                && (cancel.load(Ordering::Relaxed) || Instant::now() > hard_deadline || exited.lock().is_some());
            if stop {
                interrupted.store(true, Ordering::SeqCst);
            }
            stop
        })));
    }
    let ctx = Context::full(&rt).map_err(|e| format!("Couldn't start the code sandbox: {e}."))?;
    let (ended, record) = match built {
        // A name the executor refuses: it answers with the error before loading anything.
        Err(error) => (Ended::Answered(Answer { error: Some(error), ..Answer::default() }), None),
        Ok(module) => ctx.with(|ctx| {
            run_code(&ctx, &module, &providers, &connectors, runtime_format, limits, started, hard_deadline, &ev_tx, &reply_rx, &cancel, &armed, &interrupted, &exited)
        }),
    };
    drop(ctx);
    drop(rt);
    if matches!(ended, Ended::Cancelled) {
        return Err("The code execution was cancelled.".into());
    }

    let answer = match ended {
        Ended::Answered(a) => a,
        Ended::Threw(t) => Answer { threw: Some(t), ..Answer::default() },
        // Code that never yields: the module's own race could not fire.
        Ended::TimedOut => Answer { error: Some("Execution timed out".into()), ..Answer::default() },
        Ended::Cancelled => unreachable!(),
    };
    // The code tool's host cannot send a result JSON refuses (the AI SDK throws there, and the
    // agent's turn fails): the model is told instead.
    let (error, unserializable) = match (&answer.unserializable, setup.format) {
        (Some(why), CodeFormat::CodeTool) => (Some(why.clone()), None),
        (why, _) => (answer.error.clone(), why.clone()),
    };
    let budget = |b: Option<(i64, i64)>| b.map(|(chars, tokens)| json!({"maxChars": chars, "maxTokens": tokens}));
    let mut input = json!({
        "threw": answer.threw.as_ref().map(|t| json!({"name": t.name, "message": t.message})),
        "error": error,
        "resultJson": answer.result_json,
        "logs": answer.logs,
        "resultBudget": budget(limits.budget),
        "logsBudget": budget(limits.logs_budget),
    });
    let rendered = match setup.format {
        CodeFormat::CodeTool => util::render("render", &input)?,
        CodeFormat::Runtime => {
            input["executionId"] = json!(execution_id());
            input["connectors"] = json!(setup.namespaces);
            input["execution"] = json!(record.unwrap_or(Record { status: "running".into(), error: None }));
            input["unserializable"] = json!(unserializable);
            util::render("renderRuntime", &input)?
        }
    };
    let failed_record = rendered["error"].as_str().map(str::to_string);
    let error_text = match setup.format {
        CodeFormat::CodeTool => error.or_else(|| answer.threw.as_ref().map(|t| format!("{}: {}", t.name, t.message))),
        CodeFormat::Runtime => failed_record,
    };
    let status = match (&answer.threw, error_text.as_deref()) {
        (Some(t), _) if t.message.starts_with("Failed to start Worker:") => CodeStatus::SyntaxError,
        (_, None) => CodeStatus::Ok,
        (_, Some(e)) if e.contains("Execution timed out") => CodeStatus::Timeout,
        (_, Some(e)) if e.contains("out of memory") => CodeStatus::MemoryLimit,
        (_, Some(_)) => CodeStatus::Error,
    };
    Ok(Outcome {
        normalized: Some(normalized),
        status,
        content: rendered["content"].as_str().unwrap_or_default().to_string(),
        is_error: rendered["isError"].as_bool().unwrap_or(true),
        truncated: rendered["truncated"].as_bool().unwrap_or(false),
        result: if error_text.is_none() { answer.result_json } else { None },
        error: error_text,
        logs: answer.logs,
    })
}

struct Timers {
    due: HashMap<u32, Instant>,
}

/// The message workerd gives a Worker that does not parse, from QuickJS's SyntaxError: its text
/// and, when the engine names one, its place in `executor.js`.
fn failed_to_start(ctx: &Ctx<'_>, error: rquickjs::Error) -> Thrown {
    let mut message = error.to_string();
    let mut at = String::new();
    if matches!(error, rquickjs::Error::Exception) {
        let exc = ctx.catch();
        if let Some(s) = exc.as_string().and_then(|s| s.to_string().ok()) {
            message = s;
        }
        if let Some(obj) = exc.as_object() {
            let name: String = obj.get("name").unwrap_or_else(|_| "SyntaxError".to_string());
            let text: String = obj.get("message").unwrap_or_default();
            message = format!("{name}: {text}");
            let line: Option<u32> = obj.get("lineNumber").ok();
            let column: Option<u32> = obj.get("columnNumber").ok();
            let stack: Option<String> = obj.get("stack").ok();
            let place = match (line, column) {
                (Some(l), Some(c)) => Some((l, c)),
                // QuickJS names the place in the stack: "    at executor.js:82:24".
                _ => stack.as_deref().and_then(|s| {
                    let rest = s.lines().find_map(|l| l.trim().strip_prefix("at executor.js:"))?;
                    let mut parts = rest.trim_end_matches(')').split(':');
                    Some((parts.next()?.parse().ok()?, parts.next().and_then(|c| c.parse().ok()).unwrap_or(0)))
                }),
            };
            if let Some((l, c)) = place {
                let line = l + WORKERD_LINE_OFFSET;
                at = if c > 1 { format!("\n  at executor.js:{line}:{}", c - 1) } else { format!("\n  at executor.js:{line}") };
            }
        }
    }
    Thrown { name: "Error".into(), message: format!("Failed to start Worker:\nUncaught {message}{at}") }
}

#[allow(clippy::too_many_arguments)]
fn run_code<'js>(
    ctx: &Ctx<'js>,
    module: &str,
    providers: &[String],
    connectors: &[String],
    runtime_format: bool,
    limits: &Limits,
    started: Instant,
    hard_deadline: Instant,
    ev_tx: &tokio::sync::mpsc::UnboundedSender<Event>,
    reply_rx: &mpsc::Receiver<Reply>,
    cancel: &AtomicBool,
    armed: &AtomicBool,
    interrupted: &AtomicBool,
    exited: &Arc<parking_lot::Mutex<Option<i64>>>,
) -> (Ended, Option<Record>) {
    let timers = Rc::new(RefCell::new(Timers { due: HashMap::new() }));
    let setup = || -> rquickjs::Result<Object<'js>> {
        let host = Object::new(ctx.clone())?;
        let tx = ev_tx.clone();
        host.set(
            "call",
            Function::new(ctx.clone(), move |id: f64, namespace: String, name: String, args: String| {
                let _ = tx.send(Event::Call { id, namespace, name, args });
            })?,
        )?;
        let t = timers.clone();
        host.set(
            "timer",
            Function::new(ctx.clone(), move |id: f64, ms: f64| {
                let ms = if ms.is_finite() && ms > 0.0 { ms.min(1e12) } else { 0.0 };
                t.borrow_mut().due.insert(id as u32, Instant::now() + Duration::from_secs_f64(ms / 1000.0));
            })?,
        )?;
        let t = timers.clone();
        host.set(
            "clearTimer",
            Function::new(ctx.clone(), move |id: f64| {
                t.borrow_mut().due.remove(&(id as u32));
            })?,
        )?;
        host.set("url", Function::new(ctx.clone(), crate::url::op)?)?;
        host.set(
            "random",
            Function::new(ctx.clone(), |n: f64| {
                let mut bytes = vec![0u8; (n.max(0.0) as usize).min(65536)];
                let _ = getrandom::fill(&mut bytes);
                bytes
            })?,
        )?;
        host.set("now", Function::new(ctx.clone(), move || started.elapsed().as_secs_f64() * 1000.0)?)?;
        let exit = exited.clone();
        host.set(
            "exit",
            Function::new(ctx.clone(), move |code: f64| {
                exit.lock().get_or_insert(code as i64);
            })?,
        )?;
        let config = Object::new(ctx.clone())?;
        config.set("runtime", runtime_format)?;
        config.set("providers", providers.to_vec())?;
        // Where the runtime's codemode.step calls its function (for the error it throws without one).
        let step_line = module.lines().position(|l| l.trim() == "const value = await fn();").map_or(0, |i| i as u32 + 1);
        config.set("stepLine", step_line)?;
        config.set("connectors", connectors.to_vec())?;
        host.set("config", config)?;
        let prelude: Function = ctx.eval(PRELUDE)?;
        prelude.call((host,))
    };
    let helpers = match setup() {
        Ok(h) => h,
        Err(e) => return (Ended::Threw(Thrown { name: "Error".into(), message: describe(ctx, e) }), None),
    };
    let record_of = |helpers: &Object<'js>| -> Option<Record> {
        if !runtime_format {
            return None;
        }
        let f: Function = helpers.get("execution").ok()?;
        serde_json::from_str(&f.call::<_, String>(()).ok()?).ok()
    };
    let stopped = |interrupted: &AtomicBool| -> Option<Ended> {
        if cancel.load(Ordering::SeqCst) {
            return Some(Ended::Cancelled);
        }
        if let Some(code) = *exited.lock() {
            // workerd cancels the request: the executor's call fails with this.
            let message = format!("The Node.js process.exit({code}) API was called. Canceling the request.");
            return Some(Ended::Threw(Thrown { name: "Error".into(), message }));
        }
        if interrupted.load(Ordering::SeqCst) || Instant::now() > hard_deadline {
            return Some(Ended::TimedOut);
        }
        None
    };
    // Load the module (a SyntaxError is workerd's "Failed to start Worker"), then call
    // `new CodeExecutor(ctx, env).evaluate(__dispatchers, __connectors)`.
    let declared = match Module::declare(ctx.clone(), "executor.js", module) {
        Ok(m) => m,
        Err(e) => {
            armed.store(false, Ordering::SeqCst);
            if let Some(ended) = stopped(interrupted) {
                return (ended, None);
            }
            return (Ended::Threw(failed_to_start(ctx, e)), None);
        }
    };
    let start = || -> rquickjs::Result<Promise<'js>> {
        let (evaluated, loaded) = declared.eval()?;
        while ctx.execute_pending_job() {}
        if let Some(Err(e)) = loaded.result::<JsValue>() {
            return Err(e);
        }
        let class: JsValue = evaluated.get("default")?;
        let run: Function = ctx.eval("(C, d, c) => new C({}, {}).evaluate(d, c)")?;
        run.call((class, helpers.get::<_, JsValue>("dispatchers")?, helpers.get::<_, JsValue>("connectors")?))
    };
    let promise = match start() {
        Ok(p) => p,
        Err(e) => {
            let thrown = Thrown { name: "Error".into(), message: describe(ctx, e) };
            armed.store(false, Ordering::SeqCst);
            return (stopped(interrupted).unwrap_or(Ended::Threw(thrown)), record_of(&helpers));
        }
    };
    let deliver: Option<Function> = helpers.get("deliver").ok();
    let fire: Option<Function> = helpers.get("fire").ok();
    let mut settled_at = None;
    let ended = loop {
        while ctx.execute_pending_job() {}
        if let Some(e) = stopped(interrupted) {
            break e;
        }
        if promise.state() != PromiseState::Pending {
            settled_at = Some(());
            break Ended::Answered(Answer::default());
        }
        let now = Instant::now();
        let next_timer = timers.borrow().due.values().min().copied();
        let wake = next_timer.map_or(hard_deadline, |t| t.min(hard_deadline));
        match reply_rx.recv_timeout(wake.saturating_duration_since(now)) {
            Ok(Reply { id, json }) => {
                if let Some(d) = &deliver {
                    let _ = d.call::<_, ()>((id, json));
                    let _ = ctx.catch();
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break Ended::Cancelled,
        }
        let now = Instant::now();
        let mut due: Vec<(Instant, u32)> =
            timers.borrow().due.iter().filter(|(_, at)| **at <= now).map(|(id, at)| (*at, *id)).collect();
        due.sort_unstable();
        for (_, id) in due {
            timers.borrow_mut().due.remove(&id);
            if let Some(f) = &fire {
                // An exception in a timer callback is reported and dropped, as in Workers.
                let _ = f.call::<_, ()>((id,));
                let _ = ctx.catch();
            }
            while ctx.execute_pending_job() {}
        }
    };
    armed.store(false, Ordering::SeqCst);
    // The code may have used all its memory: make room to read the outcome.
    unsafe {
        let rt = rquickjs::qjs::JS_GetRuntime(ctx.as_raw().as_ptr());
        rquickjs::qjs::JS_SetMemoryLimit(rt, (limits.memory_bytes + RENDER_MEMORY) as _);
    }
    let record = record_of(&helpers);
    let ended = if settled_at.is_some() { settled(ctx, &promise, &helpers) } else { ended };
    (ended, record)
}

/// The module's answer as js/sandbox.js reads it, or the executor's failure when it rejected.
fn settled<'js>(ctx: &Ctx<'js>, promise: &Promise<'js>, helpers: &Object<'js>) -> Ended {
    let read = |name: &str, value: JsValue<'js>| -> Result<String, String> {
        let f: Function = helpers.get(name).map_err(|e| describe(ctx, e))?;
        f.call::<_, String>((value,)).map_err(|e| describe(ctx, e))
    };
    let failed = |message: String| Ended::Threw(Thrown { name: "Error".into(), message });
    match promise.result::<JsValue>() {
        Some(Ok(answer)) => match read("outcome", answer).and_then(|json| serde_json::from_str::<Answer>(&json).map_err(|e| e.to_string())) {
            Ok(mut a) => match a.threw.take() {
                Some(t) => Ended::Threw(t),
                None => Ended::Answered(a),
            },
            Err(e) => failed(e),
        },
        Some(Err(_)) => {
            let thrown = ctx.catch();
            match read("rejection", thrown).and_then(|json| serde_json::from_str::<Thrown>(&json).map_err(|e| e.to_string())) {
                Ok(t) => Ended::Threw(t),
                Err(e) => failed(e),
            }
        }
        None => failed("The execution did not finish.".into()),
    }
}
