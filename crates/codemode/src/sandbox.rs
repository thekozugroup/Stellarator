//! One code execution: a fresh QuickJS runtime and context on a thread of its own, running
//! Cloudflare's executor harness (`DynamicWorkerExecutor` in packages/codemode/src/executor.ts)
//! around the model's normalized code, with the namespace proxies dispatching to the run's MCP
//! tools through [`Host`]. Then what the model receives is rendered as Cloudflare's `runCode`
//! returns it and the AI SDK sends it.
//!
//! Limits: the execution's timeout (the harness's own `setTimeout` race, as Cloudflare's, plus an
//! interrupt shortly after for code that never yields, such as `while (true) {}`), a memory limit,
//! a stack limit, at most [`MAX_CALLS`] tool calls, at most [`PARALLEL_CALLS`] of them at a time.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use rquickjs::promise::PromiseState;
use rquickjs::{Coerced, Context, Ctx, Function, Object, Promise, Runtime, Value as JsValue};
use serde_json::{json, Map, Value};
use stellarator_api::{CodeExecution, CodeStatus, InnerToolCall, LiveCode};

use crate::util::{self, describe, CLOUDFLARE, HELPERS};

const SANDBOX: &str = include_str!("../js/sandbox.js");

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
/// Extra memory for reading the logs and rendering the result after the code ran out.
const RENDER_MEMORY: usize = 256 * 1024 * 1024;

/// An execution's limits.
#[derive(Clone, Debug)]
pub struct Limits {
    pub timeout: Duration,
    pub memory_bytes: usize,
    /// truncateResult's budget: Some((max_chars, max_tokens)); None for no truncation.
    pub budget: Option<(i64, i64)>,
    /// Inner call results are stored cut to this many characters.
    pub store_chars: usize,
}

