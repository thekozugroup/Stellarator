// The sandbox's host side, set up before the model's code runs. It stands where Workers RPC and
// the host Worker stand in Cloudflare's setup:
// - timers (setTimeout and friends);
// - `__dispatchers` and `__connectors`, the two arguments of the executor module's `evaluate()`:
//   Cloudflare's ToolDispatcher per provider and connector binding per connector, reached over
//   `host` instead of Workers RPC;
// - the durable runtime's bookkeeping for one execution, as far as code without approvals can see
//   it: the sequence of calls and steps, and the size limit of what it records;
// - what Workers RPC does to the value `evaluate()` returns before the host reads it.
//
// This file is the last part of the prelude function (after cloudflare-runtime.js, v8-json.js and
// workers-globals.js, which it uses). `host` is Stellarator's: call(id, namespace, name, argsJson),
// timer(id, ms), clearTimer(id), url(...), random(n), now(), and `config`:
// {runtime: boolean, providers: [name], connectors: [name]}. The object returned at the end is kept
// by Stellarator, outside the code's reach.

const stringify = JSON.stringify;
const parseJson = JSON.parse;
const runtime = Boolean(host.config.runtime);

// ------------------------------------------------------------------ timers
const timers = new Map();
let nextTimer = 1;
const delayOf = (ms) => {
  const n = Number(ms);
  return Number.isFinite(n) && n > 0 ? n : 0;
};
const immediates = new WeakMap();
/** What setImmediate returns (an object, as Node's; the timers return numbers). */
class Immediate {
  hasRef() {
    return true;
  }
  ref() {
    return this;
  }
  unref() {
    return this;
  }
}
const schedule = (name, repeat) =>
  ({
    [name]: function (...input) {
      const fn = input[0];
      if (typeof fn !== "function") throw idlError(name, SCOPE, 1, "function");
      const immediate = name === "setImmediate";
      const id = nextTimer++;
      const delay = immediate ? 0 : delayOf(input[1]);
      timers.set(id, { fn, args: input.slice(immediate ? 1 : 2), repeat, delay });
      host.timer(id, delay);
      if (!immediate) return id;
      const handle = new Immediate();
      immediates.set(handle, id);
      return handle;
    },
  })[name];
const clear = (name) =>
  ({
    [name]: function (handle) {
      const id = typeof handle === "object" && handle !== null ? immediates.get(handle) : Number(handle);
      if (timers.delete(id)) host.clearTimer(id);
    },
  })[name];
define(G, "setTimeout", schedule("setTimeout", false));
define(G, "setInterval", schedule("setInterval", true));
define(G, "setImmediate", schedule("setImmediate", false));
define(G, "clearTimeout", clear("clearTimeout"));
define(G, "clearInterval", clear("clearInterval"));
define(G, "clearImmediate", clear("clearImmediate"));
if (typeof G.queueMicrotask !== "function") {
  define(G, "queueMicrotask", function queueMicrotask(fn) {
    Promise.resolve().then(fn);
  });
}

// ------------------------------------------------------------------ calls to the host
const pending = new Map();
let nextCall = 1;
/** Ask Stellarator for `namespace.name(args)`; resolves with its reply, parsed. */
const ask = (namespace, name, argsJson) =>
  new Promise((resolve) => {
    const id = nextCall++;
    pending.set(id, resolve);
    host.call(id, namespace, name, argsJson);
  });
/** The JSON of a tool call's arguments, as the host's MCP client writes them. */
const toJson = (value) => stringify(value);
const messageOf = (error) => (error instanceof Error ? error.message : String(error));

// The durable runtime's record of this execution (CodemodeRuntime's decide, recordResult, #fail).
const execution = { status: "running", error: null, cursor: 0, steps: new Map() };
const failExecution = (message) => {
  if (execution.status !== "running") return;
  execution.status = "error";
  execution.error = message.slice(0, MAX_DURABLE_VALUE_BYTES);
};
/** decide() for a call that needs no approval: true to execute, false for a pause decision. */
const decide = (what, args) => {
  if (execution.status !== "running") return false;
  try {
    toStored(what, args);
  } catch (error) {
    failExecution(messageOf(error));
    return false;
  }
  return true;
};
const record = (what, result) => {
  try {
    toStored(what, result);
  } catch (error) {
    failExecution(messageOf(error));
  }
};

