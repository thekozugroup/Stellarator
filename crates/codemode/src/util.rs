//! Cloudflare's helpers that run outside the sandbox (normalizeCode with acorn, the type
//! generation, sanitizeToolName, the AI SDK's input errors), executed as published in one
//! long-lived QuickJS runtime on its own thread. Requests are served one at a time; each takes a
//! fraction of a millisecond for the inputs code mode sees, and a request that runs longer than
//! [`REQUEST_LIMIT`] is interrupted.

use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rquickjs::{Context, Ctx, Function, Object, Runtime};
use stellarator_api::CodeFormat;

const ACORN: &str = include_str!("../js/acorn.js");
const CLOUDFLARE: &str = include_str!("../js/cloudflare-codemode.js");
const RUNTIME: &str = include_str!("../js/cloudflare-runtime.js");
const V8_JSON: &str = include_str!("../js/v8-json.js");
const HELPERS: &str = include_str!("../js/stellarator.js");

/// One request may take this long before it is interrupted.
const REQUEST_LIMIT: Duration = Duration::from_secs(10);
/// The utility runtime's memory limit.
const MEMORY_LIMIT: usize = 512 * 1024 * 1024;

type Job = Box<dyn for<'js> FnOnce(&Ctx<'js>, &Object<'js>) + Send>;

struct Worker {
    tx: mpsc::Sender<Job>,
}

fn worker() -> &'static Mutex<Worker> {
    static WORKER: OnceLock<Mutex<Worker>> = OnceLock::new();
    WORKER.get_or_init(|| Mutex::new(Worker { tx: spawn() }))
}

fn spawn() -> mpsc::Sender<Job> {
    let (tx, rx) = mpsc::channel::<Job>();
    std::thread::Builder::new()
        .name("codemode-util".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || serve(rx))
        .expect("start the code mode helper thread");
    tx
}

fn serve(rx: mpsc::Receiver<Job>) {
    let rt = Runtime::new().expect("a QuickJS runtime");
    rt.set_memory_limit(MEMORY_LIMIT);
    rt.set_max_stack_size(2 * 1024 * 1024);
    let deadline = Arc::new(Mutex::new(None::<Instant>));
    {
        let deadline = deadline.clone();
        rt.set_interrupt_handler(Some(Box::new(move || deadline.lock().is_some_and(|d| Instant::now() > d))));
    }
    let ctx = Context::full(&rt).expect("a QuickJS context");
    ctx.with(|ctx| {
        let sources = [
            ("acorn.js", ACORN),
            ("cloudflare-codemode.js", CLOUDFLARE),
            ("cloudflare-runtime.js", RUNTIME),
            ("v8-json.js", V8_JSON),
            ("stellarator.js", HELPERS),
        ];
        for (name, src) in sources {
            if let Err(e) = ctx.eval::<(), _>(src) {
                panic!("loading {name}: {}", describe(&ctx, e));
            }
        }
    });
    while let Ok(job) = rx.recv() {
        *deadline.lock() = Some(Instant::now() + REQUEST_LIMIT);
        ctx.with(|ctx| {
            let helpers: Object = ctx.eval("__stellarator").expect("the helpers object");
            job(&ctx, &helpers);
        });
        *deadline.lock() = None;
    }
}

/// The text of a QuickJS error (the pending exception for `Error::Exception`).
pub(crate) fn describe(ctx: &Ctx<'_>, e: rquickjs::Error) -> String {
    if matches!(e, rquickjs::Error::Exception) {
        let exc = ctx.catch();
        if let Some(obj) = exc.as_object() {
            let name: Option<String> = obj.get("name").ok();
            let message: Option<String> = obj.get("message").ok();
            if let Some(message) = message {
                return match name {
                    Some(n) if !n.is_empty() => format!("{n}: {message}"),
                    _ => message,
                };
            }
        }
        if let Some(s) = exc.as_string().and_then(|s| s.to_string().ok()) {
            return s;
        }
        return format!("{exc:?}");
    }
    e.to_string()
}

/// Run `f` on the helper thread and wait for its answer; Err when the thread stopped while it ran.
fn run<T: Send + 'static>(
    f: impl for<'js> FnOnce(&Ctx<'js>, &Object<'js>) -> T + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    let job: Job = Box::new(move |ctx, helpers| {
        let _ = tx.send(f(ctx, helpers));
    });
    let sent = worker().lock().tx.send(job);
    if let Err(mpsc::SendError(job)) = sent {
        // The thread died (a panic in an earlier request): start a new one.
        let mut w = worker().lock();
        w.tx = spawn();
        let _ = w.tx.send(job);
    }
    rx.recv().map_err(|_| "The code mode helper stopped; try again.".to_string())
}

/// Call `helpers[name](...args)` with string arguments; Err is the exception's text.
fn call_helper(name: &'static str, args: Vec<String>) -> Result<Option<String>, String> {
    run(move |ctx, helpers| {
        let f: Function = helpers.get(name).map_err(|e| describe(ctx, e))?;
        // Called as methods of the helpers object: some use `this`.
        let this = rquickjs::function::This(helpers.clone());
        let result: rquickjs::Result<rquickjs::Value> = match args.len() {
            1 => f.call((this, args[0].as_str())),
            2 => f.call((this, args[0].as_str(), args[1].as_str())),
            3 => f.call((this, args[0].as_str(), args[1].as_str(), args[2].as_str())),
            _ => unreachable!("helpers take one to three arguments"),
        };
        let v = result.map_err(|e| describe(ctx, e))?;
        if v.is_null() || v.is_undefined() {
            return Ok(None);
        }
        v.as_string().and_then(|s| s.to_string().ok()).map(Some).ok_or_else(|| "a helper returned a non-string".to_string())
    })?
}

