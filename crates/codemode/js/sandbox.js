// The sandbox's host side, set up before the model's code runs: the globals a Cloudflare Dynamic
// Worker offers that Code Mode code uses (setTimeout and friends, console), and the dispatchers
// the namespace proxies call (Cloudflare's ToolDispatcher, reached over Workers RPC there and over
// `host` here). Nothing reaches the network or the file system: fetch, connect and every other
// host API are absent.
//
// `host` is provided by Stellarator: call(id, namespace, name, argsJson), timer(id, ms),
// clearTimer(id), and namespaces (the names of the namespace globals). The returned object is
// kept by Stellarator, outside the code's reach: deliver(id, json) answers a call, fire(id) runs
// a timer.
(function (host) {
  "use strict";
  const timers = new Map();
  let nextTimer = 1;
  const delayOf = (ms) => {
    const n = Number(ms);
    return Number.isFinite(n) && n > 0 ? n : 0;
  };
  const schedule = (repeat) =>
    function (fn, ms, ...args) {
      const id = nextTimer++;
      const delay = delayOf(ms);
      timers.set(id, { fn, args, repeat, delay });
      host.timer(id, delay);
      return id;
    };
  const clear = (id) => {
    if (timers.delete(Number(id))) host.clearTimer(Number(id));
  };
  globalThis.setTimeout = schedule(false);
  globalThis.setInterval = schedule(true);
  globalThis.clearTimeout = clear;
  globalThis.clearInterval = clear;
  if (typeof globalThis.queueMicrotask !== "function") {
    globalThis.queueMicrotask = (fn) => {
      Promise.resolve().then(fn);
    };
  }
  // log, warn and error are replaced by the executor's capture (as Cloudflare's harness does);
  // the rest write nowhere, as Workers' console output is not part of the result.
  const quiet = () => {};
  globalThis.console = {
    log: quiet, warn: quiet, error: quiet, info: quiet, debug: quiet, trace: quiet, dir: quiet,
    table: quiet, group: quiet, groupEnd: quiet, time: quiet, timeEnd: quiet, assert: quiet, count: quiet,
  };

  const pending = new Map();
  let nextCall = 1;
  const dispatchers = {};
  for (const ns of host.namespaces) {
    dispatchers[ns] = {
      call: (name, argsJson) =>
        new Promise((resolve) => {
          const id = nextCall++;
          pending.set(id, resolve);
          host.call(id, ns, String(name), argsJson === undefined ? "[]" : String(argsJson));
        }),
    };
  }
  const logs = [];
  const stringify = JSON.stringify;
  return {
    dispatchers,
    logs,
    stringify: JSON.stringify,
    parse: JSON.parse,
    deliver(id, json) {
      const resolve = pending.get(id);
      pending.delete(id);
      if (resolve) resolve(json);
    },
    fire(id) {
      const t = timers.get(id);
      if (!t) return;
      if (t.repeat) host.timer(id, t.delay);
      else timers.delete(id);
      if (typeof t.fn === "function") t.fn(...t.args);
    },
    /**
     * The harness's answer as {error, result}: the error as `${error}` writes it when it is truthy
     * (DynamicWorkerExecutor checks `if (response.error)`), else the result as JSON (null for
     * undefined). A result JSON cannot hold (a BigInt, a cycle) becomes the error.
     */
    outcome(answer) {
      const text = (value) => {
        try {
          return typeof value === "string" ? value : String(value);
        } catch (e) {
          return "[unprintable error]";
        }
      };
      if (answer.error) return { error: text(answer.error), result: null };
      try {
        const json = stringify(answer.result);
        return { error: null, result: json === undefined ? null : json };
      } catch (e) {
        return { error: text(e && e.message), result: null };
      }
    },
  };
});