const __dispatchers = {};
const __connectors = {};
for (const name of host.config.providers) {
  if (runtime && name === "codemode") {
    // createPlatformProvider's functions behind a ToolDispatcher.
    __dispatchers[name] = {
      call: async (method, argsJson) => {
        const text = argsJson === undefined ? "[]" : String(argsJson);
        try {
          const parsed = text ? parseForCodemode(text) : [];
          const args = Array.isArray(parsed) ? parsed : [parsed];
          switch (String(method)) {
            case "search":
            case "describe":
              return (await ask(name, String(method), text)).json;
            case "run":
              return stringifyForCodemode({ result: { error: `Snippet "${args[0]}" not found.` } });
            case "__stepDecide": {
              const seq = execution.cursor++;
              if (!decide(`Arguments to ${STEP_CONNECTOR}.${String(args[0])}`, undefined)) return stringifyForCodemode({ result: { kind: "pause", seq } });
              execution.steps.set(seq, String(args[0]));
              return stringifyForCodemode({ result: { kind: "execute", seq } });
            }
            case "__stepRecord": {
              const seq = Number(args[0]);
              if (!execution.steps.has(seq)) throw new Error(`No log entry at step ${seq}`);
              record(`The result of ${STEP_CONNECTOR}.${execution.steps.get(seq)}`, args[1]);
              return stringifyForCodemode({ result: true });
            }
            default:
              return stringifyForCodemode({ error: `Tool "${method}" not found` });
          }
        } catch (error) {
          return stringifyForCodemode({ error: messageOf(error) });
        }
      },
    };
    continue;
  }
  // ToolDispatcher.call for one provider of createCodeTool.
  __dispatchers[name] = {
    call: async (method, argsJson) => {
      try {
        const parsed = argsJson ? parseForCodemode(String(argsJson)) : [];
        const args = Array.isArray(parsed) ? parsed : [parsed];
        const reply = await ask(name, String(method), toJson(args[0]) ?? "null");
        if (reply.error !== undefined) return stringifyForCodemode({ error: reply.error });
        return stringifyForCodemode({ result: unwrapMcpResult(reply.raw) });
      } catch (error) {
        return stringifyForCodemode({ error: messageOf(error) });
      }
    },
  };
}
for (const name of host.config.connectors) {
  // buildConnectorBindings' handler: decide, execute on the connector, record.
  __connectors[name] = {
    callTool: async (method, args) => {
      // The argument crosses Workers RPC before the host sees it: what cannot be sent throws here,
      // in the code.
      wire(args, new Map());
      try {
        execution.cursor++;
        if (!decide(`Arguments to ${name}.${method}`, args)) return { [CONTROL_KEY]: "pause" };
        const reply = await ask(name, String(method), toJson(args) ?? "null");
        if (reply.error !== undefined) throw new Error(reply.error);
        const result = unwrapMcpResult(reply.raw);
        record(`The result of ${name}.${method}`, result);
        return result;
      } catch (error) {
        return { [CONTROL_KEY]: "error", message: messageOf(error) };
      }
    },
  };
}

// ------------------------------------------------------------------ the result, through Workers RPC
// evaluate() returns {result, error?, logs} to the host over Workers RPC, which copies it: plain
// data stays, a function becomes a stub (JSON has no place for it), and an instance of a class of
// the code's own cannot be sent at all. `wire` makes the value JSON.stringify then sees on the host.
const STUB = Symbol("stub");
const serializationError = (value) => new DOMException(`Could not serialize object of type "${value.constructor && value.constructor.name ? value.constructor.name : "Object"}". This type does not support serialization.`, "DataCloneError");
function wire(value, seen) {
  if (typeof value === "symbol") throw uncloneable(value);
  if (typeof value === "function") return STUB;
  if (value === null || typeof value !== "object") return value;
  if (seen.has(value)) return seen.get(value);
  const tag = tagOf(value);
  if (value instanceof Date || value instanceof Boolean || value instanceof Number || value instanceof String || ArrayBuffer.isView(value)) {
    if (value instanceof DataView) return {};
    return value;
  }
  if (value instanceof Map || value instanceof Set || value instanceof RegExp || value instanceof Error || value instanceof ArrayBuffer) return {};
  if (Array.isArray(value)) {
    const copy = new Array(value.length);
    seen.set(value, copy);
    for (let i = 0; i < value.length; i++) {
      const item = wire(value[i], seen);
      copy[i] = item === STUB ? undefined : item;
    }
    return copy;
  }
  // Values V8's serializer refuses by kind; then anything that is not a plain object.
  if (["Promise", "WeakMap", "WeakSet", "WeakRef", "Generator", "AsyncGenerator", "Arguments", "FinalizationRegistry"].includes(tag)) throw uncloneable(value);
  const proto = Object.getPrototypeOf(value);
  if (proto !== Object.prototype) throw serializationError(proto === null ? {} : value);
  const copy = {};
  seen.set(value, copy);
  for (const key of Object.keys(value)) {
    const item = wire(value[key], seen);
    // A method named toJSON arrives as a stub, and JSON.stringify calls it: nothing comes back.
    if (item === STUB && key === "toJSON") return STUB;
    if (item !== STUB) copy[key] = item;
  }
  return copy;
}