/// Cloudflare's `normalizeCode`: markdown fences stripped, and code that is not one arrow function
/// wrapped in `async () => {...}` (a trailing expression becomes the return value).
pub fn normalize_code(code: &str) -> String {
    match call_helper("normalize", vec![code.to_string()]) {
        Ok(Some(s)) => s,
        // Only an interrupted or out-of-memory parse gets here; normalizeCode's own fallback.
        _ => format!("async () => {{\n{}\n}}", code.trim()),
    }
}

/// Cloudflare's `sanitizeToolName`: a valid JavaScript identifier for a tool or namespace name.
pub fn sanitize_tool_name(name: &str) -> String {
    call_helper("sanitize", vec![name.to_string()]).ok().flatten().unwrap_or_else(|| "_".into())
}

fn format_name(format: CodeFormat) -> &'static str {
    match format {
        CodeFormat::CodeTool => "code_tool",
        CodeFormat::Runtime => "runtime",
    }
}

/// The AI SDK's error text for a code tool input that is not `{"code": string}`; None when valid.
/// The two formats check with different schemas, so the text differs.
pub fn invalid_input(format: CodeFormat, tool_name: &str, arguments: &str) -> Option<String> {
    match call_helper("invalidInput", vec![format_name(format).to_string(), tool_name.to_string(), arguments.to_string()]) {
        Ok(v) => v,
        Err(e) => Some(e),
    }
}

/// The `arguments` the AI SDK sends back, in the assistant message of the next request, for a tool
/// call the model made with the text `arguments`: the validated input (`{"code": ...}` alone) for
/// a call the code tool accepted (`valid`), the parsed input when it is an object, an array or
/// null, else `{}`. Written as `JSON.stringify` writes it.
pub fn resend_arguments(valid: bool, arguments: &str) -> String {
    call_helper("resendArguments", vec![if valid { "1".into() } else { String::new() }, arguments.to_string()])
        .ok()
        .flatten()
        .unwrap_or_else(|| "{}".into())
}

/// V8's message for `JSON.parse(text)`, or None when `text` is JSON.
#[doc(hidden)]
pub fn json_parse_error(text: &str) -> Option<String> {
    call_helper("jsonError", vec![text.to_string()]).ok().flatten()
}

/// The runtime's answer to code it will not record (over a million characters), as the tool's
/// output for the model.
pub(crate) fn code_too_large(length: usize) -> Result<String, String> {
    call_helper("codeTooLarge", vec![length.to_string()]).and_then(|v| v.ok_or_else(|| "no answer".to_string()))
}

/// The module `DynamicWorkerExecutor` loads for `spec`; the inner Err is the error it answers with
/// instead (a provider name it refuses).
pub(crate) fn executor_module(spec: &serde_json::Value) -> Result<Result<String, String>, String> {
    let out = call_helper("executorModule", vec![spec.to_string()])?.ok_or_else(|| "no module".to_string())?;
    let out: serde_json::Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
    Ok(match out["module"].as_str() {
        Some(m) => Ok(m.to_string()),
        None => Err(out["error"].as_str().unwrap_or("The executor refused the code.").to_string()),
    })
}

/// `__stellarator.render` or `renderRuntime`: what the model receives for one execution.
pub(crate) fn render(helper: &'static str, input: &serde_json::Value) -> Result<serde_json::Value, String> {
    let out = call_helper(helper, vec![input.to_string()])?.ok_or_else(|| "nothing rendered".to_string())?;
    serde_json::from_str(&out).map_err(|e| e.to_string())
}

/// `codemode.search(...)` or `codemode.describe(...)` of the runtime format: the dispatcher's JSON.
pub(crate) fn platform(method: &str, descriptions: &str, args_json: &str) -> Result<String, String> {
    if method != "search" && method != "describe" {
        return Ok(serde_json::json!({"error": format!("Tool \"{method}\" not found")}).to_string());
    }
    call_helper("platform", vec![method.to_string(), descriptions.to_string(), args_json.to_string()])
        .and_then(|v| v.ok_or_else(|| "no answer".to_string()))
}

/// `__stellarator.runtimeTool(spec)`: the runtime format's tool and connectors, as JSON text.
pub(crate) fn runtime_tool_json(spec: String) -> Result<String, String> {
    call_helper("runtimeTool", vec![spec]).and_then(|v| v.ok_or_else(|| "no runtime tool".to_string()))
}

/// Cloudflare's `truncateResult` of a JSON value, as JSON (`max_chars` None: the token budget).
#[doc(hidden)]
pub fn truncate_result(json: &str, max_chars: Option<i64>, max_tokens: i64) -> String {
    let mut options = serde_json::Map::new();
    if let Some(c) = max_chars {
        options.insert("maxChars".into(), c.into());
    }
    options.insert("maxTokens".into(), max_tokens.into());
    let options = serde_json::Value::Object(options).to_string();
    call_helper("truncate", vec![json.to_string(), options]).ok().flatten().unwrap_or_default()
}

/// `__stellarator.codeTool(spec)`: the description and the namespaces' methods, as JSON text.
pub(crate) fn code_tool_json(spec: String) -> Result<String, String> {
    call_helper("codeTool", vec![spec]).and_then(|v| v.ok_or_else(|| "no code tool".to_string()))
}