/// The answer to one tool call of the code.
#[derive(Clone, Debug, PartialEq)]
pub enum CallAnswer {
    /// The value the code gets.
    Value(Value),
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

/// Run one call of the code tool: `arguments` is the JSON text the model wrote for
/// `{"code": string}`. `progress` hears the tool calls as they start and finish. Err: the sample
/// must fail (an MCP server could not be reached, or the sandbox could not start).
pub async fn execute(
    tool_name: &str,
    arguments: &str,
    namespaces: &[String],
    limits: &Limits,
    host: &dyn Host,
    progress: &(dyn Fn(&LiveCode) + Sync),
) -> Result<Execution, String> {
    let started = Instant::now();
    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let (reply_tx, reply_rx) = mpsc::channel::<Reply>();
    let (done_tx, mut done_rx) = tokio::sync::oneshot::channel::<Result<Outcome, String>>();
    let cancel = Arc::new(AtomicBool::new(false));
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    {
        let (tool_name, arguments, namespaces, limits, cancel) =
            (tool_name.to_string(), arguments.to_string(), namespaces.to_vec(), limits.clone(), cancel.clone());
        std::thread::Builder::new()
            .name("codemode".into())
            .stack_size(THREAD_STACK)
            .spawn(move || {
                let out = run_thread(&tool_name, &arguments, &namespaces, &limits, ev_tx, reply_rx, cancel);
                let _ = done_tx.send(out);
            })
            .map_err(|e| format!("Couldn't start the code sandbox: {e}."))?;
    }

    // Each call: its record, when it started, and whether it finished.
    let mut calls: Vec<(InnerToolCall, Instant, bool)> = Vec::new();
    let mut live = LiveCode::default();
    let mut running = FuturesUnordered::<BoxFuture<'_, (usize, f64, CallAnswer)>>::new();
    let mut queued: VecDeque<Queued> = VecDeque::new();
    let start_call = |i: usize, id: f64, ns: String, name: String, args: Option<Map<String, Value>>| -> BoxFuture<'_, (usize, f64, CallAnswer)> {
        Box::pin(async move { (i, id, host.call(ns, name, args).await) })
    };
    let answer_json = |a: &Result<Value, String>| match a {
        Ok(v) => json!({"result": v}).to_string(),
        Err(e) => json!({"error": e}).to_string(),
    };
    let outcome = loop {
        tokio::select! {
            biased;
            Some(Event::Call { id, namespace, name, args }) = ev_rx.recv() => {
                let i = calls.len();
                let method = format!("{namespace}.{name}");
                let parsed: Value = serde_json::from_str(&args).unwrap_or(Value::Null);
                let first = parsed.as_array().and_then(|a| a.first()).cloned().unwrap_or(Value::Null);
                let (server, tool) = host.route(&namespace, &name).unwrap_or_default();
                calls.push((
                    InnerToolCall { method: method.clone(), server, tool, arguments: first.clone(), result: String::new(), is_error: false, truncated: false, latency_ms: 0 },
                    Instant::now(),
                    false,
                ));
                live.calls += 1;
                // Answered at once: too many calls, an unknown method, arguments that are not an object.
                let early: Option<Result<Value, String>> = if i >= MAX_CALLS {
                    Some(Err(format!("Too many tool calls in one execution (the limit is {MAX_CALLS}).")))
                } else if host.route(&namespace, &name).is_none() {
                    Some(Err(format!("Tool \"{name}\" not found")))
                } else {
                    match first {
                        Value::Null => None,
                        Value::Object(_) => None,
                        _ => Some(Err(format!("The arguments of {method} must be an object of the tool's parameters."))),
                    }
                };
                if let Some(answer) = early {
                    finish(&mut calls[i], &answer, limits.store_chars);
                    let _ = reply_tx.send(Reply { id, json: answer_json(&answer) });
                } else {
                    let args = match first { Value::Object(m) => Some(m), _ => None };
                    live.running.push(method);
                    if running.len() < PARALLEL_CALLS {
                        running.push(start_call(i, id, namespace, name, args));
                    } else {
                        queued.push_back((i, id, namespace, name, args));
                    }
                }
                progress(&live);
            }
            Some((i, id, answer)) = running.next(), if !running.is_empty() => {
                let answer = match answer {
                    CallAnswer::Value(v) => Ok(v),
                    CallAnswer::Thrown(e) => Err(e),
                    CallAnswer::Fatal(e) => {
                        cancel.store(true, Ordering::SeqCst);
                        return Err(e);
                    }
                };
                finish(&mut calls[i], &answer, limits.store_chars);
                let _ = reply_tx.send(Reply { id, json: answer_json(&answer) });
                if let Some(pos) = live.running.iter().position(|m| *m == calls[i].0.method) {
                    live.running.remove(pos);
                }
                if let Some((i, id, ns, name, args)) = queued.pop_front() {
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
        let parsed: Value = serde_json::from_str(&args).unwrap_or(Value::Null);
        let first = parsed.as_array().and_then(|a| a.first()).cloned().unwrap_or(Value::Null);
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

/// How the code ended, before rendering.
enum Ended {
    /// The harness returned: `error` is its `err.message` as text when truthy.
    Returned { result: Option<String>, error: Option<String> },
    /// The executor threw (Cloudflare: `executor.execute` rejects): the error's `toString()`.
    Threw(String),
    TimedOut,
    Cancelled,
}

fn run_thread(
    tool_name: &str,
    arguments: &str,
    namespaces: &[String],
    limits: &Limits,
    ev_tx: tokio::sync::mpsc::UnboundedSender<Event>,
    reply_rx: mpsc::Receiver<Reply>,
    cancel: Arc<AtomicBool>,
) -> Result<Outcome, String> {
    if let Some(message) = util::invalid_input(tool_name, arguments) {
        return Ok(Outcome {
            normalized: None,
            status: CodeStatus::Error,
            content: message,
            is_error: true,
            truncated: false,
            result: None,
            error: None,
            logs: Vec::new(),
        });
    }
    let code: Value = serde_json::from_str(if arguments.trim().is_empty() { "{}" } else { arguments }).unwrap_or(Value::Null);
    let code = code.get("code").and_then(Value::as_str).unwrap_or_default().to_string();
    let normalized = util::normalize_code(&code);

    let rt = Runtime::new().map_err(|e| format!("Couldn't start the code sandbox: {e}."))?;
    rt.set_memory_limit(limits.memory_bytes);
    rt.set_max_stack_size(JS_STACK);
    let started = Instant::now();
    let hard_deadline = started + limits.timeout + GRACE;
    // Interrupts end code that never yields, after the deadline or when the sample is cancelled.
    let armed = Arc::new(AtomicBool::new(true));
    let interrupted = Arc::new(AtomicBool::new(false));
    {
        let (armed, interrupted, cancel) = (armed.clone(), interrupted.clone(), cancel.clone());
        rt.set_interrupt_handler(Some(Box::new(move || {
            let stop = armed.load(Ordering::Relaxed) && (cancel.load(Ordering::Relaxed) || Instant::now() > hard_deadline);
            if stop {
                interrupted.store(true, Ordering::SeqCst);
            }
            stop
        })));
    }
    let ctx = Context::full(&rt).map_err(|e| format!("Couldn't start the code sandbox: {e}."))?;
    let (ended, logs) = ctx.with(|ctx| {
        run_code(&ctx, &normalized, namespaces, limits, hard_deadline, &ev_tx, &reply_rx, &cancel, &armed, &interrupted)
    });
    drop(ctx);
    if matches!(ended, Ended::Cancelled) {
        return Err("The code execution was cancelled.".into());
    }

    // Render in a fresh context, with room to work even when the code used all its memory.
    rt.run_gc();
    rt.set_memory_limit(limits.memory_bytes + RENDER_MEMORY);
    armed.store(false, Ordering::SeqCst);
    let (status, threw, error, result) = match ended {
        Ended::Returned { result, error } => {
            let status = match error.as_deref() {
                None => CodeStatus::Ok,
                Some("Execution timed out") => CodeStatus::Timeout,
                Some(e) if e.contains("out of memory") => CodeStatus::MemoryLimit,
                Some(_) => CodeStatus::Error,
            };
            (status, None, error, result)
        }
        Ended::TimedOut => (CodeStatus::Timeout, None, Some("Execution timed out".to_string()), None),
        Ended::Threw(text) => {
            let status = if text.contains("out of memory") {
                CodeStatus::MemoryLimit
            } else if text.starts_with("SyntaxError") {
                CodeStatus::SyntaxError
            } else {
                CodeStatus::Error
            };
            (status, Some(text), None, None)
        }
        Ended::Cancelled => unreachable!(),
    };
    let render = Context::full(&rt).map_err(|e| format!("Couldn't start the code sandbox: {e}."))?;
    let input = json!({
        "threw": threw,
        "error": error,
        "resultJson": result,
        "logs": logs,
        "maxChars": limits.budget.map(|b| b.0),
        "maxTokens": limits.budget.map(|b| b.1).unwrap_or(6000),
    });
    let rendered: Result<String, String> = render.with(|ctx| {
        for src in [CLOUDFLARE, HELPERS] {
            ctx.eval::<(), _>(src).map_err(|e| describe(&ctx, e))?;
        }
        let helpers: Object = ctx.eval("__stellarator").map_err(|e| describe(&ctx, e))?;
        let f: Function = helpers.get("render").map_err(|e| describe(&ctx, e))?;
        f.call::<_, String>((input.to_string(),)).map_err(|e| describe(&ctx, e))
    });
    let rendered: Value = serde_json::from_str(&rendered?).map_err(|e| e.to_string())?;
    let error_text = error.or_else(|| threw.clone());
    Ok(Outcome {
        normalized: Some(normalized),
        status,
        content: rendered["content"].as_str().unwrap_or_default().to_string(),
        is_error: rendered["isError"].as_bool().unwrap_or(true),
        truncated: rendered["truncated"].as_bool().unwrap_or(false),
        result,
        error: error_text,
        logs,
    })
}

/// Cloudflare's harness (the generated `executor.js` module's `evaluate`), as a function of the
/// dispatchers, the log list and the codec functions.
fn harness(code: &str, namespaces: &[String], timeout_ms: u64) -> String {
    let mut lines: Vec<String> = vec![
        "(async function evaluate(__dispatchers, __logs, __stringifyForCodemode, __parseForCodemode) {".into(),
        "    console.log = (...a) => { __logs.push(a.map(String).join(\" \")); };".into(),
        "    console.warn = (...a) => { __logs.push(\"[warn] \" + a.map(String).join(\" \")); };".into(),
        "    console.error = (...a) => { __logs.push(\"[error] \" + a.map(String).join(\" \")); };".into(),
    ];
    for ns in namespaces {
        lines.push(format!(
            "    const {ns} = new Proxy({{}}, {{\n      get: (target, toolName) => {{\n        if (Object.prototype.hasOwnProperty.call(target, toolName)) return target[toolName];\n        if (typeof toolName !== \"string\") return undefined;\n        return async (...args) => {{\n          const resJson = await __dispatchers.{ns}.call(String(toolName), __stringifyForCodemode(args));\n          const data = __parseForCodemode(resJson);\n          if (data.error) throw new Error(data.error);\n          return data.result;\n        }};\n      }}\n    }});"
        ));
    }
    lines.extend([
        String::new(),
        "    try {".into(),
        "      const result = await Promise.race([".into(),
        "        (".into(),
        code.to_string(),
        ")(),".into(),
        format!("        new Promise((_, reject) => setTimeout(() => reject(new Error(\"Execution timed out\")), {timeout_ms}))"),
        "      ]);".into(),
        "      return { result, logs: __logs };".into(),
        "    } catch (err) {".into(),
        "      return { result: undefined, error: err.message, logs: __logs };".into(),
        "    }".into(),
        "})".into(),
    ]);
    lines.join("\n")
}

struct Timers {
    due: HashMap<u32, Instant>,
}

#[allow(clippy::too_many_arguments)]
fn run_code<'js>(
    ctx: &Ctx<'js>,
    normalized: &str,
    namespaces: &[String],
    limits: &Limits,
    hard_deadline: Instant,
    ev_tx: &tokio::sync::mpsc::UnboundedSender<Event>,
    reply_rx: &mpsc::Receiver<Reply>,
    cancel: &AtomicBool,
    armed: &AtomicBool,
    interrupted: &AtomicBool,
) -> (Ended, Vec<String>) {
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
        host.set("namespaces", namespaces.to_vec())?;
        let prelude: Function = ctx.eval(SANDBOX)?;
        prelude.call((host,))
    };
    let helpers = match setup() {
        Ok(h) => h,
        Err(e) => return (Ended::Threw(describe(ctx, e)), Vec::new()),
    };
    let logs_of = |helpers: &Object<'js>| -> Vec<String> {
        helpers
            .get::<_, Vec<Coerced<String>>>("logs")
            .map(|l| l.into_iter().map(|c| c.0).collect())
            .unwrap_or_default()
    };
    let timeout_ms = limits.timeout.as_millis().min(u128::from(u32::MAX)) as u64;
    let evaluate: Function = match ctx.eval(harness(normalized, namespaces, timeout_ms)) {
        Ok(f) => f,
        Err(e) => {
            let text = describe(ctx, e);
            armed.store(false, Ordering::SeqCst);
            if interrupted.load(Ordering::SeqCst) {
                return (if cancel.load(Ordering::SeqCst) { Ended::Cancelled } else { Ended::TimedOut }, Vec::new());
            }
            return (Ended::Threw(text), Vec::new());
        }
    };
    let get = |k: &str| helpers.get::<_, JsValue>(k).unwrap_or_else(|_| JsValue::new_undefined(ctx.clone()));
    let promise: rquickjs::Result<Promise> =
        evaluate.call((get("dispatchers"), get("logs"), get("stringify"), get("parse")));
    let stopped = |interrupted: &AtomicBool| -> Option<Ended> {
        if cancel.load(Ordering::SeqCst) {
            return Some(Ended::Cancelled);
        }
        if interrupted.load(Ordering::SeqCst) || Instant::now() > hard_deadline {
            return Some(Ended::TimedOut);
        }
        None
    };
    let promise = match promise {
        Ok(p) => p,
        Err(e) => {
            let text = describe(ctx, e);
            armed.store(false, Ordering::SeqCst);
            let ended = stopped(interrupted).unwrap_or(Ended::Threw(text));
            return (ended, logs_of(&helpers));
        }
    };
    let deliver: Option<Function> = helpers.get("deliver").ok();
    let fire: Option<Function> = helpers.get("fire").ok();
    let ended = loop {
        while ctx.execute_pending_job() {}
        if let Some(e) = stopped(interrupted) {
            break e;
        }
        if promise.state() != PromiseState::Pending {
            break settled(ctx, &promise, &helpers);
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
    // The code may have used all its memory: make room to read the logs.
    unsafe {
        let rt = rquickjs::qjs::JS_GetRuntime(ctx.as_raw().as_ptr());
        rquickjs::qjs::JS_SetMemoryLimit(rt, (limits.memory_bytes + RENDER_MEMORY) as _);
    }
    let logs = logs_of(&helpers);
    (ended, logs)
}

/// The harness's answer, `{result, error?, logs}`, or the executor's failure when it rejected.
fn settled<'js>(ctx: &Ctx<'js>, promise: &Promise<'js>, helpers: &Object<'js>) -> Ended {
    match promise.result::<JsValue>() {
        Some(Ok(answer)) => {
            let read = || -> rquickjs::Result<(Option<String>, Option<String>)> {
                let outcome: Function = helpers.get("outcome")?;
                let o: Object = outcome.call((answer,))?;
                Ok((o.get("result")?, o.get("error")?))
            };
            match read() {
                Ok((result, error)) => Ended::Returned { result, error },
                Err(e) => Ended::Threw(describe(ctx, e)),
            }
        }
        Some(Err(e)) => Ended::Threw(describe(ctx, e)),
        None => Ended::Threw("The execution did not finish.".into()),
    }
}