// In a Worker the Web APIs are properties of the global scope's prototype, not of globalThis
// itself: Object.getOwnPropertyNames(globalThis) lists the language's globals and a few of
// Workers' own.
{
  class EventTarget {}
  class WorkerGlobalScope extends EventTarget {}
  class ServiceWorkerGlobalScope extends WorkerGlobalScope {
    get [Symbol.toStringTag]() {
      return "ServiceWorkerGlobalScope";
    }
  }
  const scope = ServiceWorkerGlobalScope.prototype;
  define(G, "ServiceWorkerGlobalScope", ServiceWorkerGlobalScope);
  // A Worker's origin is opaque.
  define(G, "origin", "null");
  delete G[Symbol.toStringTag];
  for (const name of [
    "atob", "btoa", "fetch", "setTimeout", "setInterval", "setImmediate", "clearTimeout", "clearInterval", "clearImmediate", "queueMicrotask",
    "structuredClone", "URL", "URLSearchParams", "TextEncoder", "TextDecoder", "DOMException", "Crypto",
  ]) {
    const descriptor = Object.getOwnPropertyDescriptor(G, name);
    if (descriptor && delete G[name]) Object.defineProperty(scope, name, descriptor);
  }
  Object.setPrototypeOf(G, scope);
  // QuickJS's own error class for its limits; V8 has no such global.
  delete G.InternalError;
}

return {
  dispatchers: __dispatchers,
  connectors: __connectors,
  /** Stellarator's reply to call `id`: JSON `{raw}` (an MCP result), `{error}` or `{json}`. */
  deliver(id, json) {
    const resolve = pending.get(id);
    pending.delete(id);
    if (resolve) resolve(parseJson(json));
  },
  fire(id) {
    const t = timers.get(id);
    if (!t) return;
    if (t.repeat) host.timer(id, t.delay);
    else timers.delete(id);
    t.fn(...t.args);
  },
  /**
   * What the host reads from evaluate()'s answer, as JSON: {threw, error, resultJson, logs,
   * unserializable}. `threw` ({name, message}) when the answer cannot cross (the executor
   * rejects); else `error` is the execution's error as text (null when it has none) and
   * `resultJson` the result as JSON (null for undefined). `unserializable`: why the result has no
   * JSON form (the runtime format answers that as data; the code tool's host fails on it).
   */
  outcome(answer) {
    const logs = [];
    try {
      if (Array.isArray(answer.logs)) for (const line of answer.logs) logs.push(typeof line === "string" ? line : String(line));
    } catch (error) {
      // A log entry the code replaced with something that has no string form.
    }
    const none = { threw: null, error: null, resultJson: null, logs, unserializable: null };
    let sent;
    try {
      sent = { result: wire(answer.result, new Map()), error: wire(answer.error, new Map()), logs: wire(answer.logs, new Map()) };
    } catch (error) {
      return stringify({ ...none, threw: { name: String(error.name), message: String(error.message) } });
    }
    if (sent.error !== STUB && sent.error) {
      let text;
      try {
        text = `${sent.error}`;
      } catch (error) {
        text = "[object Object]";
      }
      return stringify({ ...none, error: text });
    }
    const result = sent.result === STUB ? undefined : sent.result;
    try {
      const json = runtime ? stringify(result, (_key, v) => (typeof v === "bigint" ? v.toString() : v)) : stringify(result);
      return stringify({ ...none, resultJson: json === undefined ? null : json });
    } catch (error) {
      return stringify({ ...none, unserializable: messageOf(error) });
    }
  },
  /**
   * evaluate() rejected: the error's name and message. Its `catch` reads `err.message`, which
   * throws for a thrown null or undefined: that TypeError's text is V8's in Workers.
   */
  rejection(error) {
    let name = "Error";
    let message = "";
    try {
      name = String(error.name);
      message = String(error.message);
    } catch (e) {
      message = "The execution failed.";
    }
    const read = /^cannot read property 'message' of (null|undefined)$/.exec(message);
    if (name === "TypeError" && read) message = `Cannot read properties of ${read[1]} (reading 'message')`;
    return stringify({ name, message });
  },
  /** The durable runtime's record of the execution, as JSON: {status, error}. */
  execution() {
    return stringify({ status: execution.status, error: execution.error });
  },
};
