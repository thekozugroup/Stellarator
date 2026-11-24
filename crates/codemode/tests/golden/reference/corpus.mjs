// The reference corpus: what "matches Cloudflare" means for Stellarator's code mode. Each case is
// one conversation with the scripted model: the code tool calls it makes, in order. run.mjs runs
// every case through the published package on workerd, once per format, and records what the
// model server received; crates/engine/tests/conformance.rs runs the same calls through
// Stellarator and compares.
//
// A case: {id, group, code | arguments | steps, ...options}
//   code       the model's code; the call's arguments are JSON.stringify({code})
//   arguments  the raw arguments text instead (invalid inputs)
//   steps      several answers: [{calls: [{arguments, name?}], content?, reasoning_content?}]
//   formats    which formats run it (default: code_tool and runtime)
//   servers    MCP servers offered, by name (default: mock and tracker)
//   timeout_ms the executor's timeout (default: Cloudflare's 60000)
//   namespaces "codemode": code_tool with one tool set instead of one provider per server
//   transform_result  runtime: createCodemodeRuntime's transformResult set to truncateResult
//   tool_options      runtime: the options of runtime.tool() (description, connectorHints)
//   driver     "stream": streamText instead of generateText
//   hang_ms    how long to wait before the case counts as hanging
//
// Results must not depend on the clock, on randomness or on the machine: cases return facts about
// such values (a type, a pattern match), not the values.

export const FORMATS = ["code_tool", "runtime", "ai_tools"];

/** The MCP servers of stellarator-mock, by the name they are added under. */
export const SERVERS = { mock: { path: "/mcp" }, tracker: { path: "/tracker/mcp" } };

const cases = [];
const add = (group, id, code, options = {}) => cases.push({ id, group, code, ...options });
const raw = (group, id, fields) => cases.push({ id, group, ...fields });
const call = (code) => ({ calls: [{ arguments: JSON.stringify({ code }) }] });
const RUNTIME = { formats: ["runtime"] };
const CODE_TOOL = { formats: ["code_tool"] };

// ---------------------------------------------------------------- results: every JSON type and more
add("results", "result-integer", "async () => 42");
add("results", "result-float", "async () => 0.1 + 0.2");
add("results", "result-negative-zero", "async () => -0");
add("results", "result-string", 'async () => "hello"');
add("results", "result-empty-string", 'async () => ""');
add("results", "result-true", "async () => true");
add("results", "result-false", "async () => false");
add("results", "result-null", "async () => null");
add("results", "result-undefined", "async () => undefined");
add("results", "result-no-return", "async () => { const x = 1; }");
add("results", "result-zero", "async () => 0");
add("results", "result-array", 'async () => [1, "two", null, true, [3, [4]], {}]');
add("results", "result-empty-array", "async () => []");
add("results", "result-empty-object", "async () => ({})");
add("results", "result-object", 'async () => ({ a: 1, b: { c: [1, 2] }, d: "é ☃ \\u{1F600} <tag> \\"q\\" \\\\ /" })');
add("results", "result-key-order", 'async () => ({ b: 1, a: 2, 10: "ten", 2: "two", "-1": "minus" })');
add("results", "result-nested-undefined", "async () => ({ a: undefined, b: [undefined, 1], c: null })");
add("results", "result-nan-infinity", "async () => ({ n: NaN, i: Infinity, m: -Infinity, list: [NaN] })");
add("results", "result-nan", "async () => NaN");
add("results", "result-numbers", "async () => [1e21, 1e-7, 123456789012345680000, 0.000001, 1.0, -1.5e-10, 9007199254740993, 5e-324, 1.7976931348623157e308, 0.1 * 3, 100, 1e3]");
add("results", "result-date", "async () => new Date(0)");
add("results", "result-date-nested", "async () => ({ at: new Date(86400000), list: [new Date(1700000000000)] })");
add("results", "result-invalid-date", 'async () => new Date("nope")');
add("results", "result-map", 'async () => new Map([["a", 1], ["b", { c: 2 }]])');
add("results", "result-set", "async () => new Set([1, 2, 2, 3])");
add("results", "result-bigint", "async () => 10n");
add("results", "result-bigint-nested", "async () => ({ n: 2n ** 64n, small: 1 })");
add("results", "result-symbol", 'async () => Symbol("x")');
add("results", "result-function", "async () => (x) => x");
add("results", "result-function-nested", "async () => ({ f: () => 1, ok: true })");
add("results", "result-class-instance", "async () => { class P { constructor() { this.x = 1; } get y() { return 2; } } return new P(); }");
add("results", "result-error-object", 'async () => new Error("as a value")');
add("results", "result-error-nested", 'async () => ({ failed: new TypeError("inner"), ok: false })');
add("results", "result-regexp", "async () => /ab+c/gi");
add("results", "result-uint8array", "async () => new Uint8Array([1, 2, 255])");
add("results", "result-arraybuffer", "async () => new Uint8Array([1, 2, 3]).buffer");
add("results", "result-float64array", "async () => new Float64Array([1.5, 2])");
add("results", "result-cycle", "async () => { const a = { name: \"loop\" }; a.self = a; return a; }");
add("results", "result-shared-reference", "async () => { const shared = { v: 1 }; return { a: shared, b: shared }; }");
add("results", "result-sparse-array", "async () => [1, , 3]");
add("results", "result-tojson", "async () => ({ toJSON() { return { viaToJSON: true }; } })");
add("results", "result-promise", "async () => Promise.resolve(5)");
add("results", "result-thenable", "async () => ({ then(resolve) { resolve(\"from then\"); } })");
add("results", "result-null-prototype", "async () => { const o = Object.create(null); o.k = 1; return o; }");
add("results", "result-getter", "async () => ({ get computed() { return 7; }, plain: 1 })");
add("results", "result-symbol-key", 'async () => ({ [Symbol("hidden")]: 1, shown: 2 })');
add("results", "result-non-enumerable", 'async () => Object.defineProperty({ shown: 1 }, "hidden", { value: 2, enumerable: false })');
add("results", "result-string-escapes", 'async () => "line\\nbreak\\ttab \\u2028 \\u2029 \\u0000 \\u001f \\u007f \\ud83d alone"');
add("results", "result-boxed", "async () => [new Number(5), new String(\"s\"), new Boolean(false), Object(1n) instanceof BigInt]");
add("results", "result-array-with-props", 'async () => { const a = [1, 2]; a.extra = "x"; return a; }');
add("results", "result-nested-deep", "async () => { let o = { leaf: true }; for (let i = 0; i < 40; i++) o = { o }; return o; }");
add("results", "result-proxy", "async () => new Proxy({ a: 1 }, {})");
add("results", "result-weakmap", "async () => new WeakMap()");
add("results", "result-generator", "async () => (function* () { yield 1; })()");
add("results", "result-arguments-object", "async () => (function () { return arguments; })(1, 2)");
add("results", "result-url-object", 'async () => new URL("https://example.com/a?b=1")');
add("results", "result-headers-object", 'async () => typeof Headers === "function" ? new Headers({ a: "b" }) : "no Headers"');
add("results", "result-long-string", 'async () => "abcdefghij".repeat(300)');

// ---------------------------------------------------------------- console
add("console", "console-log", 'async () => { console.log("hello"); return 1; }');
add("console", "console-log-values", 'async () => { console.log("a", 1, true, null, undefined, 1.5, -0, 10n); return "done"; }');
add("console", "console-log-objects", 'async () => { console.log({ x: 1 }, [1, [2, 3]], [], {}, () => 1, function named() {}, class K {}); }');
add("console", "console-log-builtins", 'async () => { console.log(Symbol("s"), new Error("e"), new TypeError("t"), /re/g, new Map([[1, 2]]), new Set([1]), Promise.resolve(1), null); }');
add("console", "console-log-date", "async () => { console.log(new Date(0)); console.log(String(new Date(1700000000000))); }");
add("console", "console-warn-error", 'async () => { console.warn("careful", 1); console.error("bad", { a: 1 }); console.log("after"); return null; }');
add("console", "console-info-debug", 'async () => { console.info("info line"); console.debug("debug line"); console.trace("trace line"); return "x"; }');
add("console", "console-other-methods", 'async () => { console.dir({ a: 1 }); console.table([1, 2]); console.assert(false, "assert"); console.count(); console.count("c"); console.countReset("c"); console.group("g"); console.groupCollapsed("gc"); console.groupEnd(); console.time("t"); console.timeLog("t"); console.timeEnd("t"); console.dirxml({}); console.timeStamp("ts"); return Object.keys(console).sort(); }');
add("console", "console-method-types", 'async () => Object.fromEntries(["log", "warn", "error", "info", "debug", "trace", "dir", "dirxml", "table", "assert", "count", "countReset", "group", "groupCollapsed", "groupEnd", "time", "timeLog", "timeEnd", "timeStamp", "clear", "profile", "context", "nope"].map((m) => [m, typeof console[m]]))');
add("console", "console-no-arguments", 'async () => { console.log(); console.warn(); console.error(); return "three empty lines"; }');
add("console", "console-format-specifiers", 'async () => { console.log("%s is %d years", "Ana", 30); console.log("%o", { a: 1 }); console.log("100%"); }');
add("console", "console-only-logs", 'async () => { console.log("just a log"); }');
add("console", "console-multiline", 'async () => { console.log("line one\\nline two"); console.log("  indented  "); console.log(""); return 0; }');
add("console", "console-custom-tostring", 'async () => { console.log({ toString() { return "custom!"; } }, { [Symbol.toPrimitive]() { return "prim"; } }); }');
add("console", "console-throwing-tostring", 'async () => { console.log("before"); console.log({ toString() { throw new Error("no string for you"); } }); return "unreached"; }');
add("console", "console-null-prototype", "async () => { console.log(Object.create(null)); return 1; }");
add("console", "console-destructured", 'async () => { const { log, warn } = console; log("via log"); warn("via warn"); return 1; }');
add("console", "console-many-lines", 'async () => { for (let i = 0; i < 40; i++) console.log("line", i); return "logged"; }');
add("console", "console-then-throw", 'async () => { console.log("step 1"); console.warn("step 2"); throw new Error("after the logs"); }');
add("console", "console-unicode", 'async () => { console.log("é ☃ \\u{1F600} \\u2028 end", "\\u0000"); return 1; }');
add("console", "console-nested-array", 'async () => { console.log([1, [2, [3, [4]]], "s", null, undefined, { a: 1 }]); }');
add("console", "console-reassigned", 'async () => { const lines = []; console.log = (...a) => lines.push(a.join("|")); console.log("x", 1); return lines; }');
add("console", "console-after-await", 'async () => { console.log("before"); await mock.add({ a: 1, b: 2 }); console.log("after"); return "ok"; }');

// ---------------------------------------------------------------- errors the code throws
add("errors", "throw-error", 'async () => { throw new Error("boom"); }');
add("errors", "throw-type-error", 'async () => { throw new TypeError("bad type"); }');
add("errors", "throw-range-error", 'async () => { throw new RangeError("out of range"); }');
add("errors", "throw-custom-error", 'async () => { class QuotaError extends Error { constructor(m) { super(m); this.name = "QuotaError"; this.code = 7; } } throw new QuotaError("quota used up"); }');
add("errors", "throw-error-with-cause", 'async () => { throw new Error("outer", { cause: new Error("inner") }); }');
add("errors", "throw-error-empty-message", 'async () => { throw new Error(""); }');
add("errors", "throw-error-no-message", "async () => { throw new Error(); }");
add("errors", "throw-error-multiline", 'async () => { throw new Error("first line\\nsecond line"); }');
add("errors", "throw-string", 'async () => { throw "plain string"; }');
add("errors", "throw-number", "async () => { throw 42; }");
add("errors", "throw-object-with-message", 'async () => { throw { code: 42, message: "object message" }; }');
add("errors", "throw-object-without-message", "async () => { throw { code: 42 }; }");
add("errors", "throw-object-numeric-message", "async () => { throw { message: 404 }; }");
add("errors", "throw-object-object-message", 'async () => { throw { message: { nested: "detail" } }; }');
add("errors", "throw-null", "async () => { throw null; }");
add("errors", "throw-undefined", "async () => { throw undefined; }");
add("errors", "throw-array", 'async () => { throw ["a", "b"]; }');
add("errors", "throw-after-logs", 'async () => { console.log("one"); console.error("two"); throw new Error("three"); }');
add("errors", "throw-in-nested-function", 'async () => { const inner = () => { throw new Error("from inner"); }; const outer = () => inner(); return outer(); }');
add("errors", "throw-non-async-arrow", '() => { throw new Error("sync throw"); }');
add("errors", "reject-awaited", 'async () => { await Promise.reject(new Error("rejected")); }');
add("errors", "reject-returned", 'async () => Promise.reject(new Error("returned rejection"))');
add("errors", "reject-unhandled", 'async () => { Promise.reject(new Error("floating")); return "returned"; }');
add("errors", "reject-unhandled-awaited-later", 'async () => { const p = Promise.reject(new Error("late handler")); await new Promise((r) => setTimeout(r, 20)); try { await p; } catch (e) { return "caught " + e.message; } }');
add("errors", "aggregate-error", 'async () => { await Promise.any([Promise.reject(new Error("a")), Promise.reject(new Error("b"))]); }');
add("errors", "throw-in-timer", 'async () => { setTimeout(() => { throw new Error("thrown in a timer"); }, 5); await new Promise((r) => setTimeout(r, 40)); return "survived"; }');
add("errors", "throw-in-microtask", 'async () => { queueMicrotask(() => { throw new Error("thrown in a microtask"); }); await new Promise((r) => setTimeout(r, 20)); return "survived"; }');

// ---------------------------------------------------------------- errors the engine throws
add("engine", "engine-property-of-undefined", "async () => { const o = undefined; return o.x; }");
add("engine", "engine-property-of-null", "async () => { const o = null; return o.name; }");
add("engine", "engine-nested-property-of-undefined", "async () => { const data = {}; return data.items.length; }");
add("engine", "engine-index-of-undefined", "async () => { const rows = undefined; return rows[0]; }");
add("engine", "engine-computed-property-of-undefined", 'async () => { const o = undefined; const k = "key"; return o[k]; }');
add("engine", "engine-set-property-of-undefined", "async () => { const o = undefined; o.x = 1; }");
add("engine", "engine-destructure-undefined", "async () => { const { a } = undefined; return a; }");
add("engine", "engine-destructure-null-array", "async () => { const [a] = null; return a; }");
add("engine", "engine-result-property-chain", 'async () => { const r = await mock.lookup({ query: "tides" }); return r.data.items[0]; }');
add("engine", "engine-not-a-function-member", "async () => { const o = {}; return o.go(); }");
add("engine", "engine-not-a-function-variable", "async () => { const f = 5; return f(); }");
add("engine", "engine-not-a-function-string-method", 'async () => "abc".reverse()');
add("engine", "engine-not-a-function-nested", "async () => { const api = { v1: {} }; return api.v1.list({}); }");
add("engine", "engine-not-a-function-result", 'async () => { const r = await mock.add({ a: 1, b: 2 }); return r.map((x) => x); }');
add("engine", "engine-not-a-function-array-method", "async () => [1, 2].contains(1)");
add("engine", "engine-not-a-constructor", "async () => new (() => {})()");
add("engine", "engine-not-iterable-number", "async () => { for (const x of 5) {} }");
add("engine", "engine-not-iterable-spread", "async () => [...undefined]");
add("engine", "engine-not-iterable-object", "async () => { const [a, b] = {}; return a; }");
add("engine", "engine-not-iterable-result", 'async () => { const r = await mock.add({ a: 1, b: 2 }); for (const x of r) {} }');
add("engine", "engine-not-iterable-promise-all", "async () => Promise.all(5)");
add("engine", "engine-not-iterable-map", "async () => new Map(5)");
add("engine", "engine-reference-error", "async () => missing + 1");
add("engine", "engine-reference-error-call", "async () => doSomething()");
add("engine", "engine-reference-error-typeof-safe", 'async () => typeof missing === "undefined"');
add("engine", "engine-reference-error-tdz", "async () => { const f = () => early; const v = f(); const early = 1; return v; }");
add("engine", "engine-assign-const", "async () => { const a = 1; a = 2; return a; }");
add("engine", "engine-assign-undeclared", "async () => { undeclared = 5; return undeclared; }");
add("engine", "engine-invalid-array-length", "async () => new Array(-1)");
add("engine", "engine-stack-overflow", "async () => { const f = () => f(); return f(); }");
add("engine", "engine-bigint-mix", "async () => 1n + 1");
add("engine", "engine-bigint-json", "async () => JSON.stringify({ n: 1n })");
add("engine", "engine-json-cycle", "async () => { const a = {}; a.a = a; return JSON.stringify(a); }");
add("engine", "engine-invalid-time", 'async () => new Date("nope").toISOString()');
add("engine", "engine-tofixed-range", "async () => (1).toFixed(200)");
add("engine", "engine-repeat-negative", 'async () => "x".repeat(-1)');
add("engine", "engine-invalid-string-length", 'async () => "x".repeat(2 ** 30)');
add("engine", "engine-uri-malformed", 'async () => decodeURIComponent("%")');
add("engine", "engine-in-operator", 'async () => "a" in 5');
add("engine", "engine-instanceof", "async () => ({}) instanceof 5");
add("engine", "engine-symbol-to-string", 'async () => "" + Symbol("s")');
add("engine", "engine-null-to-object", "async () => Object.keys(null)");
add("engine", "engine-undefined-to-object", "async () => Object.entries(undefined)");
add("engine", "engine-invalid-regexp", 'async () => new RegExp("(")');
add("engine", "engine-reduce-empty", "async () => [].reduce((a, b) => a + b)");
add("engine", "engine-class-without-new", "async () => { class A {} return A(); }");
add("engine", "engine-super-before-this", "async () => { class A {} class B extends A { constructor() { this.x = 1; super(); } } return new B(); }");
add("engine", "engine-frozen-assign", 'async () => { "use strict"; const o = Object.freeze({ a: 1 }); o.a = 2; return o.a; }');
add("engine", "engine-await-in-sync-callback", "async () => [1, 2].map(async (x) => x * 2)");
add("engine", "engine-property-forms", 'async () => { const tries = { read: () => undefined.x, readNull: () => null.name, index: () => undefined[0], computed: () => { const k = "key"; return null[k]; }, emptyKey: () => undefined[""], spacedKey: () => undefined["a b"], quoteKey: () => undefined["it\'s"], method: () => undefined.map((x) => x), nested: () => ({}).a.b.c, increment: () => { const o = {}; o.a.n++; }, set: () => { const o = {}; o.a.b = 1; }, setNull: () => { null.x = 1; }, setIndex: () => { undefined[3] = 1; }, deleteIt: () => { const o = undefined; delete o.x; }, optional: () => undefined?.x.y, chainAfterOptional: () => ({}).a?.b.c.d, templateKey: () => undefined[`k${1}`], numberKey: () => null[1.5], callResult: () => [].find((x) => x).id, jsonPath: () => JSON.parse("{}").data.items }; const out = {}; for (const [k, f] of Object.entries(tries)) { try { out[k] = ["no error", f()]; } catch (e) { out[k] = [e.name, e.message]; } } return out; }');
add("engine", "engine-property-symbol-key", 'async () => { try { return undefined[Symbol.iterator]; } catch (e) { return [e.name, e.message]; } }');
add("engine", "engine-destructure-forms", 'async () => { const tries = { param: () => (({ a }) => a)(undefined), nested: () => { const { a: { b } } = {}; return b; }, arrayOfUndefined: () => { const [x] = undefined; return x; }, nestedArray: () => { const { list: [first] } = {}; return first; }, defaulted: () => (({ a } = {}) => a)(null), restOfNull: () => { const { ...rest } = null; return rest; }, forOf: () => { for (const { id } of [undefined]) return id; } }; const out = {}; for (const [k, f] of Object.entries(tries)) { try { out[k] = ["no error", f()]; } catch (e) { out[k] = [e.name, e.message]; } } return out; }');
add("engine", "engine-property-of-undefined-in-callback", 'async () => { const rows = [{ user: { name: "a" } }, {}]; return rows.map((r) => r.user.name); }');
add("engine", "engine-property-of-undefined-after-await", 'async () => { const issue = await tracker.get_issue({ id: 3 }); return issue.assignee.login; }');
add("engine", "engine-set-property-of-undefined-nested", "async () => { const state = {}; state.totals.count = 1; return state; }");
add("engine", "engine-user-lookalike-messages", 'async () => { const texts = ["cannot read property \'x\' of undefined", "x is not initialized", "invalid array length", "empty array", "toPrimitive", "this is not initialized", "invalid number of digits", "cannot convert symbol to string"]; const makers = [TypeError, ReferenceError, RangeError, Error, URIError]; const out = []; for (const text of texts) for (const make of makers) { try { throw new make(text); } catch (e) { if (e.message !== text) out.push([make.name, text, e.message]); } } class Mine extends TypeError {} const mine = new Mine("toPrimitive"); const called = TypeError("empty array"); return { changed: out, subclass: mine.message, called: called.message, uri: new URIError("mine").message }; }');
add("engine", "engine-user-lookalike-uncaught", 'async () => { throw new TypeError("cannot read property \'x\' of undefined"); }');
add("engine", "engine-tdz-forms", 'async () => { const tries = { letRead: () => { const f = () => later; f(); let later = 1; }, classRead: () => { const f = () => new Later(); f(); class Later {} }, constSelf: () => { const a = a + 1; }, typeofTdz: () => { const f = () => typeof inner; f(); let inner; }, defaultParam: () => ((a = b, b = 1) => a)() }; const out = {}; for (const [k, f] of Object.entries(tries)) { try { out[k] = ["no error", f()]; } catch (e) { out[k] = [e.name, e.message]; } } return out; }');
add("engine", "engine-super-forms", 'async () => { class A {} const tries = { thisFirst: () => { class B extends A { constructor() { this.x = 1; super(); } } return new B(); }, noSuper: () => { class B extends A { constructor() {} } return new B(); }, returnBefore: () => { class B extends A { constructor() { return; } } return new B(); } }; const out = {}; for (const [k, f] of Object.entries(tries)) { try { out[k] = ["no error", typeof f()]; } catch (e) { out[k] = [e.name, e.message]; } } return out; }');
add("engine", "engine-length-forms", 'async () => { const tries = { negative: () => new Array(-1), fraction: () => new Array(1.5), tooLarge: () => new Array(2 ** 32), setLength: () => { const a = []; a.length = -1; }, setLengthFraction: () => { const a = []; a.length = 0.5; }, repeatHuge: () => "abc".repeat(2 ** 30), padHuge: () => "a".padEnd(2 ** 30) }; const out = {}; for (const [k, f] of Object.entries(tries)) { try { out[k] = ["no error", typeof f()]; } catch (e) { out[k] = [e.name, e.message]; } } return out; }');
add("engine", "engine-typed-array-length", 'async () => { const tries = { typed: () => new Uint8Array(-1), buffer: () => new ArrayBuffer(-1) }; const out = {}; for (const [k, f] of Object.entries(tries)) { try { out[k] = ["no error", typeof f()]; } catch (e) { out[k] = [e.name, e.message]; } } return out; }');
add("engine", "engine-conversion-forms", 'async () => { const sym = Symbol("s"); const tries = { concat: () => "" + sym, template: () => `${sym}`, plus: () => sym + 1, numberOf: () => Number(sym), join: () => [sym].join(","), noPrimitive: () => `${{ toString: () => ({}), valueOf: () => ({}) }}`, nullProto: () => `${Object.create(null)}`, plusNoPrimitive: () => +{ [Symbol.toPrimitive]: () => ({}) }, stringOf: () => String(sym) }; const out = {}; for (const [k, f] of Object.entries(tries)) { try { out[k] = ["no error", f()]; } catch (e) { out[k] = [e.name, e.message]; } } return out; }');
add("engine", "engine-digits-forms", 'async () => { const tries = { toFixed: () => (1).toFixed(101), toFixedNegative: () => (1).toFixed(-1), toPrecision: () => (1).toPrecision(101), toPrecisionZero: () => (1).toPrecision(0), toExponential: () => (1).toExponential(101), radix: () => (255).toString(1), radixHigh: () => (255).toString(37), bigintRadix: () => 255n.toString(1), parseRadix: () => parseInt("11", 99) }; const out = {}; for (const [k, f] of Object.entries(tries)) { try { out[k] = ["no error", f()]; } catch (e) { out[k] = [e.name, e.message]; } } return out; }');
add("engine", "engine-reduce-forms", 'async () => { const tries = { reduce: () => [].reduce((a, b) => a + b), reduceRight: () => [].reduceRight((a, b) => a + b), holes: () => [, ,].reduce((a, b) => a + b), typed: () => new Uint8Array(0).reduce((a, b) => a + b), filtered: () => [1, 2].filter((x) => x > 5).reduce((a, b) => a + b), withInitial: () => [].reduce((a, b) => a + b, 0) }; const out = {}; for (const [k, f] of Object.entries(tries)) { try { out[k] = ["no error", f()]; } catch (e) { out[k] = [e.name, e.message]; } } return out; }');
add("engine", "engine-uri-forms", 'async () => { const tries = { percent: () => decodeURIComponent("%"), badHex: () => decodeURIComponent("%zz"), truncated: () => decodeURIComponent("%E0%A4%A"), invalidUtf8: () => decodeURIComponent("%ff"), decodeUri: () => decodeURI("%"), loneSurrogate: () => encodeURIComponent("\\ud800"), encodeUri: () => encodeURI("\\udfff") }; const out = {}; for (const [k, f] of Object.entries(tries)) { try { out[k] = ["no error", f()]; } catch (e) { out[k] = [e.name, e.message]; } } return out; }');
add("engine", "engine-error-constructors", 'async () => ({ names: [Error.name, TypeError.name, RangeError.name], lengths: [Error.length, TypeError.length, AggregateError.length], strings: [String(Error), String(TypeError)], types: [typeof Error, typeof TypeError], sameCtor: new TypeError("t").constructor === TypeError, protoCtor: TypeError.prototype.constructor === TypeError, chain: Object.getPrototypeOf(TypeError) === Error, instance: [new TypeError("t") instanceof TypeError, new TypeError("t") instanceof Error, TypeError("called") instanceof TypeError], subclass: (() => { class E extends RangeError {} const e = new E("m"); return [e instanceof E, e instanceof RangeError, e.name, e.message, e.constructor === E]; })(), cause: new Error("m", { cause: 1 }).cause, aggregate: new AggregateError([new Error("a")], "many").errors.length, isError: typeof Error.isError === "function" ? Error.isError(new TypeError("t")) : "no Error.isError", keys: Object.getOwnPropertyNames(Error).sort() })');
add("engine", "engine-structured-messages", 'async () => { const out = {}; const tries = { undefinedProperty: () => undefined.x, nullProperty: () => null.x, notDefined: () => missing, jsonParse: () => JSON.parse("{bad"), tdz: () => { const f = () => later; f(); let later; }, uri: () => decodeURIComponent("%"), digits: () => (1).toFixed(200), reduce: () => [].reduce((a, b) => a + b), length: () => new Array(-1) }; for (const [k, f] of Object.entries(tries)) { try { f(); } catch (e) { out[k] = { name: e.name, message: e.message, string: String(e), ctor: e.constructor.name }; } } return out; }');
add("engine", "engine-structured-messages-quoting-source", 'async () => { const out = {}; const tries = { notFunction: () => ({}).go(), notIterable: () => { for (const x of 5) {} }, constAssign: () => { const a = 1; a = 2; }, bigintMix: () => 1n + 1 }; for (const [k, f] of Object.entries(tries)) { try { f(); } catch (e) { out[k] = { name: e.name, message: e.message, string: String(e), ctor: e.constructor.name }; } } return out; }');
add("engine", "engine-caught-type-error", 'async () => { try { null.x; } catch (e) { return { name: e.name, isTypeError: e instanceof TypeError, isError: e instanceof Error, stackType: typeof e.stack, keys: Object.keys(e) }; } }');
add("engine", "engine-caught-rethrown", 'async () => { try { undefined.x; } catch (e) { throw new Error("wrapped: " + e.message); } }');
add("engine", "engine-error-tostring", 'async () => [String(new Error("m")), String(new TypeError("t")), String(new Error()), new Error("m").toString(), Object.prototype.toString.call(new Error("m")), `${new RangeError("r")}`]');
add("engine", "engine-error-properties", 'async () => { const e = new Error("m", { cause: "c" }); return { own: Object.getOwnPropertyNames(e).sort(), cause: e.cause, json: JSON.stringify(e), name: e.name }; }');

// ---------------------------------------------------------------- JSON.parse on non-JSON
add("json", "json-parse-word", 'async () => JSON.parse("not json")');
add("json", "json-parse-single-quotes", "async () => JSON.parse(\"{'a': 1}\")");
add("json", "json-parse-empty", 'async () => JSON.parse("")');
add("json", "json-parse-undefined", "async () => JSON.parse(undefined)");
add("json", "json-parse-trailing-comma", "async () => JSON.parse('{\"a\": 1,}')");
add("json", "json-parse-truncated", "async () => JSON.parse('{\"a\": [1, 2')");
add("json", "json-parse-html", 'async () => JSON.parse("<!DOCTYPE html><html></html>")');
add("json", "json-parse-text-result", 'async () => { const r = await mock.lookup({ query: "tides" }); return JSON.parse(r); }');
add("json", "json-parse-object", "async () => JSON.parse({ a: 1 })");
add("json", "json-parse-already-parsed", 'async () => { const r = await tracker.get_issue({ id: 3 }); return JSON.parse(r); }');
add("json", "json-parse-nan", 'async () => JSON.parse("NaN")');
add("json", "json-parse-unquoted-key", 'async () => JSON.parse("{a: 1}")');
add("json", "json-parse-bad-escape", 'async () => JSON.parse(\'"\\\\x41"\')');
add("json", "json-parse-control-character", 'async () => JSON.parse(\'"line\\nbreak"\')');
add("json", "json-parse-long-text", 'async () => JSON.parse("Sorry, I could not find any results for your query. Please try again later.")');
add("json", "json-parse-caught", 'async () => { try { JSON.parse("{bad"); } catch (e) { return { name: e.name, isSyntaxError: e instanceof SyntaxError, hasMessage: typeof e.message === "string" && e.message.length > 0 }; } }');
add("json", "json-parse-valid", 'async () => JSON.parse(\' {"a": [1, 2, {"b": null}], "c": "\\\\u00e9"} \')');
add("json", "json-stringify-forms", 'async () => [JSON.stringify({ a: [1, { b: 2 }] }, null, 2), JSON.stringify("é\\u2028"), JSON.stringify(undefined), JSON.stringify(() => 1), JSON.stringify(new Date(0)), JSON.stringify({ a: undefined }), JSON.stringify([undefined]), JSON.stringify(NaN), JSON.stringify("\\ud83d")]');

// ---------------------------------------------------------------- syntax errors
add("syntax", "syntax-unexpected-token", "async () => { return 1 +; }");
add("syntax", "syntax-missing-paren", 'async () => { const value = await mock.add({ a: 1, b: 2 }\n  return value;\n}');
add("syntax", "syntax-missing-brace", "async () => { if (true) { return 1; }");
add("syntax", "syntax-extra-brace", "async () => { return 1; } }");
add("syntax", "syntax-unterminated-string", 'async () => { return "abc; }');
add("syntax", "syntax-unterminated-template", "async () => { return `abc; }");
add("syntax", "syntax-reserved-word", "async () => { const class = 1; return class; }");
add("syntax", "syntax-duplicate-const", "async () => { const a = 1; const a = 2; return a; }");
add("syntax", "syntax-invalid-regexp", 'async () => /(/.test("x")');
add("syntax", "syntax-garbage", "this is not valid javascript @#$");
add("syntax", "syntax-unexpected-identifier", "async () => { const a = 1 const b = 2; return a + b; }");
add("syntax", "syntax-missing-initializer", "async () => { const a; return a; }");
add("syntax", "syntax-invalid-assignment", "async () => { 1 = 2; }");
add("syntax", "syntax-await-in-sync-function", "async () => { function inner() { return await mock.add({ a: 1, b: 2 }); } return inner(); }");
add("syntax", "syntax-await-in-sync-callback", "async () => [1, 2].map((x) => await mock.add({ a: x, b: 1 }))");
add("syntax", "syntax-import-statement", 'import fs from "fs";\nasync () => fs.readFileSync("x")');
add("syntax", "syntax-import-in-arrow", 'async () => { import fs from "fs"; return 1; }');
add("syntax", "syntax-export", "export const x = 1;");
add("syntax", "syntax-return-outside", "return 42;;) ");
add("syntax", "syntax-python", 'def main():\n    return mock.add(a=1, b=2)\n');
add("syntax", "syntax-json-object", '{ "tool": "add", "a": 1 }');
add("syntax", "syntax-html", "<div>hello</div>");
add("syntax", "syntax-octal-literal", "async () => 010 + 1");
add("syntax", "syntax-optional-chain-assign", "async () => { const o = {}; o?.a = 1; }");
add("syntax", "syntax-multiline-position", "async () => {\n  const a = 1;\n  const b = 2;\n  return a +* b;\n}");
add("syntax", "syntax-one-server", "async () => { return 1 +; }", { servers: ["mock"] });
add("syntax", "syntax-second-line-one-server", "async () => {\n  return 1 +;\n}", { servers: ["tracker"] });
add("syntax", "syntax-with-statement", "async () => { with (Math) { return PI; } }");
add("syntax", "syntax-private-outside-class", "async () => { const o = {}; return o.#secret; }");
add("syntax", "syntax-line-terminator", 'async () => { return "a\nb"; }');

// ---------------------------------------------------------------- TypeScript syntax
add("typescript", "ts-annotation-const", "async () => { const a: number = 1; return a; }");
add("typescript", "ts-annotation-parameter", "async () => { const double = (x: number) => x * 2; return double(2); }");
add("typescript", "ts-annotation-return", "async (): Promise<number> => { return 1; }");
add("typescript", "ts-interface", "interface Row { id: number }\nasync () => { const r: Row = { id: 1 }; return r; }");
add("typescript", "ts-type-alias", 'async () => { type Id = string; const id: Id = "a"; return id; }');
add("typescript", "ts-generic-call", "async () => { const m = new Map<string, number>(); m.set(\"a\", 1); return m.size; }");
add("typescript", "ts-as-cast", "async () => { const x = (5 as number); return x; }");
add("typescript", "ts-as-any-result", 'async () => { const r = (await mock.add({ a: 1, b: 2 })) as any; return r; }');
add("typescript", "ts-non-null", "async () => { const o = { a: { b: 1 } }; return o.a!.b; }");
add("typescript", "ts-enum", "async () => { enum Color { Red, Green } return Color.Green; }");
add("typescript", "ts-satisfies", 'async () => { const o = { a: 1 } satisfies Record<string, number>; return o; }');
add("typescript", "ts-angle-cast", "async () => { const x = <number>5; return x; }");
add("typescript", "ts-optional-parameter", "async () => { const f = (a?: number) => a ?? 0; return f(); }");
add("typescript", "ts-fence", "```typescript\nasync () => { const a: number = 1; return a; }\n```");
add("typescript", "ts-declare", "declare const mock: any;\nasync () => 1");
add("typescript", "ts-typed-catch", "async () => { try { throw new Error(\"x\"); } catch (e: any) { return e.message; } }");
add("typescript", "ts-typed-destructure", "async () => { const { a }: { a: number } = { a: 1 }; return a; }");

// ---------------------------------------------------------------- code shapes normalizeCode handles
add("shapes", "shape-fence-js", "```js\nasync () => { return 1; }\n```");
add("shapes", "shape-fence-javascript", "```javascript\nconst x = await mock.add({ a: 1, b: 2 });\nx + 1\n```");
add("shapes", "shape-fence-plain", "```\nasync () => { return \"plain fence\"; }\n```");
add("shapes", "shape-fence-ts", "```ts\nasync () => 5\n```");
add("shapes", "shape-fence-unclosed", "```js\nasync () => { return 1; }");
add("shapes", "shape-fence-with-prose", "Here is the code:\n```js\nasync () => 1\n```");
add("shapes", "shape-fence-trailing-prose", "```js\nasync () => 1\n```\nThis returns one.");
add("shapes", "shape-fence-inside-code", "async () => { return \"```js\\ncode\\n```\"; }");
add("shapes", "shape-statements", 'const x = await mock.add({ a: 1, b: 2 });\nx * 2');
add("shapes", "shape-statements-no-expression", "const x = 1;\nconst y = 2;");
add("shapes", "shape-statements-with-return", 'const r = await mock.lookup({ query: "tides" });\nreturn r.length;');
add("shapes", "shape-expression", "1 + 2");
add("shapes", "shape-expression-await", "await mock.add({ a: 2, b: 2 })");
add("shapes", "shape-expression-object", '({ a: 1, b: "two" })');
add("shapes", "shape-object-literal-statement", "{ a: 1 }");
add("shapes", "shape-named-async-function", "async function main() { return \"from main\"; }");
add("shapes", "shape-named-function-with-call", "async function main() { return await mock.add({ a: 3, b: 4 }); }\nmain()");
add("shapes", "shape-named-function-awaited-call", "async function run() { return 7; }\nawait run();");
add("shapes", "shape-helper-then-arrow", "function helper(n) { return n * 2; }\nasync () => helper(21)");
add("shapes", "shape-iife", "(async () => { return 42; })()");
add("shapes", "shape-iife-with-semicolon", "(async () => { return 43; })();");
add("shapes", "shape-export-default-arrow", "export default async () => { return 1; }");
add("shapes", "shape-export-default-function", "export default async function () { return 2; }");
add("shapes", "shape-module-exports", "module.exports = async () => 3;");
add("shapes", "shape-arrow-with-parameter", "async (input) => { return typeof input; }");
add("shapes", "shape-arrow-with-default-parameter", "async (n = 5) => n * 2");
add("shapes", "shape-arrow-destructured-parameter", "async ({ a, b } = { a: 1, b: 2 }) => a + b");
add("shapes", "shape-non-async-arrow", "() => 42");
add("shapes", "shape-non-async-arrow-returning-promise", "() => mock.add({ a: 5, b: 5 })");
add("shapes", "shape-arrow-expression-body", "async () => mock.add({ a: 1, b: 2 })");
add("shapes", "shape-arrow-trailing-semicolon", "async () => { return 1; };");
add("shapes", "shape-arrow-in-parens", "(async () => { return 9; })");
add("shapes", "shape-leading-comment", "// compute the answer\nasync () => 5");
add("shapes", "shape-block-comment", "/* compute */ async () => 6 /* done */");
add("shapes", "shape-whitespace", "  \n\n  async () => 7  \n\n ");
add("shapes", "shape-function-expression", "async function () { return 8; }");
add("shapes", "shape-two-arrows", "async () => 1;\nasync () => 2");
add("shapes", "shape-arrow-assigned", "const main = async () => 10;\nmain()");
add("shapes", "shape-arrow-assigned-no-call", "const main = async () => 11;");
add("shapes", "shape-empty", "");
add("shapes", "shape-only-whitespace", "   \n\t ");
add("shapes", "shape-only-comment", "// nothing to do");
add("shapes", "shape-string-literal", '"just a string"');
add("shapes", "shape-number-literal", "5");
add("shapes", "shape-template-literal", "`template ${1 + 1}`");
add("shapes", "shape-try-catch-statements", 'try {\n  await mock.fail({ reason: "x" });\n} catch (e) {\n  console.log("caught", e.message);\n}\n"finished"');
add("shapes", "shape-if-statement-last", "const n = 3;\nif (n > 2) { console.log(\"big\"); }");
add("shapes", "shape-for-loop-last", "let total = 0;\nfor (let i = 0; i < 4; i++) total += i;\ntotal");
add("shapes", "shape-class-then-use", "class Counter { n = 0; inc() { return ++this.n; } }\nconst c = new Counter();\nc.inc();\nc.inc()");
add("shapes", "shape-generator-arrow", "async function* gen() { yield 1; }");
add("shapes", "shape-windows-newlines", "async () => {\r\n  const a = 1;\r\n  return a + 1;\r\n}");
add("shapes", "shape-unicode-identifiers", 'async () => { const café = "é"; const 数 = 2; return café + 数; }');
add("shapes", "shape-html-comment", "async () => { return 1; } <!-- trailing");
add("shapes", "shape-shebang", "#!/usr/bin/env node\nasync () => 1");
add("shapes", "shape-label", "outer: for (;;) { break outer; }\n\"after the label\"");
add("shapes", "shape-uses-this", "async () => typeof this");
add("shapes", "shape-uses-arguments", 'async () => { try { return typeof arguments; } catch (e) { return e.name; } }');
add("shapes", "shape-strict-mode", 'async () => { try { undeclaredVariable = 1; return "sloppy"; } catch (e) { return "strict"; } }');
add("shapes", "shape-top-level-await-expression", "(await mock.add({ a: 1, b: 1 })) + (await mock.add({ a: 2, b: 2 }))");

// ---------------------------------------------------------------- tool calls
add("tools", "tool-number-result", "async () => {\n  const s = await mock.add({ a: 4, b: 5 });\n  console.log(\"sum\", s);\n  return s;\n}");
add("tools", "tool-text-result", 'async () => mock.lookup({ query: "tide tables" })');
add("tools", "tool-mixed-content-result", "async () => mock.chart({})");
add("tools", "tool-structured-result", 'async () => tracker.list_issues({ state: "open", limit: 3 })');
add("tools", "tool-json-text-result", "async () => tracker.get_issue({ id: 3 })");
add("tools", "tool-json-array-result", 'async () => tracker.search({ query: "export" })');
add("tools", "tool-json-empty-array-result", 'async () => tracker.search({ query: "zzz" })');
add("tools", "tool-plain-text-result", "async () => tracker.delete_({ id: 3 })");
add("tools", "tool-structured-created", 'async () => tracker.create_issue({ title: "Add CSV import", labels: ["feature"] })');
add("tools", "tool-result-types", 'async () => { const sum = await mock.add({ a: 1, b: 2 }); const text = await mock.lookup({ query: "x" }); const list = await tracker.search({ query: "export" }); const issue = await tracker.get_issue({ id: 1 }); const chart = await mock.chart({}); return { sum: typeof sum, text: typeof text, list: Array.isArray(list), issue: Object.keys(issue), chart: Object.keys(chart), chartParts: chart.content.map((c) => c.type) }; }');
add("tools", "tool-text-parsed-as-json", 'async () => { const out = {}; for (const text of ["12", "1e5", "-0", " 5 ", "true", "null", "\\"quoted\\"", "[1, 2]", "{\\"a\\": 1}", "01", "1.", "NaN", "0x10", "{\\"a\\":1}{", "12345678901234567890", "1e999", "{\\"a\\":1,\\"a\\":2}", "[1,]", "\'single\'", "undefined", ""]) { const r = await mock.repeat({ text, times: 1 }); out[text] = [typeof r, r]; } return out; }');
add("tools", "tool-text-json-unicode", 'async () => mock.repeat({ text: "{\\"k\\": \\"é \\\\u00e9 \\\\ud83d\\\\ude00\\"}", times: 1 })');
add("tools", "tool-empty-text-result", 'async () => { const r = await mock.repeat({ text: "abc", times: 0 }); return [typeof r, r]; }');
add("tools", "tool-error-caught", 'async () => {\n  try {\n    await mock.fail({ reason: "checking" });\n  } catch (e) {\n    return { name: e.name, message: e.message, isError: e instanceof Error, string: String(e), keys: Object.keys(e) };\n  }\n}');
add("tools", "tool-error-uncaught", 'async () => {\n  const value = await mock.fail({ reason: "checking" });\n  return value;\n}');
add("tools", "tool-error-uncaught-after-logs", 'async () => { console.log("looking up"); await tracker.get_issue({ id: 999 }); console.log("unreached"); }');
add("tools", "tool-error-default-reason", "async () => mock.fail({})");
add("tools", "tool-error-then-continue", 'async () => { let note = null; try { await mock.fail({ reason: "first" }); } catch (e) { note = e.message; } const value = await mock.add({ a: 1, b: 1 }); return { value, note }; }');
add("tools", "tool-error-in-promise-all", 'async () => Promise.all([mock.add({ a: 1, b: 1 }), mock.fail({ reason: "one of several" })])');
add("tools", "tool-error-allsettled", 'async () => { const r = await Promise.allSettled([mock.add({ a: 1, b: 1 }), mock.fail({ reason: "settled" })]); return r.map((x) => ({ status: x.status, value: x.value, reason: x.reason && x.reason.message })); }');
add("tools", "tool-error-catch-method", 'async () => mock.fail({ reason: "handled inline" }).catch((e) => "fallback: " + e.message)');
add("tools", "tool-error-finally", 'async () => { try { return await mock.fail({ reason: "with finally" }); } finally { console.log("cleanup"); } }');
add("tools", "tool-missing-required-argument", "async () => mock.add({ a: 1 })");
add("tools", "tool-wrong-argument-type", 'async () => mock.add({ a: "four", b: [5] })');
add("tools", "tool-extra-argument", "async () => mock.add({ a: 1, b: 2, c: 3 })");
add("tools", "tool-no-arguments", "async () => mock.chart()");
add("tools", "tool-no-arguments-required", "async () => mock.add()");
add("tools", "tool-positional-string", 'async () => mock.lookup("tides")');
add("tools", "tool-positional-numbers", "async () => mock.add(4, 5)");
add("tools", "tool-array-argument", "async () => mock.add([4, 5])");
add("tools", "tool-null-argument", "async () => mock.add(null)");
add("tools", "tool-second-argument", "async () => mock.add({ a: 1, b: 2 }, { extra: true })");
add("tools", "tool-undefined-in-arguments", 'async () => mock.lookup({ query: "x", extra: undefined })');
add("tools", "tool-date-in-arguments", "async () => mock.lookup({ query: new Date(0) })");
add("tools", "tool-nested-arguments", 'async () => tracker.create_issue({ title: "Nested", body: "b", labels: ["a", "b"], extra: { deep: [1, { x: null }] } })');
add("tools", "tool-nan-in-arguments", "async () => mock.add({ a: NaN, b: Infinity })");
add("tools", "tool-function-in-arguments", 'async () => mock.lookup({ query: "x", callback: () => 1 })');
add("tools", "tool-bigint-in-arguments", "async () => mock.add({ a: 1n, b: 2 })");
add("tools", "tool-cycle-in-arguments", 'async () => { const a = { query: "x" }; a.self = a; return mock.lookup(a); }');
add("tools", "tool-symbol-argument", 'async () => mock.lookup(Symbol("s"))');
add("tools", "tool-uint8array-in-arguments", "async () => mock.lookup({ query: \"bytes\", data: new Uint8Array([1, 2, 3]) })");
add("tools", "tool-map-in-arguments", 'async () => mock.lookup({ query: "map", data: new Map([["a", 1]]) })');
add("tools", "tool-unicode-arguments", 'async () => mock.lookup({ query: "é ☃ \\u{1F600} \\"quoted\\" \\\\ back" })');
add("tools", "tool-unknown-method", "async () => mock.nope({})");
add("tools", "tool-unknown-method-caught", 'async () => { try { await mock.nope({}); } catch (e) { return e.message; } }');
add("tools", "tool-unsanitized-name", 'async () => tracker["list-issues"]({})');
add("tools", "tool-reserved-word-name", "async () => tracker.delete({ id: 3 })");
add("tools", "tool-original-case", "async () => mock.Add({ a: 1, b: 2 })");
add("tools", "tool-unknown-namespace", "async () => github.list_issues({})");
add("tools", "tool-unknown-namespace-typeof", 'async () => [typeof github, typeof mock, typeof tracker, typeof codemode]');
add("tools", "tool-codemode-namespace", "async () => codemode.add({ a: 1, b: 2 })");
add("tools", "tool-method-as-property", 'async () => [typeof mock.add, typeof mock.nope, typeof mock.then, typeof mock[Symbol.iterator], Object.keys(mock), "add" in mock, String(mock.add).slice(0, 5)]');
add("tools", "tool-namespace-reassign", 'async () => { try { mock = null; return "reassigned"; } catch (e) { return e.name; } }');
add("tools", "tool-method-detached", "async () => { const { add } = mock; return add({ a: 2, b: 3 }); }");
add("tools", "tool-method-override", 'async () => { mock.add = async () => "overridden"; return mock.add({ a: 1, b: 2 }); }');
add("tools", "tool-unawaited", 'async () => { mock.slow({ seconds: 0.3 }); return "returned first"; }');
add("tools", "tool-unawaited-then", 'async () => { mock.add({ a: 1, b: 2 }).then((v) => console.log("late", v)); return "no wait"; }');
add("tools", "tool-unawaited-failing", 'async () => { mock.fail({ reason: "nobody listens" }); return "ignored"; }');
add("tools", "tool-unawaited-returned-promise", 'async () => { const p = mock.add({ a: 1, b: 2 }); return { p }; }');
add("tools", "tool-parallel", 'async () => {\n  const [a, b, c] = await Promise.all([\n    mock.add({ a: 6, b: 7 }),\n    mock.lookup({ query: "both at once" }),\n    tracker.get_issue({ id: 2 }),\n  ]);\n  return { a, b, title: c.title };\n}');
add("tools", "tool-parallel-many", "async () => (await Promise.all(Array.from({ length: 24 }, (_, i) => mock.add({ a: i, b: 1 })))).join(\",\")");
add("tools", "tool-parallel-order", 'async () => { const order = []; await Promise.all([mock.slow({ seconds: 0.25 }).then(() => order.push("slow")), mock.add({ a: 1, b: 1 }).then(() => order.push("fast"))]); return order; }');
add("tools", "tool-race", "async () => Promise.race([mock.slow({ seconds: 0.4 }), mock.add({ a: 1, b: 1 })])");
add("tools", "tool-sequential-loop", 'async () => {\n  const results = [];\n  for (const q of ["tides", "tides history"]) {\n    results.push(await mock.lookup({ query: q }));\n  }\n  console.log("looked up", results.length);\n  return results;\n}');
add("tools", "tool-chained", 'async () => { const { issues } = await tracker.list_issues({ label: "docs", state: "all", limit: 2 }); const details = []; for (const i of issues) details.push(await tracker.get_issue({ id: i.id })); return details.map((d) => `${d.id}: ${d.title} (${d.comments.length} comments)`); }');
add("tools", "tool-filter-large-result", 'async () => { const { issues, total } = await tracker.list_issues({ state: "all", limit: 500 }); const bugs = issues.filter((i) => i.labels.includes("bug")); return { total, bugs: bugs.length, firstOpenBug: bugs.find((i) => i.state === "open").title }; }');
add("tools", "tool-two-servers", 'async () => { const n = await mock.add({ a: 1, b: 2 }); const issue = await tracker.get_issue({ id: n }); return issue.title; }');
add("tools", "tool-result-mutation", "async () => { const a = await tracker.get_issue({ id: 1 }); a.title = \"changed\"; const b = await tracker.get_issue({ id: 1 }); return [a.title, b.title]; }");
add("tools", "tool-retry-loop", 'async () => { let attempts = 0; for (;;) { attempts++; try { await mock.fail({ reason: "try " + attempts }); } catch (e) { console.log(e.message); if (attempts === 3) return { attempts }; } } }');
add("tools", "tool-await-in-map", "async () => Promise.all([1, 2, 3].map(async (n) => (await mock.add({ a: n, b: n })) * 10))");
add("tools", "tool-large-result-kept-inside", 'async () => { const text = await mock.repeat({ text: "0123456789", times: 20000 }); return { length: text.length, head: text.slice(0, 12) }; }');
add("tools", "tool-single-server", "async () => mock.add({ a: 2, b: 2 })", { servers: ["mock"] });
add("tools", "tool-single-server-other-missing", "async () => tracker.get_issue({ id: 1 })", { servers: ["mock"] });
add("tools", "tool-codemode-namespaces-option", 'async () => { const n = await codemode.add({ a: 1, b: 2 }); const issue = await codemode.get_issue({ id: n }); const hits = await codemode.search({ query: "export" }); return { n, title: issue.title, hits: hits.length, deleted: await codemode.delete_({ id: 4 }) }; }', { formats: ["code_tool"], namespaces: "codemode" });
add("tools", "tool-codemode-namespaces-server-global", "async () => mock.add({ a: 1, b: 2 })", { formats: ["code_tool"], namespaces: "codemode" });

// ---------------------------------------------------------------- timers
add("timers", "timer-settimeout", 'async () => { await new Promise((r) => setTimeout(r, 30)); return "waited"; }');
add("timers", "timer-settimeout-arguments", 'async () => new Promise((r) => setTimeout(r, 10, "passed through"))');
add("timers", "timer-order", 'async () => { const order = []; setTimeout(() => order.push("t30"), 30); setTimeout(() => order.push("t10"), 10); setTimeout(() => order.push("t0"), 0); queueMicrotask(() => order.push("micro")); Promise.resolve().then(() => order.push("promise")); order.push("sync"); await new Promise((r) => setTimeout(r, 60)); return order; }');
add("timers", "timer-setinterval", "async () => { let n = 0; await new Promise((r) => { const id = setInterval(() => { n++; if (n === 3) { clearInterval(id); r(); } }, 10); }); return n; }");
add("timers", "timer-cleartimeout", 'async () => { let fired = false; const id = setTimeout(() => { fired = true; }, 10); clearTimeout(id); await new Promise((r) => setTimeout(r, 40)); return fired; }');
add("timers", "timer-id-type", "async () => { const id = setTimeout(() => {}, 0); const iv = setInterval(() => {}, 1000); clearInterval(iv); return [typeof id, typeof iv]; }");
add("timers", "timer-ids", "async () => { const a = setTimeout(() => {}, 0); const b = setTimeout(() => {}, 0); const c = setInterval(() => {}, 1000); clearInterval(c); const d = setImmediate(() => {}); return [a, b, c, typeof d, typeof clearTimeout(a), typeof clearTimeout(undefined), typeof clearTimeout(\"x\")]; }");
add("timers", "timer-interval-string-callback", 'async () => { try { setInterval("1", 10); return "accepted"; } catch (e) { return e.name + ": " + e.message; } }');
add("timers", "timer-no-arguments", 'async () => { try { setTimeout(); return "accepted"; } catch (e) { return e.name + ": " + e.message; } }');
add("timers", "timer-no-delay", 'async () => new Promise((r) => setTimeout(() => r("no delay argument")))');
add("timers", "timer-string-delay", 'async () => new Promise((r) => setTimeout(() => r("string delay"), "20"))');
add("timers", "timer-negative-delay", 'async () => new Promise((r) => setTimeout(() => r("negative delay"), -5))');
add("timers", "timer-nested", 'async () => new Promise((r) => setTimeout(() => setTimeout(() => r("nested"), 5), 5))');
add("timers", "timer-sleep-helper", 'async () => { const sleep = (ms) => new Promise((r) => setTimeout(r, ms)); const marks = []; for (let i = 0; i < 3; i++) { await sleep(5); marks.push(i); } return marks; }');
add("timers", "timer-clock-advances", "async () => { const a = Date.now(); await new Promise((r) => setTimeout(r, 50)); const waited = Date.now() - a; return waited >= 40 && waited < 2000; }");
add("timers", "timer-clock-after-tool-call", "async () => { const a = Date.now(); await mock.slow({ seconds: 0.2 }); const waited = Date.now() - a; return waited >= 150 && waited < 5000; }");
add("timers", "timer-unfinished", 'async () => { setTimeout(() => console.log("never seen"), 200); return "returned before the timer"; }');
add("timers", "timer-setimmediate", 'async () => typeof setImmediate === "function" ? new Promise((r) => setImmediate(() => r("immediate"))) : "no setImmediate"');
add("timers", "timer-callback-string", 'async () => { try { setTimeout("globalThis.fromString = 1", 0); await new Promise((r) => setTimeout(r, 20)); return typeof globalThis.fromString; } catch (e) { return e.name + ": " + e.message; } }');
add("timers", "timer-abort-signal", 'async () => typeof AbortSignal === "function" && typeof AbortSignal.timeout === "function" ? new Promise((r) => { const s = AbortSignal.timeout(10); s.addEventListener("abort", () => r(s.reason.name)); }) : "no AbortSignal"');

// ---------------------------------------------------------------- timeouts
add("timeouts", "timeout-slow-call", "async () => mock.slow({ seconds: 3 })", { timeout_ms: 1000 });
add("timeouts", "timeout-timer", 'async () => { await new Promise((r) => setTimeout(r, 5000)); return "late"; }', { timeout_ms: 500 });
add("timeouts", "timeout-never-settles", "async () => new Promise(() => {})", { timeout_ms: 500 });
add("timeouts", "timeout-after-logs", 'async () => { console.log("started"); await mock.slow({ seconds: 3 }); console.log("unreached"); }', { timeout_ms: 800 });
add("timeouts", "timeout-caught-inside", 'async () => { try { await mock.slow({ seconds: 3 }); } catch (e) { return "caught: " + e.message; } }', { timeout_ms: 800 });
add("timeouts", "timeout-just-in-time", 'async () => { await mock.slow({ seconds: 0.2 }); return "in time"; }', { timeout_ms: 2000 });
add("timeouts", "timeout-own-race", 'async () => Promise.race([mock.slow({ seconds: 2 }), new Promise((_, reject) => setTimeout(() => reject(new Error("my own deadline")), 200))])', { timeout_ms: 5000 });
add("timeouts", "timeout-infinite-loop", "async () => {\n  let n = 0;\n  while (true) {\n    n++;\n  }\n}", { timeout_ms: 1000, hang_ms: 8000, note: "Code that never yields: the executor's setTimeout race cannot fire." });
add("timeouts", "timeout-infinite-loop-after-await", "async () => { await mock.add({ a: 1, b: 1 }); for (;;) {} }", { timeout_ms: 1000, hang_ms: 8000 });

// ---------------------------------------------------------------- results and logs over the budget
add("budget", "budget-long-string", 'async () => "0123456789".repeat(10000)');
add("budget", "budget-long-array", "async () => Array.from({ length: 6000 }, (_, i) => i * 1000)");
add("budget", "budget-large-object", 'async () => tracker.list_issues({ state: "all", limit: 500 })');
add("budget", "budget-large-nested", 'async () => ({ summary: "kept", rows: Array.from({ length: 400 }, (_, i) => ({ id: i, title: "Row " + i + " " + "x".repeat(i % 50), tags: ["a", "b"] })), note: "y".repeat(3000) })');
add("budget", "budget-many-log-lines", 'async () => { for (let i = 0; i < 2000; i++) console.log("log line number", i, "with some padding text"); return "done"; }');
add("budget", "budget-one-long-log-line", 'async () => { console.log("z".repeat(100000)); return "done"; }');
add("budget", "budget-large-result-and-logs", 'async () => { for (let i = 0; i < 1500; i++) console.log("progress", i, "of 1500 steps complete"); return Array.from({ length: 3000 }, (_, i) => "item-" + i); }');
add("budget", "budget-error-with-large-logs", 'async () => { for (let i = 0; i < 2000; i++) console.log("log line number", i, "with some padding text"); throw new Error("failed after logging"); }');
add("budget", "budget-long-error-message", 'async () => { throw new Error("e".repeat(50000)); }');
add("budget", "budget-just-under", 'async () => "a".repeat(23998)');
add("budget", "budget-exactly-at", 'async () => "a".repeat(24000)');
add("budget", "budget-just-over", 'async () => "a".repeat(24001)');
add("budget", "budget-object-just-over", 'async () => ({ text: "a".repeat(23995) })');
add("budget", "budget-large-tool-text", 'async () => mock.repeat({ text: "lorem ipsum ", times: 5000 })');
add("budget", "budget-megabyte-result", 'async () => "0123456789abcdef".repeat(70000)');
for (const id of ["budget-long-string", "budget-long-array", "budget-large-object", "budget-large-nested", "budget-large-result-and-logs", "budget-just-under", "budget-exactly-at", "budget-just-over", "budget-object-just-over", "budget-megabyte-result"]) {
  const base = cases.find((c) => c.id === id);
  cases.push({ ...base, id: id + "-transformed", formats: ["runtime"], transform_result: true });
}
add("budget", "budget-small-limit-transformed", 'async () => ({ rows: Array.from({ length: 60 }, (_, i) => ({ id: i, title: "Item " + i })), note: "short" })', { formats: ["runtime"], transform_result: { maxTokens: 500 } });
add("budget", "budget-chars-limit-transformed", 'async () => "lorem ipsum dolor sit amet ".repeat(200)', { formats: ["runtime"], transform_result: { maxChars: 1000 } });

// ---------------------------------------------------------------- globals models reach for
const GLOBAL_NAMES = [
  "globalThis", "self", "window", "global", "document", "navigator", "location", "console",
  "fetch", "Request", "Response", "Headers", "FormData", "Blob", "File", "WebSocket", "EventSource", "XMLHttpRequest",
  "URL", "URLSearchParams", "URLPattern", "TextEncoder", "TextDecoder", "TextEncoderStream", "TextDecoderStream",
  "atob", "btoa", "crypto", "Crypto", "CryptoKey", "SubtleCrypto", "structuredClone", "queueMicrotask",
  "setTimeout", "setInterval", "clearTimeout", "clearInterval", "setImmediate", "clearImmediate", "requestAnimationFrame",
  "performance", "Performance", "scheduler", "AbortController", "AbortSignal", "Event", "EventTarget", "CustomEvent", "MessageChannel", "BroadcastChannel",
  "ReadableStream", "WritableStream", "TransformStream", "CompressionStream", "DecompressionStream", "caches", "DOMException",
  "process", "Buffer", "require", "module", "exports", "__dirname", "__filename", "Deno", "Bun", "fs", "path", "os",
  "Intl", "Temporal", "WebAssembly", "SharedArrayBuffer", "Atomics", "WeakRef", "FinalizationRegistry", "Iterator", "AsyncIterator",
  "AggregateError", "SuppressedError", "DisposableStack", "AsyncDisposableStack", "Float16Array", "BigInt64Array",
  "eval", "Function", "escape", "unescape", "encodeURIComponent", "decodeURIComponent", "encodeURI", "parseInt", "parseFloat", "isNaN",
  "Proxy", "Reflect", "Symbol", "BigInt", "Map", "Set", "WeakMap", "WeakSet", "Promise", "JSON", "Math", "Date", "RegExp", "Error", "ArrayBuffer", "DataView",
  "localStorage", "sessionStorage", "indexedDB", "alert", "prompt", "print", "gc", "std", "scriptArgs",
  "env", "ctx", "host", "tools", "WorkerEntrypoint", "DurableObject", "RpcTarget", "Cloudflare", "HTMLRewriter", "EmailMessage", "reportError", "navigation",
  "__dispatchers", "__connectors", "__logs", "__stringifyForCodemode", "CodeExecutor",
];
add("globals", "globals-typeof-direct", `async () => ({ ${GLOBAL_NAMES.map((n) => `${JSON.stringify(n)}: typeof ${n}`).join(", ")} })`);
add("globals", "globals-internals", "async () => ({ dispatchers: Object.keys(__dispatchers).sort(), connectors: Object.keys(__connectors).sort(), logs: Array.isArray(__logs), codec: typeof __stringifyForCodemode, executor: typeof CodeExecutor, entrypoint: typeof WorkerEntrypoint })");
add("globals", "globals-navigator-shape", 'async () => ({ own: Object.keys(navigator), proto: Object.getOwnPropertyNames(Object.getPrototypeOf(navigator)).sort(), tag: Object.prototype.toString.call(navigator), hardwareConcurrency: navigator.hardwareConcurrency, languages: navigator.languages, platform: navigator.platform, onLine: navigator.onLine })');
add("globals", "globals-process-shape", 'async () => ({ own: Object.getOwnPropertyNames(process).sort(), tag: Object.prototype.toString.call(process), platform: process.platform, envTag: Object.prototype.toString.call(process.env), exit: typeof process.exit, versions: typeof process.versions })');
add("globals", "globals-process-features", 'async () => ({ features: process.features, path: typeof process.getBuiltinModule("node:path"), missing: (() => { try { return String(process.getBuiltinModule("nope")); } catch (e) { return e.name + ": " + e.message; } })(), fs: (() => { try { return typeof process.getBuiltinModule("node:fs"); } catch (e) { return e.name + ": " + e.message; } })(), exitLength: process.exit.length, tickLength: process.nextTick.length, envProto: Object.getPrototypeOf(process.env) === Object.prototype, envSet: (() => { process.env.X = 1; return [process.env.X, typeof process.env.X]; })() })');
add("globals", "globals-process-exit", 'async () => { try { process.exit(1); return "returned"; } catch (e) { return e.name + ": " + e.message; } }', { hang_ms: 6000 });
add("globals", "globals-process-exit-default", 'async () => { console.log("leaving"); process.exit(); }', { hang_ms: 6000 });
add("globals", "globals-process-exit-forms", null, { hang_ms: 6000, steps: [call('async () => process.exit(0)'), call('async () => process.exit("done")'), call('async () => { try { process.exit(2); } finally { console.log("finally ran"); } }'), call('async () => { await mock.add({ a: 1, b: 1 }); process.exit(3.5); }'), call('async () => "still works"')] });
add("globals", "globals-process-nexttick-order", 'async () => { const order = []; Promise.resolve().then(() => order.push("promise 1")); process.nextTick(() => order.push("tick 1")); queueMicrotask(() => order.push("microtask")); process.nextTick(() => order.push("tick 2")); Promise.resolve().then(() => order.push("promise 2")); await new Promise((r) => setTimeout(r, 10)); return order; }');
add("globals", "globals-process-nexttick-errors", 'async () => { try { process.nextTick("not a function"); return "accepted"; } catch (e) { return [e.name, e.message, e.code]; } }');
add("globals", "globals-process-env-forms", 'async () => { process.env.A = 5; process.env.B = null; process.env.C = { x: 1 }; delete process.env.A; return [process.env.A === undefined, process.env.B, process.env.C, Object.keys(process.env), JSON.stringify(process.env), "B" in process.env, typeof process.env.toString]; }');
add("globals", "globals-scheduler-wait", 'async () => { const order = []; setTimeout(() => order.push("timer 20"), 20); await scheduler.wait(40); order.push("waited"); return [order, typeof scheduler.wait(1).then, await scheduler.wait(1)]; }');
add("globals", "globals-navigator-sendbeacon-forms", 'async () => [navigator.sendBeacon("https://example.com/"), (() => { try { return navigator.sendBeacon(); } catch (e) { return e.name + ": " + e.message; } })(), (() => { try { return navigator.sendBeacon("not a url"); } catch (e) { return e.name + ": " + e.message; } })()]');
add("globals", "globals-process-nexttick", 'async () => { const order = []; process.nextTick((a) => order.push("tick " + a), "arg"); Promise.resolve().then(() => order.push("promise")); queueMicrotask(() => order.push("microtask")); order.push("sync"); await new Promise((r) => setTimeout(r, 10)); return order; }');
add("globals", "globals-navigator-sendbeacon", 'async () => { try { return navigator.sendBeacon("https://example.com/", "x"); } catch (e) { return e.name + ": " + e.message; } }');
add("globals", "globals-scope", 'async () => { const chain = []; for (let o = Object.getPrototypeOf(globalThis); o; o = Object.getPrototypeOf(o)) chain.push(o.constructor && o.constructor.name); return { chain, type: typeof ServiceWorkerGlobalScope, isScope: typeof ServiceWorkerGlobalScope === "function" && globalThis instanceof ServiceWorkerGlobalScope, tag: Object.prototype.toString.call(globalThis), origin: typeof origin === "undefined" ? "none" : String(origin), fetchOwn: Object.prototype.hasOwnProperty.call(globalThis, "fetch"), fetchIn: "fetch" in globalThis }; }');
add("globals", "globals-workers-own", 'async () => ({ scheduler: typeof scheduler === "object" ? Object.getOwnPropertyNames(Object.getPrototypeOf(scheduler)).sort() : typeof scheduler, caches: typeof caches === "object" ? Object.prototype.toString.call(caches) : typeof caches, cloudflare: typeof Cloudflare === "object" ? Object.keys(Cloudflare).sort() : typeof Cloudflare, report: typeof reportError })');
add("globals", "globals-performance-shape", 'async () => ({ proto: Object.getOwnPropertyNames(Object.getPrototypeOf(performance)).sort(), own: Object.keys(performance), timeOrigin: performance.timeOrigin, early: performance.now() < 60000 })');
add("globals", "globals-console-keys", "async () => Object.keys(console)");
add("globals", "globals-dom-exception", 'async () => { const e = new DOMException("m", "AbortError"); const plain = new DOMException(); return { name: e.name, message: e.message, code: e.code, string: String(e), keys: Object.keys(e), isError: e instanceof Error, ctor: e.constructor.name, plain: [plain.name, plain.message, plain.code], custom: new DOMException("x", "MadeUp").code, stack: typeof e.stack, tag: Object.prototype.toString.call(e), timeout: new DOMException("t", "TimeoutError").code, protoKeys: Object.getOwnPropertyNames(DOMException.prototype).sort().slice(0, 8) }; }');
add("globals", "globals-own-names", "async () => Object.getOwnPropertyNames(globalThis).sort()");
add("globals", "globals-this", 'async () => { const t = this; return { type: typeof t, isGlobal: t === globalThis, keys: t ? Object.keys(t) : null, env: t ? typeof t.env : null, ctx: t ? typeof t.ctx : null }; }');
add("globals", "globals-eval", 'async () => { try { return eval("1 + 1"); } catch (e) { return e.name + ": " + e.message; } }');
add("globals", "globals-eval-uncaught", 'async () => eval("({ a: 1 })")');
add("globals", "globals-new-function", 'async () => { try { return new Function("a", "b", "return a + b")(1, 2); } catch (e) { return e.name + ": " + e.message; } }');
add("globals", "globals-new-function-uncaught", 'async () => new Function("return 1")()');
add("globals", "globals-async-function-constructor", 'async () => { const AsyncFunction = (async () => {}).constructor; try { return await new AsyncFunction("return 5")(); } catch (e) { return e.name + ": " + e.message; } }');
add("globals", "globals-dynamic-import", 'async () => { try { const m = await import("node:fs"); return Object.keys(m).length > 0 ? "imported node:fs" : "empty module"; } catch (e) { return e.name + ": " + e.message; } }');
add("globals", "globals-dynamic-import-bare", 'async () => { try { await import("lodash"); return "imported"; } catch (e) { return e.name + ": " + e.message; } }');
add("globals", "globals-dynamic-import-uncaught", 'async () => (await import("node:path")).join("a", "b")');
add("globals", "globals-require", 'async () => { const fs = require("fs"); return fs.readFileSync("/etc/hostname", "utf8"); }');
add("globals", "globals-require-caught", 'async () => { try { require("fs"); return "required"; } catch (e) { return e.name + ": " + e.message; } }');
add("globals", "globals-process", 'async () => { try { return { type: typeof process, env: typeof process.env, envKeys: Object.keys(process.env).length, platform: typeof process.platform, nextTick: typeof process.nextTick, cwd: typeof process.cwd, argv: Array.isArray(process.argv), version: typeof process.version }; } catch (e) { return e.name + ": " + e.message; } }');
add("globals", "globals-process-env", 'async () => process.env.API_KEY ?? "no API_KEY"');
add("globals", "globals-process-uncaught", "async () => process.cwd()");
add("globals", "globals-buffer", 'async () => { try { return [Buffer.from("héllo").toString("base64"), Buffer.from("aGk=", "base64").toString(), Buffer.byteLength("é")]; } catch (e) { return e.name + ": " + e.message; } }');
add("globals", "globals-navigator", 'async () => { try { return { userAgent: navigator.userAgent, language: navigator.language, hardwareConcurrency: typeof navigator.hardwareConcurrency }; } catch (e) { return e.name + ": " + e.message; } }');
add("globals", "globals-fs-write", 'async () => { fs.writeFileSync("/tmp/out.txt", "x"); return "written"; }');
add("globals", "globals-host", 'async () => host.readFile("/etc/passwd")');
add("globals", "globals-window", "async () => window.location.href");
add("globals", "globals-document", 'async () => document.querySelector("body")');
add("globals", "globals-alert", 'async () => alert("hi")');

add("fetch", "fetch-uncaught", 'async () => { const res = await fetch("https://example.com/api"); return res.status; }');
add("fetch", "fetch-caught", 'async () => { try { await fetch("https://example.com/api"); return "fetched"; } catch (e) { return { name: e.name, message: e.message, isError: e instanceof Error, isTypeError: e instanceof TypeError, string: String(e), ctor: e.constructor.name }; } }');
add("fetch", "fetch-sync-or-async", 'async () => { let p; try { p = fetch("https://example.com/"); } catch (e) { return "threw synchronously: " + e.message; } const kind = Object.prototype.toString.call(p); try { await p; return kind + " resolved"; } catch (e) { return kind + " rejected"; } }');
add("fetch", "fetch-unawaited", 'async () => { fetch("https://example.com/").catch(() => {}); return "returned"; }');
add("fetch", "fetch-unawaited-unhandled", 'async () => { fetch("https://example.com/"); return "returned"; }');
add("fetch", "fetch-post-json", 'async () => { const res = await fetch("https://api.example.com/items", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ a: 1 }) }); return res.json(); }');
add("fetch", "fetch-localhost", 'async () => (await fetch("http://127.0.0.1:1/")).text()');
add("fetch", "fetch-invalid-url", 'async () => fetch("not a url")');
add("fetch", "fetch-relative-url", 'async () => fetch("/api/items")');
add("fetch", "fetch-no-arguments", "async () => fetch()");
add("fetch", "fetch-argument-errors-caught", 'async () => { const out = {}; for (const [k, f] of Object.entries({ none: () => fetch(), invalid: () => fetch("not a url"), relative: () => fetch("/api"), number: () => fetch(5), object: () => fetch({ url: "https://example.com/" }), nullUrl: () => fetch(null) })) { let how = "sync"; try { const p = f(); how = "async"; await p; out[k] = "resolved"; } catch (e) { out[k] = [how, e.name, e.message, e instanceof TypeError]; } } return out; }');
add("fetch", "fetch-request-object", 'async () => { try { return await fetch(new Request("https://example.com/")); } catch (e) { return e.name + ": " + e.message; } }');
add("fetch", "fetch-in-promise-all", 'async () => Promise.all(["https://a.example/", "https://b.example/"].map((u) => fetch(u)))');
add("fetch", "fetch-then-chain", 'async () => fetch("https://example.com/").then((r) => r.json()).catch((e) => "fallback: " + e.message)');
add("fetch", "fetch-data-url", 'async () => { try { return await (await fetch("data:text/plain,hello")).text(); } catch (e) { return e.name + ": " + e.message; } }');
add("fetch", "fetch-abort-signal", 'async () => { try { const c = new AbortController(); c.abort(); await fetch("https://example.com/", { signal: c.signal }); return "fetched"; } catch (e) { return e.name + ": " + e.message; } }');
add("fetch", "fetch-websocket", 'async () => { try { const ws = new WebSocket("wss://example.com/"); return typeof ws; } catch (e) { return e.name + ": " + e.message; } }');
add("fetch", "fetch-xhr", "async () => { const x = new XMLHttpRequest(); return typeof x; }");
add("fetch", "fetch-response-local", 'async () => { try { const r = new Response(JSON.stringify({ a: 1 }), { headers: { "content-type": "application/json" } }); return [r.status, r.ok, await r.json(), r.headers.get("content-type")]; } catch (e) { return e.name + ": " + e.message; } }');
add("fetch", "fetch-headers-local", 'async () => { try { const h = new Headers({ "X-A": "1" }); h.append("x-b", "2"); return [...h.entries()]; } catch (e) { return e.name + ": " + e.message; } }');
add("fetch", "fetch-connect", 'async () => { try { const { connect } = await import("cloudflare:sockets"); connect("example.com:80"); return "connected"; } catch (e) { return e.name + ": " + e.message; } }');

add("url", "url-parts", 'async () => { const u = new URL("https://user:pw@Example.com:8443/a/b%20c/../d?x=1&y=two&x=3#frag"); return { href: u.href, origin: u.origin, protocol: u.protocol, username: u.username, password: u.password, host: u.host, hostname: u.hostname, port: u.port, pathname: u.pathname, search: u.search, hash: u.hash, x: u.searchParams.get("x"), all: u.searchParams.getAll("x"), string: String(u), json: JSON.stringify({ u }) }; }');
add("url", "url-relative", 'async () => [new URL("../c?d=1", "https://example.com/a/b").href, new URL("/root", "https://example.com/a/b?q#h").href, new URL("//other.example/x", "https://example.com").href, new URL("?only=query", "https://example.com/p").href, new URL("#h", "https://example.com/p?q").href]');
add("url", "url-default-port", 'async () => [new URL("https://example.com:443/").href, new URL("http://example.com:80/x").port, new URL("http://example.com:8080").host, new URL("HTTP://EXAMPLE.COM").href]');
add("url", "url-encoding", 'async () => [new URL("https://example.com/a b?q=é ü&r=a+b#x y").href, new URL("https://exämple.com/päth").href, new URL("https://example.com/%7Euser/%2F").pathname, new URL("https://example.com/?q=%20%zz").search]');
add("url", "url-invalid", 'async () => new URL("not a url")');
add("url", "url-invalid-caught", 'async () => { try { new URL("/relative/only"); } catch (e) { return { name: e.name, message: e.message, isTypeError: e instanceof TypeError, code: e.code }; } }');
add("url", "url-invalid-forms", 'async () => ["", "http://", "//x", "example.com", "http://exa mple.com", "http://[::1", "http://256.1.1.1/", "https://example.com:99999/"].map((s) => { try { return new URL(s).href; } catch (e) { return e.name + ": " + e.message; } })');
add("url", "url-canparse", 'async () => [typeof URL.canParse, URL.canParse("https://example.com"), URL.canParse("nope"), URL.canParse("/x", "https://example.com"), typeof URL.parse, URL.parse ? URL.parse("nope") : "no URL.parse"]');
add("url", "url-object-shape", 'async () => { const u = new URL("https://a.example/p?q=1"); return { keys: Object.keys(u), json: JSON.stringify(u), tag: Object.prototype.toString.call(u), concat: u + "", sameParams: u.searchParams === u.searchParams, paramsTag: Object.prototype.toString.call(u.searchParams), paramsJson: JSON.stringify(u.searchParams), paramsKeys: Object.keys(u.searchParams), originSet: (() => { try { u.origin = "https://x.example"; return u.origin; } catch (e) { return e.name; } })(), noArgs: (() => { try { new URL(); } catch (e) { return e.name + ": " + e.message; } })(), badBase: (() => { try { new URL("/x", "nope"); } catch (e) { return e.name + ": " + e.message; } })(), hrefSet: (() => { try { u.href = "nope"; return u.href; } catch (e) { return e.name + ": " + e.message; } })() }; }');
add("url", "url-setters", 'async () => { const u = new URL("https://example.com/a?x=1"); u.pathname = "/b c"; u.searchParams.set("x", "2"); u.searchParams.append("y", "é&="); u.hash = "top"; u.port = "8080"; u.protocol = "http"; u.hostname = "other.example"; return [u.href, u.search]; }');
add("url", "url-search-setter", 'async () => { const u = new URL("https://example.com/"); u.search = "a=1&b=2"; const first = [...u.searchParams.keys()]; u.search = ""; return [first, u.href, u.searchParams.toString()]; }');
add("url", "url-special-schemes", 'async () => ["file:///etc/hosts", "mailto:a@b.c", "data:text/plain,hi", "ftp://example.com/x", "blob:https://example.com/1", "javascript:alert(1)", "custom+scheme://host/path?q"].map((s) => { const u = new URL(s); return [u.protocol, u.host, u.pathname, u.origin]; })');
add("url", "url-ip-hosts", 'async () => ["http://127.0.0.1:8080/", "http://[::1]:3000/x", "http://0x7f.1/", "http://2130706433/", "http://[2001:db8::1]/"].map((s) => { const u = new URL(s); return [u.hostname, u.host, u.href]; })');
add("url", "url-idn", 'async () => [new URL("https://bücher.example/").hostname, new URL("https://xn--bcher-kva.example/").hostname, new URL("https://EXAMPLE.com./").hostname]');
add("url", "url-path-normalization", 'async () => ["https://example.com/a/./b/../c", "https://example.com/a//b", "https://example.com/a/%2e%2e/b", "https://example.com\\\\a\\\\b", "https://example.com/a/..", "https://example.com"].map((s) => new URL(s).pathname)');
add("url", "urlsearchparams-build", 'async () => { const p = new URLSearchParams({ q: "tide tables", page: 2, tag: "é&=", empty: "" }); p.append("tag", "second"); return [p.toString(), p.get("page"), p.getAll("tag"), p.has("q"), p.has("nope"), p.get("nope"), [...p.keys()], p.size]; }');
add("url", "urlsearchparams-parse", 'async () => { const p = new URLSearchParams("?a=1&b=two+words&c=%C3%A9&a=3&novalue&=x&d=a%3Db"); return [Object.fromEntries(p), [...p.entries()], p.toString()]; }');
add("url", "urlsearchparams-mutate", 'async () => { const p = new URLSearchParams("b=2&a=1&c=3&a=0"); p.sort(); const sorted = p.toString(); p.delete("a"); p.set("c", "new"); p.set("z", "last"); return [sorted, p.toString(), [...p]]; }');
add("url", "urlsearchparams-from-pairs", 'async () => [new URLSearchParams([["a", "1"], ["a", "2"]]).toString(), new URLSearchParams(new URLSearchParams("x=1")).toString(), new URLSearchParams("").toString(), new URLSearchParams().toString(), new URLSearchParams({ a: [1, 2], b: null, c: undefined }).toString()]');
add("url", "urlsearchparams-encoding", 'async () => new URLSearchParams({ s: "a b+c~d*e\'f(g)!h", u: "\\u{1F600}/?:@", n: "line\\nbreak" }).toString()');
add("url", "urlsearchparams-in-template", 'async () => `https://api.example.com/search?${new URLSearchParams({ q: "a&b", n: 10 })}`');
add("url", "urlsearchparams-foreach", 'async () => { const out = []; new URLSearchParams("a=1&b=2").forEach((v, k) => out.push(k + "=" + v)); return out; }');
add("url", "url-encode-functions", 'async () => [encodeURIComponent("a b&c=d/é?"), encodeURI("https://example.com/a b?q=é&r=1#h"), decodeURIComponent("%C3%A9%20x"), escape("a b+é"), unescape("%E9%20")]');

add("encoding", "textencoder", 'async () => { const b = new TextEncoder().encode("héllo € \\u{1F600}"); return [b.constructor.name, b.length, Array.from(b)]; }');
add("encoding", "textencoder-props", 'async () => { const e = new TextEncoder(); return [e.encoding, typeof e.encodeInto, Array.from(e.encode()), Array.from(e.encode("")), Array.from(e.encode("\\ud83d"))]; }');
add("encoding", "textencoder-encodeinto", 'async () => { const buf = new Uint8Array(5); const r = new TextEncoder().encodeInto("héllo", buf); return [r, Array.from(buf)]; }');
add("encoding", "textdecoder", 'async () => [new TextDecoder().decode(new Uint8Array([104, 195, 169, 108, 108, 111, 32, 226, 130, 172])), new TextDecoder().decode(new Uint8Array([240, 159, 152, 128])), new TextDecoder().decode(), new TextDecoder("utf-8").encoding]');
add("encoding", "textdecoder-invalid-bytes", 'async () => [new TextDecoder().decode(new Uint8Array([255, 104, 105, 195])), new TextDecoder().decode(new Uint8Array([0xef, 0xbb, 0xbf, 104, 105])), new TextDecoder("utf-8", { ignoreBOM: true }).decode(new Uint8Array([0xef, 0xbb, 0xbf, 104])).length]');
add("encoding", "textdecoder-fatal", 'async () => { try { return new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array([255])); } catch (e) { return { name: e.name, message: e.message, isTypeError: e instanceof TypeError }; } }');
add("encoding", "textdecoder-other-encodings", 'async () => ["utf-16le", "latin1", "iso-8859-1", "windows-1252", "ascii", "utf-16be", "UTF-8", " utf8 ", "nope", ""].map((label) => { try { const d = new TextDecoder(label); return [label, d.encoding, d.decode(new Uint8Array([104, 0, 233, 0, 0x80, 0x9f]))]; } catch (e) { return [label, e.name + ": " + e.message]; } })');
add("encoding", "textdecoder-legacy-encodings", 'async () => ["shift_jis", "gbk", "euc-kr", "iso-8859-2", "windows-1251", "big5"].map((label) => { try { const d = new TextDecoder(label); return [label, d.encoding, d.decode(new Uint8Array([104, 105, 0xe9]))]; } catch (e) { return [label, e.name + ": " + e.message]; } })');
add("encoding", "textdecoder-arraybuffer", 'async () => [new TextDecoder().decode(new Uint8Array([104, 105]).buffer), new TextDecoder().decode(new DataView(new Uint8Array([111, 107]).buffer)), new TextDecoder().decode(new Uint16Array([26984]))]');
add("encoding", "textdecoder-stream", 'async () => { const d = new TextDecoder(); return d.decode(new Uint8Array([226, 130]), { stream: true }) + "|" + d.decode(new Uint8Array([172])); }');
add("encoding", "base64-btoa-atob", 'async () => [btoa("hello"), atob("aGVsbG8="), btoa(""), atob(""), btoa("é"), atob("6Q==").charCodeAt(0), btoa(String.fromCharCode(0, 255, 128)), atob("aGVsbG8"), atob(" aGVs bG8= "), btoa(123), btoa(null), btoa(undefined)]');
add("encoding", "base64-btoa-unicode", 'async () => btoa("€ is outside latin1")');
add("encoding", "base64-btoa-unicode-caught", 'async () => { try { btoa("\\u{1F600}"); } catch (e) { return { name: e.name, message: e.message, ctor: e.constructor.name, isError: e instanceof Error, code: e.code, string: String(e) }; } }');
add("encoding", "base64-atob-invalid", 'async () => atob("***not base64***")');
add("encoding", "base64-atob-invalid-caught", 'async () => ["a", "ab=c", "abcde", "ab==cd", "a===", "aGVsbG8=="].map((s) => { try { return atob(s); } catch (e) { return e.name + ": " + e.message; } })');
add("encoding", "base64-no-arguments", "async () => btoa()");
add("encoding", "base64-utf8-roundtrip", 'async () => { const bytes = new TextEncoder().encode("héllo ☃"); const b64 = btoa(String.fromCharCode(...bytes)); const back = new TextDecoder().decode(Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))); return [b64, back]; }');
add("encoding", "base64-uint8array-methods", 'async () => [typeof Uint8Array.fromBase64, typeof Uint8Array.prototype.toBase64, typeof Uint8Array.prototype.toHex, Uint8Array.prototype.toBase64 ? new Uint8Array([104, 105]).toBase64() : null]');

add("crypto", "crypto-random-uuid", 'async () => { const a = crypto.randomUUID(); const b = crypto.randomUUID(); return [typeof a, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(a), a !== b, a.length]; }');
add("crypto", "crypto-get-random-values", "async () => { const a = crypto.getRandomValues(new Uint8Array(16)); const b = crypto.getRandomValues(new Uint32Array(2)); return [a.constructor.name, a.length, a.some((x) => x !== 0), b.length, a === crypto.getRandomValues(a)]; }");
add("crypto", "crypto-get-random-values-errors", 'async () => [() => crypto.getRandomValues(new Float64Array(2)), () => crypto.getRandomValues(new Uint8Array(70000)), () => crypto.getRandomValues([1, 2]), () => crypto.getRandomValues()].map((f) => { try { f(); return "ok"; } catch (e) { return e.name + ": " + e.message; } })');
add("crypto", "crypto-shape", 'async () => ({ crypto: typeof crypto, tag: Object.prototype.toString.call(crypto), subtle: typeof crypto.subtle, digest: typeof crypto.subtle?.digest, randomUUID: typeof crypto.randomUUID, getRandomValues: typeof crypto.getRandomValues, keys: Object.keys(crypto), digestStream: typeof crypto.DigestStream, createHash: typeof crypto.createHash, randomBytes: typeof crypto.randomBytes })');
add("crypto", "crypto-subtle-digest", 'async () => { const hex = (buf) => [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join(""); const data = new TextEncoder().encode("abc"); return { sha256: hex(await crypto.subtle.digest("SHA-256", data)), sha1: hex(await crypto.subtle.digest("SHA-1", data)), sha512: hex(await crypto.subtle.digest({ name: "SHA-512" }, data)).slice(0, 32), md5: await crypto.subtle.digest("MD5", data).then(hex, (e) => e.name + ": " + e.message) }; }');
add("crypto", "crypto-subtle-hmac", 'async () => { try { const key = await crypto.subtle.importKey("raw", new TextEncoder().encode("secret"), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]); const sig = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode("message")); return [...new Uint8Array(sig)].map((b) => b.toString(16).padStart(2, "0")).join(""); } catch (e) { return e.name + ": " + e.message; } }');
add("crypto", "crypto-node-module", 'async () => { try { const { createHash, randomUUID } = await import("node:crypto"); return [createHash("sha256").update("abc").digest("hex"), typeof randomUUID()]; } catch (e) { return e.name + ": " + e.message; } }');
add("crypto", "crypto-math-random", "async () => { const xs = Array.from({ length: 20 }, () => Math.random()); return [xs.every((x) => x >= 0 && x < 1), new Set(xs).size > 1]; }");

add("clone", "structuredclone-plain", 'async () => { const o = { a: 1, b: [1, { c: "d" }], n: null, u: undefined }; const c = structuredClone(o); return [c, c !== o, c.b !== o.b, "u" in c]; }');
add("clone", "structuredclone-types", 'async () => { const c = structuredClone({ d: new Date(0), m: new Map([["k", { v: 1 }]]), s: new Set([1, 2]), r: /x/gi, b: new Uint8Array([1, 2]), big: 5n, e: new RangeError("r"), nan: NaN, neg: -0, buf: new ArrayBuffer(2), sparse: [1, , 3] }); return { d: [c.d instanceof Date, c.d.getTime()], m: [c.m instanceof Map, c.m.get("k")], s: [c.s instanceof Set, [...c.s]], r: [c.r instanceof RegExp, c.r.source, c.r.flags, c.r.lastIndex], b: [c.b.constructor.name, [...c.b]], big: typeof c.big, e: [c.e instanceof RangeError, c.e.name, c.e.message], nan: Number.isNaN(c.nan), neg: Object.is(c.neg, -0), buf: [c.buf.constructor.name, c.buf.byteLength], sparse: [c.sparse.length, 1 in c.sparse] }; }');
add("clone", "structuredclone-cycle", "async () => { const a = { name: \"a\" }; a.self = a; a.list = [a]; const c = structuredClone(a); return [c.self === c, c.list[0] === c, c !== a]; }");
add("clone", "structuredclone-shared", "async () => { const shared = { v: 1 }; const c = structuredClone({ x: shared, y: shared }); return c.x === c.y; }");
add("clone", "structuredclone-class-instance", "async () => { class P { constructor() { this.x = 1; } m() {} } const c = structuredClone(new P()); return [c instanceof P, c.constructor === Object, c, Object.getPrototypeOf(c) === Object.prototype]; }");
add("clone", "structuredclone-function", "async () => structuredClone({ f: () => 1 })");
add("clone", "structuredclone-function-caught", 'async () => [() => structuredClone(() => 1), () => structuredClone({ s: Symbol("x") }), () => structuredClone(new WeakMap()), () => structuredClone(Promise.resolve(1)), () => structuredClone()].map((f) => { try { f(); return "ok"; } catch (e) { return { name: e.name, message: e.message, ctor: e.constructor.name, code: e.code }; } })');
add("clone", "structuredclone-primitives", 'async () => [structuredClone(1), structuredClone("s"), structuredClone(null), structuredClone(undefined), structuredClone(true), typeof structuredClone(1n)]');
add("clone", "structuredclone-getters-and-descriptors", 'async () => { const o = { get g() { return 5; } }; Object.defineProperty(o, "hidden", { value: 1, enumerable: false }); const c = structuredClone(o); return [c, Object.getOwnPropertyDescriptor(c, "g").value, "hidden" in c]; }');
add("clone", "structuredclone-transfer", "async () => { const buf = new ArrayBuffer(8); const c = structuredClone(buf, { transfer: [buf] }); return [buf.byteLength, c.byteLength]; }");
add("clone", "structuredclone-error-fields", 'async () => { const e = new Error("m", { cause: { why: 1 } }); e.extra = "x"; const c = structuredClone(e); return [c.message, c.cause, c.extra, typeof c.stack, c instanceof Error]; }');

add("intl", "intl-shape", 'async () => typeof Intl === "object" ? Object.getOwnPropertyNames(Intl).sort() : typeof Intl');
add("intl", "intl-numberformat", 'async () => [new Intl.NumberFormat("en-US").format(1234567.891), new Intl.NumberFormat().format(1234.5), new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" }).format(1234.5), new Intl.NumberFormat("en-US", { style: "percent", maximumFractionDigits: 1 }).format(0.1234), new Intl.NumberFormat("en-US", { notation: "compact" }).format(1234567), new Intl.NumberFormat("en-US", { minimumFractionDigits: 2 }).format(5), new Intl.NumberFormat("en-US", { style: "unit", unit: "kilometer-per-hour" }).format(50)]');
add("intl", "intl-numberformat-locales", 'async () => ["de-DE", "fr-FR", "en-IN", "ja-JP", "ar-EG", "en-GB", "xx-invalid"].map((l) => { try { return [l, new Intl.NumberFormat(l).format(1234567.891), new Intl.NumberFormat(l, { style: "currency", currency: "EUR" }).format(1234.5)]; } catch (e) { return [l, e.name + ": " + e.message]; } })');
add("intl", "intl-datetimeformat", 'async () => { const d = new Date(1700000000000); return [new Intl.DateTimeFormat("en-US").format(d), new Intl.DateTimeFormat("en-US", { dateStyle: "medium", timeStyle: "short", timeZone: "UTC" }).format(d), new Intl.DateTimeFormat("en-US", { weekday: "long", year: "numeric", month: "long", day: "numeric" }).format(d), new Intl.DateTimeFormat("en-GB", { timeZone: "Europe/Paris", dateStyle: "full", timeStyle: "long" }).format(d), new Intl.DateTimeFormat("en-US", { timeZone: "America/New_York", hour: "numeric", minute: "2-digit" }).format(d), new Intl.DateTimeFormat("de-DE").format(d)]; }');
add("intl", "intl-resolved-options", "async () => { const o = Intl.DateTimeFormat().resolvedOptions(); return { locale: o.locale, timeZone: o.timeZone, calendar: o.calendar, numberingSystem: o.numberingSystem, number: new Intl.NumberFormat().resolvedOptions().locale }; }");
add("intl", "intl-others", 'async () => [new Intl.PluralRules("en-US").select(1), new Intl.PluralRules("en-US", { type: "ordinal" }).select(2), new Intl.RelativeTimeFormat("en", { numeric: "auto" }).format(-1, "day"), new Intl.ListFormat("en", { style: "long", type: "conjunction" }).format(["a", "b", "c"]), ["ä", "z", "a", "Z"].sort(new Intl.Collator("de").compare), [...new Intl.Segmenter("en", { granularity: "word" }).segment("Hello, world")].filter((s) => s.isWordLike).map((s) => s.segment), new Intl.DisplayNames(["en"], { type: "region" }).of("FR"), Intl.supportedValuesOf("timeZone").length > 100, Intl.getCanonicalLocales("EN-us")]');
add("intl", "intl-missing-uncaught", 'async () => new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" }).format(1234.5)');
add("intl", "number-tolocalestring", "async () => [(1234567.891).toLocaleString(), (1234.5).toLocaleString(), (0.5).toLocaleString(), (1000).toLocaleString(), (999).toLocaleString(), (-1234567).toLocaleString(), (1e21).toLocaleString(), (1.23456789).toLocaleString(), (0.0001).toLocaleString(), (1.0005).toLocaleString(), (2.5).toLocaleString(), NaN.toLocaleString(), Infinity.toLocaleString(), (-0).toLocaleString(), (123456789012345680000).toLocaleString(), 1234567n.toLocaleString(), (1234.5678).toLocaleString(undefined), (12).toLocaleString()]");
add("intl", "number-tolocalestring-default-locale", 'async () => [(1234567.891).toLocaleString("en-US"), (1234567.891).toLocaleString("en"), (1234567.891).toLocaleString([]), (1234567.891).toLocaleString(undefined)]');
add("intl", "number-tolocalestring-locale", 'async () => [(1234567.891).toLocaleString("de-DE"), (1234567.891).toLocaleString("fr-FR"), (1234567.891).toLocaleString("en-IN"), (1234567.891).toLocaleString(["de", "en"])]');
add("intl", "number-tolocalestring-options", 'async () => [(1234.5).toLocaleString("en-US", { style: "currency", currency: "USD" }), (0.256).toLocaleString("en-US", { style: "percent" }), (1234.5678).toLocaleString("en-US", { maximumFractionDigits: 1 }), (5).toLocaleString("en-US", { minimumFractionDigits: 2 }), (1234.5).toLocaleString(undefined, { maximumFractionDigits: 0 }), (1234567).toLocaleString("en-US", { notation: "compact" }), (1234.5).toLocaleString("en-US", { useGrouping: false }), (42).toLocaleString("en-US", { minimumIntegerDigits: 4 }), (1234.5).toLocaleString("en-US", { style: "currency", currency: "EUR" }), (1234.5).toLocaleString("en-US", { style: "currency", currency: "JPY" })]');
add("intl", "number-tolocalestring-bad-locale", 'async () => (1234.5).toLocaleString("not a locale")');
add("intl", "date-tolocalestring", "async () => { const d = new Date(1700000000000); const e = new Date(Date.UTC(2025, 0, 5, 0, 7, 9)); return [d.toLocaleString(), d.toLocaleDateString(), d.toLocaleTimeString(), e.toLocaleString(), e.toLocaleDateString(), e.toLocaleTimeString(), new Date(Date.UTC(2025, 11, 31, 12, 0, 0)).toLocaleString(), new Date(NaN).toLocaleString(), new Date(Date.UTC(-1, 0, 1)).toLocaleDateString(), new Date(Date.UTC(99, 5, 15, 23, 59, 59)).toLocaleString()]; }");
add("intl", "date-tolocalestring-default-locale", 'async () => { const d = new Date(1700000000000); return [d.toLocaleString("en-US"), d.toLocaleDateString("en-US"), d.toLocaleTimeString("en-US"), d.toLocaleDateString(undefined), d.toLocaleString([]), d.toLocaleTimeString("en")]; }');
add("intl", "date-tolocalestring-locale", 'async () => { const d = new Date(1700000000000); return [d.toLocaleString("en-GB"), d.toLocaleDateString("de-DE"), d.toLocaleString("fr-FR"), d.toLocaleString("ja-JP")]; }');
add("intl", "date-tolocalestring-options", 'async () => { const d = new Date(1700000000000); return [d.toLocaleDateString("en-US", { weekday: "long", year: "numeric", month: "long", day: "numeric" }), d.toLocaleDateString("en-US", { month: "short", day: "numeric" }), d.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit" }), d.toLocaleString("en-US", { timeZone: "America/New_York" }), d.toLocaleString("en-US", { timeZone: "UTC", timeZoneName: "short" }), d.toLocaleString("en-US", { hour12: false }), d.toLocaleDateString("en-US", { dateStyle: "medium" }), d.toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" })]; }');
add("intl", "string-locale-compare", 'async () => [["b", "a", "C", "ä", "Z", "é", "e"].sort((x, y) => x.localeCompare(y)), "a".localeCompare("b"), "a".localeCompare("A"), "ä".localeCompare("z", "de"), "a".localeCompare("á", "en", { sensitivity: "base" }), "10".localeCompare("9", undefined, { numeric: true }), ["item10", "item9", "Item1", "item-2", "item_3", "item 4", "item.5"].sort((x, y) => x.localeCompare(y)), "résumé".localeCompare("resume"), "a".localeCompare("a"), ["Zoe", "adam", "Émile", "eve", "Bob"].sort((x, y) => x.localeCompare(y))]');
add("intl", "string-locale-case", 'async () => ["I".toLocaleLowerCase("tr"), "i".toLocaleUpperCase("tr"), "ß".toLocaleUpperCase(), "É".toLocaleLowerCase()]');
add("intl", "array-tolocalestring", "async () => [1234.5, new Date(0), \"s\"].toLocaleString()");
add("intl", "array-tolocalestring-locale", 'async () => [1e6, 2e6].toLocaleString("de-DE")');

add("dates", "date-to-string-forms", "async () => { const d = new Date(1700000000000); return { string: String(d), toString: d.toString(), date: d.toDateString(), time: d.toTimeString(), utc: d.toUTCString(), iso: d.toISOString(), json: d.toJSON(), gmt: typeof d.toGMTString === \"function\" ? d.toGMTString() : null, value: d.valueOf(), template: `${d}`, concat: \"\" + d, called: Date(0).length > 20 }; }");
add("dates", "date-to-string-edge", 'async () => [String(new Date(0)), String(new Date(NaN)), String(new Date(Date.UTC(1, 0, 1))), String(new Date(Date.UTC(-100, 0, 1))), String(new Date(Date.UTC(12345, 0, 1))), new Date(8.64e15).toISOString(), String(new Date(8.64e15 + 1))]');
add("dates", "date-timezone", "async () => { const d = new Date(1700000000000); const j = new Date(Date.UTC(2025, 6, 1, 12)); return { offset: d.getTimezoneOffset(), julyOffset: j.getTimezoneOffset(), hours: d.getHours(), utcHours: d.getUTCHours(), day: d.getDay(), date: d.getDate(), local: new Date(2025, 0, 15, 10, 30).toISOString(), localParts: new Date(2025, 5, 1).getTime(), tz: Intl.DateTimeFormat().resolvedOptions().timeZone }; }");
add("dates", "date-timezone-no-intl", "async () => { const d = new Date(1700000000000); const j = new Date(Date.UTC(2025, 6, 1, 12)); return { offset: d.getTimezoneOffset(), julyOffset: j.getTimezoneOffset(), hours: d.getHours(), utcHours: d.getUTCHours(), day: d.getDay(), date: d.getDate(), month: d.getMonth(), year: d.getFullYear(), minutes: d.getMinutes(), local: new Date(2025, 0, 15, 10, 30).toISOString(), localParts: new Date(2025, 5, 1).getTime(), twoDigitYear: new Date(99, 0).getFullYear(), monthOverflow: new Date(2025, 12, 1).toISOString(), setHours: new Date(new Date(0).setHours(5, 30)).toISOString(), setDate: new Date(new Date(Date.UTC(2025, 0, 31)).setDate(32)).toISOString(), setMonth: new Date(new Date(Date.UTC(2025, 0, 31)).setMonth(1)).toISOString(), setFullYear: new Date(new Date(Date.UTC(2024, 1, 29)).setFullYear(2025)).toISOString() }; }");
const DATE_STRINGS = [
  "2025-03-07", "2025-03-07T10:00:00", "2025-03-07T10:00:00Z", "2025-03-07T10:00:00.123Z", "2025-03-07T10:00:00+02:00", "2025-03-07T10:00", "2025-03-07 10:00", "2025-03-07 10:00:00",
  "2025-03", "2025", "20250307", "2025-3-7", "2025/03/07", "2025/3/7 10:00", "03/07/2025", "3/7/2025", "12/25/25", "07.03.2025", "07-03-2025", "7-Mar-2025",
  "March 7, 2025", "Mar 7, 2025", "Mar 7 2025", "7 March 2025", "7 Mar 2025", "March 2025", "Mar 2025", "March 7", "Friday, March 7, 2025", "Fri Mar 07 2025",
  "March 7, 2025 10:00", "March 7, 2025 10:00 PM", "Mar 7, 2025, 10:00:00 PM", "March 7, 2025 10:00:00 GMT", "March 7, 2025 at 10:00",
  "Fri, 07 Mar 2025 10:00:00 GMT", "Fri, 07 Mar 2025 10:00:00 +0100", "Fri Mar 07 2025 10:00:00 GMT+0100 (CET)", "Fri Mar 07 2025 10:00:00 GMT+0000 (Coordinated Universal Time)",
  "1 Jan 2025 00:00:00 PST", "1 Jan 2025 00:00:00 EST", "1 Jan 2025 00:00:00 UTC", "Jan 1 2025 12:00 AM", "Jan 1 2025 12:00 PM", "2025-03-07T10:00:00-0800", "2025-03-07T10:00:00.5", "2025-03-07T24:00:00",
  "2025-02-30", "2025-13-01", "2025-03-07T25:00:00", "10:00", "10:00 AM", "tomorrow", "now", "yesterday at noon", "", " 2025-03-07 ", "1700000000000", "1700000000", "0", "1", "99", "2025-W10", "+002025-03-07T00:00:00Z", "-000001-01-01T00:00:00Z",
  "3rd March 2025", "March 7th, 2025", "2025年3月7日", "7 de marzo de 2025", "Sat, 31 Feb 2025 00:00:00 GMT", "Thu, 01 Jan 1970 00:00:00 GMT", "12/31/1999 23:59:59", "1999-12-31T23:59:59.999999Z",
];
add("dates", "date-parse-strings", `async () => Object.fromEntries(${JSON.stringify(DATE_STRINGS)}.map((s) => { const t = new Date(s).getTime(); return [s, Number.isNaN(t) ? "Invalid Date" : new Date(t).toISOString()]; }))`);
add("dates", "date-parse-strings-ordinal-quarter", 'async () => Object.fromEntries(["2025-066", "Q1 2025"].map((s) => { const t = new Date(s).getTime(); return [s, Number.isNaN(t) ? "Invalid Date" : new Date(t).toISOString()]; }))');
add("dates", "date-parse-function", 'async () => [Date.parse("2025-03-07T10:00:00Z"), Date.parse("March 7, 2025"), Date.parse("nope"), Date.UTC(2025, 2, 7), new Date(2025, 2, 7).getTime(), new Date("2025-03-07").getTime() === new Date("2025-03-07T00:00:00").getTime()]');
add("dates", "date-arithmetic", 'async () => { const start = new Date("2025-01-31T00:00:00Z"); const plusMonth = new Date(start); plusMonth.setUTCMonth(plusMonth.getUTCMonth() + 1); const days = (new Date("2025-03-01") - new Date("2025-02-01")) / 86400000; return [plusMonth.toISOString(), days, new Date(Date.UTC(2024, 1, 29)).toISOString().slice(0, 10), new Date(2025, 0, 1) < new Date(2025, 0, 2)]; }');
add("dates", "date-now-shape", 'async () => { const now = Date.now(); const d = new Date(); return [typeof now, Number.isInteger(now), now > 1.7e12, Math.abs(d.getTime() - now) < 1000, /^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}\\.\\d{3}Z$/.test(d.toISOString()), typeof Date()]; }');
add("dates", "performance-now", 'async () => { const a = performance.now(); return [typeof a, a >= 0, Number.isFinite(a), typeof performance.timeOrigin, typeof performance.mark, typeof performance.measure, Object.prototype.toString.call(performance)]; }');
add("dates", "temporal", 'async () => typeof Temporal === "object" ? [Temporal.PlainDate.from("2025-03-07").add({ days: 30 }).toString(), Temporal.Instant.fromEpochMilliseconds(0).toString()] : "no Temporal"');

add("language", "language-features", 'async () => ({ replaceAll: "a-b-c".replaceAll("-", "+"), at: [1, 2, 3].at(-1), groupBy: typeof Object.groupBy, mapGroupBy: typeof Map.groupBy, toSorted: typeof [].toSorted, toReversed: typeof [].toReversed, with: typeof [].with, findLast: [1, 2, 3].findLast((x) => x < 3), hasOwn: Object.hasOwn({ a: 1 }, "a"), fromAsync: typeof Array.fromAsync, withResolvers: typeof Promise.withResolvers, promiseTry: typeof Promise.try, setUnion: typeof Set.prototype.union, iteratorMap: typeof Iterator !== "undefined" && typeof Iterator.prototype.map, isWellFormed: typeof "".isWellFormed, errorIsError: typeof Error.isError, regexpEscape: typeof RegExp.escape, sumPrecise: typeof Math.sumPrecise, rawJSON: typeof JSON.rawJSON, arrayBufferTransfer: typeof ArrayBuffer.prototype.transfer, resizable: typeof ArrayBuffer.prototype.resize, weakRef: typeof WeakRef, float16: typeof Math.f16round, structuredClone: typeof structuredClone, stringAt: "abc".at(-1), trimEnd: " a ".trimEnd(), flat: [[1], [[2]]].flat(Infinity), fromEntries: Object.fromEntries([["a", 1]]), allSettled: typeof Promise.allSettled, any: typeof Promise.any, matchAll: [..."a1b2".matchAll(/\\d/g)].map((m) => m[0]), bigint: typeof 1n, nullish: null ?? "d", optional: undefined?.x, logicalAssign: (() => { let a = null; a ??= 5; return a; })(), numericSeparator: 1_000, classFields: (() => { class A { #p = 1; static s = 2; get p() { return this.#p; } } return [new A().p, A.s]; })(), topLevelRegexFlags: [/a/d.flags, /a/s.flags, /\\p{L}/u.test("é"), /[\\p{L}--[a-z]]/v.test("é")], lookbehind: /(?<=\\$)\\d+/.exec("$42")[0], namedGroups: /(?<y>\\d{4})/.exec("2025").groups.y })');
add("language", "language-syntax-using", "async () => { const log = []; { using r = { [Symbol.dispose]() { log.push(\"disposed\"); } }; log.push(\"used\"); } return log; }");
add("language", "language-syntax-regexp-modifiers", 'async () => /(?i:a)b/.test("Ab")');
add("language", "language-syntax-decorators", "async () => { const d = (v) => v; class A { @d m() { return 1; } } return new A().m(); }");
add("language", "language-syntax-import-defer", 'async () => typeof import.meta');
add("language", "language-number-formatting", "async () => [(0.1).toFixed(20), (1e21).toString(), (255).toString(16), (0.000001234).toExponential(2), (123.456).toPrecision(4), (1.005).toFixed(2), (25).toString(36), (-1.5).toFixed(0), (2.5).toFixed(0), (1e-7).toString(), (123456789.123456789).toString(), parseFloat(\"1e-7\"), Number(\"0x1f\"), Number(\"\"), Number(\" 12 \"), Number(\"1_000\"), parseInt(\"08\"), (0.1 + 0.7) * 10, 9007199254740992 + 1, 5e-324, Number.MAX_SAFE_INTEGER, Number.EPSILON > 0, Math.round(2.5), Math.round(-2.5), Math.round(0.49999999999999994)]");
add("language", "language-math", "async () => [Math.max(), Math.min(), Math.hypot(3, 4), Math.sign(-3), Math.trunc(-4.7), Math.cbrt(27), Math.log2(8), Math.fround(5.5), Math.clz32(1), Math.expm1(0), Math.atan2(1, 1).toFixed(10), Math.sin(1).toFixed(12), Math.pow(2, 0.5).toFixed(12), Math.exp(1).toFixed(12), 2 ** 53, 7 % -3, -7 % 3, 0.1 * 3, 1 / 3]");
add("language", "language-string-methods", 'async () => ["é".normalize("NFD").length, "\\u0065\\u0301".normalize("NFC").length, "abc".padStart(6, "12"), "İstanbul".toLowerCase().length, "ß".toUpperCase(), "\\u{1F600}".length, [..."\\u{1F600}"].length, "a\\u0301".localeCompare ? "has localeCompare" : "", "x".codePointAt(0), String.fromCodePoint(128512), "aXbxc".split(/x/i), "2025-03-07".split("-").map(Number), " \\u00a0\\ufeff x ".trim(), "abc".substr(1, 1), "Hello".at(0), "a,b".split(",", 1), "ǅ".toLowerCase(), "ﬁ".toUpperCase()]');
add("language", "language-sort", 'async () => [[10, 9, 1, 100].sort(), [10, 9, 1, 100].sort((a, b) => a - b), ["b", "a", "C", "ä", "Z", "é", "e"].sort(), [3, 1, 2].toSorted ? [3, 1, 2].toSorted() : null, [{ k: 1, v: "a" }, { k: 0, v: "b" }, { k: 1, v: "c" }, { k: 0, v: "d" }].sort((x, y) => x.k - y.k).map((x) => x.v).join(""), [undefined, 3, null, 1].sort()]');
add("language", "language-regexp", 'async () => ["2025-03-07".replace(/(\\d+)-(\\d+)-(\\d+)/, "$3/$2/$1"), "aBc".replace(/b/i, (m) => m.toUpperCase() + "!"), /^[\\w.+-]+@[\\w-]+\\.[\\w.]+$/.test("a.b+c@example.co.uk"), "a1b22c333".match(/\\d+/g), "x".match(/y/), /\\bfoo\\b/.test("a foo b"), "ǅ".match(/\\p{Lt}/u) !== null, "été".match(/\\w+/g), "été".match(/[\\p{L}]+/gu), /(a+)+b/.test("aaaaaaaaaaaaaaaaaaaaaaaac") === false, new RegExp("a.c", "s").test("a\\nc"), "aaa".lastIndexOf("a"), /a/y.sticky, RegExp.prototype.flags !== undefined]');
add("language", "language-iteration", 'async () => { const m = new Map([["a", 1], ["b", 2]]); const s = new Set([3, 3, 4]); function* gen() { yield* [1, 2]; return 3; } const it = gen(); return { entries: [...m], keys: [...m.keys()], set: [...s], objectEntries: Object.entries({ x: 1, y: [2] }), gen: [...gen()], manual: [it.next(), it.next(), it.next(), it.next()], forIn: (() => { const k = []; for (const key in { a: 1, b: 2 }) k.push(key); return k; })(), arrayFrom: Array.from({ length: 3 }, (_, i) => i * i), spreadString: [..."héy"], asyncGen: await (async () => { const out = []; for await (const v of (async function* () { yield 1; yield 2; })()) out.push(v); return out; })() }; }');
add("language", "language-labels-and-switch", 'async () => { const kind = (v) => { switch (typeof v) { case "number": return "n"; case "string": return "s"; default: return "?"; } }; return [kind(1), kind("a"), kind(null), [1, 2, 3].includes(2), 0.1 + 0.2 === 0.3, [] + [], [] + {}, 1 + "2", "3" * "4", null + 1, undefined + 1, [2] * [3], typeof null, typeof [], void 0]; }');
add("language", "language-getters-proxies-symbols", 'async () => { const calls = []; const p = new Proxy({}, { get: (t, k) => (calls.push(String(k)), 1) }); const sum = p.a + p.b; const o = { [Symbol.toStringTag]: "Custom" }; return [sum, calls, String(o), Object.prototype.toString.call(null), Object.prototype.toString.call([]), Symbol("d").description, Symbol.for("k") === Symbol.for("k"), Reflect.ownKeys({ a: 1, [Symbol.iterator]: 2 }).length, typeof Symbol.asyncDispose, typeof Symbol.dispose]; }');
add("language", "language-typed-arrays", 'async () => { const f = new Float32Array([1.1, 2.5]); const dv = new DataView(new ArrayBuffer(8)); dv.setFloat64(0, Math.PI); return [Array.from(f), new Uint8Array(new Uint16Array([258]).buffer)[0], dv.getUint8(0), new Uint8ClampedArray([300, -5])[0], new BigInt64Array(1).length, Array.from(new Uint8Array([3, 1, 2]).sort()), new Int8Array([200])[0], typeof Float16Array, new Uint8Array(4).fill(7).join(""), new Uint8Array([1, 2, 3]).subarray(1).length]; }');
add("language", "language-weakref-finalization", 'async () => [typeof WeakRef, typeof FinalizationRegistry, typeof WeakRef === "function" ? typeof new WeakRef({}).deref() : null]');
add("language", "language-error-stack-header", 'async () => { const e = new Error("x"); const t = (() => { try { null.x; } catch (err) { return err; } })(); class Custom extends Error { constructor(m) { super(m); this.name = "Custom"; } } return [typeof e.stack, String(e.stack).split("\\n")[0], String(t.stack).split("\\n")[0].startsWith("TypeError: "), String(new Custom("c").stack).split("\\n")[0], String(e.stack).split("\\n").length > 1]; }');
add("language", "language-error-stack-api", 'async () => [typeof Error.captureStackTrace, typeof Error.stackTraceLimit, "prepareStackTrace" in Error, Object.getOwnPropertyNames(new Error("m")).sort()]');
add("language", "language-error-stack-text", 'async () => { function inner() { throw new Error("deep"); } const outer = () => inner(); try { outer(); } catch (e) { return e.stack; } }');
add("language", "language-error-stack-tool", 'async () => { try { await mock.fail({ reason: "x" }); } catch (e) { return e.stack; } }');
add("language", "language-error-stack-engine", 'async () => { const rows = undefined; try { return rows.map((r) => r.id); } catch (e) { return e.stack; } }');
add("language", "language-error-stack-logged", 'async () => { try { JSON.parse("{bad"); } catch (e) { console.error(e); console.log(e.stack.split("\\n")[0]); } }');
add("language", "language-async-ordering", 'async () => { const order = []; const p = (async () => { order.push(1); await null; order.push(3); })(); order.push(2); await p; await Promise.resolve().then(() => order.push(4)); const r = await Promise.race([new Promise((r) => setTimeout(() => r("timer"), 0)), Promise.resolve("micro")]); order.push(r); return order; }');
add("language", "language-json-roundtrip-order", 'async () => JSON.stringify(JSON.parse(\'{"b":1,"a":2,"1":3,"__proto__":{"x":1},"constructor":4}\'))');
add("language", "language-array-holes-and-length", "async () => { const a = [1, 2, 3]; a.length = 1; const b = []; b[3] = 1; return [a, b.length, Object.keys(b), Array(3).fill().map((_, i) => i), Array.isArray(Array.prototype), [,].length, [1, 2, 3].indexOf(4)]; }");
add("language", "language-object-protos", 'async () => { const o = {}; const hasProto = "__proto__" in o; o.__proto__ = { inherited: 1 }; return [hasProto, o.inherited, Object.keys(o), typeof Object.prototype.__defineGetter__, ({}).constructor === Object, Object.getPrototypeOf(async () => {}).constructor.name, (async () => {}).constructor.name, (function* () {}).constructor.name]; }');
add("language", "language-function-tostring", 'async () => [(() => 1).toString(), (function named(a, b) { return a + b; }).toString(), String(class A { m() {} }), (async (x) => x).toString(), Math.max.toString(), (() => {}).name, (function () {}).name, ((a, b = 1, ...c) => {}).length]');

// ---------------------------------------------------------------- the runtime's platform SDK
const R = (id, code, options = {}) => add("runtime", id, code, { ...RUNTIME, ...options });
R("runtime-search-no-match", 'async () => codemode.search("zzzz qqqq nothing")');
R("runtime-search-one-match", 'async () => codemode.search("chart")');
R("runtime-search-many-matches", 'async () => codemode.search("issue")');
R("runtime-search-all-of-a-connector", 'async () => codemode.search("tracker")');
R("runtime-search-phrase", 'async () => codemode.search("add two numbers")');
R("runtime-search-docs-example", 'async () => codemode.search("short intent phrase")');
R("runtime-search-partial-words", 'async () => [await codemode.search("iss"), await codemode.search("look up a term"), await codemode.search("waits seconds answering")].map((r) => r.results.map((x) => [x.path, x.score]))');
R("runtime-search-forms", 'async () => Object.fromEntries(await Promise.all(["listIssues", "list issues", "list_issues", "list-issues", "tracker.get_issue", "get", "delete", "DELETE", "mock", "create an issue with labels", "numbers", "repeat text", "a", "the", "search issue titles", "fail error", "data"].map(async (q) => [q, (await codemode.search(q)).results.map((r) => r.path + ":" + r.score)])))');
R("runtime-search-empty-query", 'async () => codemode.search("")');
R("runtime-search-no-argument", "async () => codemode.search()");
R("runtime-search-non-string", "async () => [await codemode.search(42), await codemode.search({ query: \"add\" }), await codemode.search(null), await codemode.search([\"add\", \"numbers\"])]");
R("runtime-search-punctuation", 'async () => [await codemode.search("add!!! (numbers)"), await codemode.search("   "), await codemode.search("?"), await codemode.search("ADD")].map((r) => [r.total, r.results.map((x) => x.path)])');
R("runtime-search-result-shape", 'async () => { const r = await codemode.search("lookup"); return { keys: Object.keys(r), first: Object.keys(r.results[0]), types: Object.fromEntries(Object.entries(r.results[0]).map(([k, v]) => [k, typeof v])) }; }');
R("runtime-describe-method", 'async () => codemode.describe("mock.add")');
R("runtime-describe-method-with-descriptions", 'async () => codemode.describe("mock.lookup")');
R("runtime-describe-method-output-schema", 'async () => codemode.describe("tracker.list_issues")');
R("runtime-describe-method-sanitized-name", 'async () => codemode.describe("tracker.delete_")');
R("runtime-describe-method-empty-input", 'async () => codemode.describe("mock.chart")');
R("runtime-describe-connector", 'async () => codemode.describe("mock")');
R("runtime-describe-connector-with-instructions", 'async () => codemode.describe("tracker")');
R("runtime-describe-unknown", 'async () => codemode.describe("github.list_pull_requests")');
R("runtime-describe-unknown-method", 'async () => codemode.describe("mock.nope")');
R("runtime-describe-unknown-bare", 'async () => codemode.describe("nothing")');
R("runtime-describe-bare-method", 'async () => [await codemode.describe("add"), await codemode.describe("search"), await codemode.describe("get_issue")]');
R("runtime-describe-unsanitized", 'async () => [await codemode.describe("tracker.list-issues"), await codemode.describe("tracker.delete")]');
R("runtime-describe-odd-targets", 'async () => [await codemode.describe(""), await codemode.describe("mock."), await codemode.describe(".add"), await codemode.describe("mock.add.extra"), await codemode.describe("codemode"), await codemode.describe("codemode.search"), await codemode.describe("MOCK"), await codemode.describe(" mock ")]');
R("runtime-describe-no-argument", "async () => codemode.describe()");
R("runtime-describe-non-string", "async () => [await codemode.describe(5), await codemode.describe({ path: \"mock.add\" }), await codemode.describe(null)]");
R("runtime-describe-search-result", 'async () => { const matches = await codemode.search("open issues"); return codemode.describe(matches.results[0]); }');
R("runtime-workflow", 'async () => {\n  const matches = await codemode.search("list issues");\n  const docs = await codemode.describe(matches.results[0].path);\n  const open = await tracker.list_issues({ state: "open", limit: 2 });\n  return { path: matches.results[0].path, docs, open };\n}');
R("runtime-workflow-logged", 'async () => {\n  const matches = await codemode.search("add numbers");\n  console.log(matches.results.map((m) => m.path).join(", "));\n  const docs = await codemode.describe(matches.results[0].path);\n  console.log(docs.types);\n}');
R("runtime-step", 'async () => { const id = await codemode.step("pick-a-number", () => 41 + 1); return id; }');
R("runtime-step-async", 'async () => codemode.step("lookup-inside", async () => { const r = await mock.add({ a: 20, b: 22 }); return { r }; })');
R("runtime-step-random", 'async () => { const id = await codemode.step("gen-id", () => crypto.randomUUID()); const again = await codemode.step("gen-id", () => crypto.randomUUID()); return [typeof id, id.length, id !== again]; }');
R("runtime-step-time", 'async () => { const t = await codemode.step("now", () => Date.now()); return typeof t; }');
R("runtime-step-undefined", 'async () => { const v = await codemode.step("nothing", () => {}); return [typeof v, v === undefined]; }');
R("runtime-step-throws", 'async () => codemode.step("boom", () => { throw new Error("inside the step"); })');
R("runtime-step-throws-caught", 'async () => { try { await codemode.step("boom", async () => { throw new TypeError("inside"); }); } catch (e) { return [e.name, e.message]; } }');
R("runtime-step-no-function", 'async () => codemode.step("no-fn")');
R("runtime-step-no-name", "async () => codemode.step(() => 5)");
R("runtime-step-no-arguments", "async () => codemode.step()");
R("runtime-step-returns-values", 'async () => [await codemode.step("date", () => new Date(0)), await codemode.step("map", () => new Map([[1, 2]])), await codemode.step("big", () => 10n).then(String, (e) => "threw " + e.message), await codemode.step("fn", () => () => 1).then((v) => typeof v), await codemode.step("nan", () => NaN), await codemode.step("nested", () => ({ a: [1, { b: undefined }] }))]');
R("runtime-step-nested", 'async () => codemode.step("outer", () => codemode.step("inner", () => "innermost"))');
R("runtime-step-parallel", 'async () => Promise.all([codemode.step("a", () => 1), codemode.step("b", async () => 2), mock.add({ a: 1, b: 2 })])');
R("runtime-step-fetch", 'async () => codemode.step("fetch", async () => (await fetch("https://example.com/")).status)');
R("runtime-step-large-value", 'async () => { const v = await codemode.step("large", () => "x".repeat(1200000)); return v.length; }');
R("runtime-step-uncloneable-value", 'async () => { const v = await codemode.step("cycle", () => { const a = {}; a.a = a; return a; }); return typeof v; }');
R("runtime-run-no-snippet", 'async () => codemode.run("nightly-report")');
R("runtime-run-with-input", 'async () => codemode.run("summarize", { limit: 5 })');
R("runtime-run-no-argument", "async () => codemode.run()");
R("runtime-run-result-used", 'async () => { const r = await codemode.run("weekly"); if (r.error) { console.log("snippet missing:", r.error); return (await tracker.list_issues({ limit: 1 })).total; } return r; }');
R("runtime-platform-shape", 'async () => ({ keys: Object.keys(codemode), search: typeof codemode.search, describe: typeof codemode.describe, step: typeof codemode.step, run: typeof codemode.run, other: typeof codemode.anything, stepDecide: typeof codemode.__stepDecide, string: String(codemode.step).slice(0, 12), hasStep: Object.prototype.hasOwnProperty.call(codemode, "step") })');
R("runtime-platform-unknown-method", "async () => codemode.list()");
R("runtime-platform-unknown-method-caught", 'async () => { try { await codemode.tools(); } catch (e) { return e.message; } }');
R("runtime-platform-internal-methods", 'async () => { const d = await codemode.__stepDecide("manual"); return { kind: d.kind, seq: typeof d.seq, keys: Object.keys(d) }; }');
R("runtime-connector-shape", 'async () => ({ keys: Object.keys(mock), add: typeof mock.add, nope: typeof mock.nope, then: typeof mock.then, symbol: typeof mock[Symbol.iterator], own: Object.prototype.hasOwnProperty.call(mock, "add"), inOp: "add" in mock })');
R("runtime-connector-method-override", 'async () => { mock.add = async () => "overridden"; return mock.add({ a: 1, b: 2 }); }');
R("runtime-syntax-error-one-connector", "async () => { return 1 +; }", { servers: ["mock"] });
R("runtime-two-executions", null, { steps: [call('async () => { globalThis.kept = "from the first execution"; const first = await mock.add({ a: 1, b: 2 }); return { first, kept: typeof globalThis.kept }; }'), call('async () => ({ kept: typeof globalThis.kept, again: await mock.add({ a: 3, b: 4 }) })')] });
R("runtime-three-executions-discovery", null, { steps: [call('async () => codemode.search("issues")'), call('async () => codemode.describe("tracker.list_issues")'), call('async () => { const { issues, total } = await tracker.list_issues({ state: "closed", limit: 3 }); return { total, titles: issues.map((i) => i.title) }; }')] });
R("runtime-error-then-retry", null, { steps: [call('async () => tracker.listIssues({ state: "open" })'), call('async () => (await tracker.list_issues({ state: "open", limit: 1 })).issues[0].title')] });
R("runtime-two-calls-in-one-answer", null, { steps: [{ calls: [{ arguments: JSON.stringify({ code: 'async () => mock.add({ a: 1, b: 1 })' }) }, { arguments: JSON.stringify({ code: 'async () => mock.lookup({ query: "second call" })' }) }] }] });
R("runtime-reference-error-hint", 'async () => host.readFile("/etc/passwd")');
R("runtime-reference-error-hint-with-logs", 'async () => { console.log("about to read"); return fs.readFileSync("x"); }');
R("runtime-reference-error-caught", 'async () => { try { return missing; } catch (e) { return e.message; } }');
R("runtime-error-mentioning-not-defined", 'async () => { throw new Error("the field is not defined in the schema"); }');
R("runtime-tool-result-too-large", 'async () => { const text = await mock.repeat({ text: "0123456789", times: 100000 }); return text.length; }');
R("runtime-tool-result-too-large-continues", 'async () => { const text = await mock.repeat({ text: "0123456789", times: 100000 }); console.log("got", text.length); try { await mock.add({ a: 1, b: 1 }); return "second call ran"; } catch (e) { return "second call threw: " + e.message; } }');
R("runtime-tool-result-just-fits", 'async () => { const text = await mock.repeat({ text: "0123456789", times: 99999 }); return text.length; }');
R("runtime-tool-arguments-too-large", 'async () => { try { await mock.lookup({ query: "q".repeat(1000001) }); return "called"; } catch (e) { return "threw: " + e.message; } }');
R("runtime-final-result-too-large-for-the-log", 'async () => "0123456789".repeat(120000)');
R("runtime-one-server", "async () => mock.add({ a: 2, b: 2 })", { servers: ["mock"] });
R("runtime-one-server-reference-error", "async () => tracker.get_issue({ id: 1 })", { servers: ["mock"] });
R("runtime-one-server-search", 'async () => codemode.search("issue")', { servers: ["mock"] });
R("runtime-tracker-only-describe", 'async () => codemode.describe("tracker")', { servers: ["tracker"] });
R("runtime-connector-hints", "async () => mock.add({ a: 1, b: 1 })", { tool_options: { connectorHints: { mock: "Reference lookups and arithmetic.", tracker: "Issues of the mock project." } } });
R("runtime-connector-hint-partial", "async () => mock.add({ a: 1, b: 1 })", { tool_options: { connectorHints: { tracker: "Issues of the mock project." } } });
R("runtime-custom-description", "async () => mock.add({ a: 1, b: 1 })", { tool_options: { description: "Run JavaScript against the connectors." } });
R("runtime-host-search-describe", "async () => 1", { host_calls: [["search", "add numbers"], ["describe", "mock.add"], ["describe", "tracker"], ["search", "zzzz"]] });

// ---------------------------------------------------------------- inputs the tool does not accept
const INVALID = [
  ["input-empty-object", "{}"],
  ["input-code-number", '{"code": 5}'],
  ["input-code-null", '{"code": null}'],
  ["input-code-array", '{"code": ["async () => 1"]}'],
  ["input-code-object", '{"code": {"source": "async () => 1"}}'],
  ["input-wrong-key", '{"script": "async () => 1"}'],
  ["input-javascript-key", '{"javascript": "async () => 1", "code_": 1}'],
  ["input-string", '"async () => 1"'],
  ["input-array", '["async () => 1"]'],
  ["input-number", "42"],
  ["input-null", "null"],
  ["input-true", "true"],
  ["input-empty", ""],
  ["input-whitespace", "  "],
  ["input-truncated-json", '{"code": "async () => 1'],
  ["input-trailing-comma", '{"code": "async () => 1",}'],
  ["input-single-quotes", "{'code': 'async () => 1'}"],
  ["input-bare-code", "async () => 1"],
  ["input-two-objects", '{"code": "async () => 1"}{"code": "async () => 2"}'],
  ["input-unescaped-newline", '{"code": "async () => {\n  return 1;\n}"}'],
  ["input-python-dict", "{'code': \"async () => 1\"}"],
  ["input-xml", "<code>async () => 1</code>"],
];
for (const [id, text] of INVALID) raw("inputs", id, { arguments: text });
raw("inputs", "input-extra-key", { arguments: '{"code": "async () => 1", "explanation": "adds nothing"}' });
raw("inputs", "input-extra-key-first", { arguments: '{"language": "javascript", "code": "async () => 2"}' });
raw("inputs", "input-duplicate-key", { arguments: '{"code": "async () => 1", "code": "async () => 2"}' });
raw("inputs", "input-spaced-json", { arguments: '{ "code" : "async () => 3" }' });
raw("inputs", "input-pretty-json", { arguments: '{\n  "code": "async () => 4"\n}\n' });
raw("inputs", "input-unicode-escapes", { arguments: '{"code": "async () => \\"\\u00e9 \\u2603 \\ud83d\\ude00 \\/ \\u0041\\""}' });
raw("inputs", "input-raw-unicode", { arguments: '{"code": "async () => \\"é ☃ 😀\\""}' });
raw("inputs", "input-html-characters", { arguments: '{"code": "async () => \\"<b> & \' > \\u003c\\""}' });
raw("inputs", "input-leading-whitespace", { arguments: '  {"code": "async () => 5"}  ' });
raw("inputs", "input-empty-code", { arguments: '{"code": ""}' });
raw("inputs", "input-number-formats", { arguments: '{"code": "async () => 6", "n": 1.0, "e": 1e2, "big": 12345678901234567890}' });
raw("inputs", "unknown-tool", { steps: [{ calls: [{ name: "search", arguments: '{"query": "issues"}' }] }] });
raw("inputs", "unknown-tool-with-code", { steps: [{ calls: [{ name: "execute_javascript", arguments: '{"code": "async () => 1"}' }] }] });
raw("inputs", "unknown-tool-namespace-method", { steps: [{ calls: [{ name: "mock.add", arguments: '{"a": 1, "b": 2}' }] }] });
raw("inputs", "unknown-tool-then-code", { steps: [{ calls: [{ name: "add", arguments: '{"a": 1, "b": 2}' }] }, call("async () => mock.add({ a: 1, b: 2 })")] });
raw("inputs", "unknown-tool-and-code-together", { steps: [{ calls: [{ name: "nope", arguments: "{}" }, { arguments: JSON.stringify({ code: "async () => 7" }) }] }] });
raw("inputs", "unknown-tool-invalid-arguments", { steps: [{ calls: [{ name: "nope", arguments: "{not json" }] }] });
raw("inputs", "tool-name-case", { steps: [{ calls: [{ name: "Codemode", arguments: '{"code": "async () => 1"}' }] }] });

// ---------------------------------------------------------------- the conversation around the call
raw("conversation", "assistant-text-with-call", { steps: [{ content: "I'll add them in code.", calls: [{ arguments: JSON.stringify({ code: "async () => mock.add({ a: 4, b: 5 })" }) }] }] });
raw("conversation", "assistant-empty-text-with-call", { steps: [{ content: "", calls: [{ arguments: JSON.stringify({ code: "async () => 1" }) }] }] });
raw("conversation", "assistant-whitespace-text-with-call", { steps: [{ content: "\n\n", calls: [{ arguments: JSON.stringify({ code: "async () => 1" }) }] }] });
raw("conversation", "assistant-reasoning-with-call", { steps: [{ reasoning_content: "The user wants a sum; one call is enough.", calls: [{ arguments: JSON.stringify({ code: "async () => mock.add({ a: 4, b: 5 })" }) }] }] });
raw("conversation", "assistant-reasoning-and-text-with-call", { steps: [{ reasoning_content: "Think first.", content: "Running the code now.", calls: [{ arguments: JSON.stringify({ code: "async () => 2" }) }] }] });
raw("conversation", "two-code-calls-in-one-answer", { steps: [{ calls: [{ arguments: JSON.stringify({ code: "async () => mock.add({ a: 1, b: 1 })" }) }, { arguments: JSON.stringify({ code: 'async () => { console.log("second"); return mock.lookup({ query: "second call" }); }' }) }] }] });
raw("conversation", "three-rounds", { steps: [call("async () => mock.add({ a: 1, b: 2 })"), call('async () => { throw new Error("second round fails"); }'), call('async () => { console.log("third"); return tracker.get_issue({ id: 3 }); }')] });
raw("conversation", "custom-call-id", { steps: [{ calls: [{ id: "chatcmpl-tool-9f8e7d6c5b4a", arguments: JSON.stringify({ code: "async () => 1" }) }] }] });
raw("conversation", "stream-result", { driver: "stream", code: "async () => {\n  const s = await mock.add({ a: 4, b: 5 });\n  console.log(\"sum\", s);\n  return s;\n}" });
raw("conversation", "stream-error", { driver: "stream", code: 'async () => { console.log("one"); throw new Error("boom"); }' });
raw("conversation", "stream-syntax-error", { driver: "stream", code: "async () => { return 1 +; }" });
raw("conversation", "stream-invalid-input", { driver: "stream", arguments: '{"code": 5}' });
raw("conversation", "stream-invalid-json", { driver: "stream", arguments: '{"code": "async () => 1' });
raw("conversation", "stream-unknown-tool", { driver: "stream", steps: [{ calls: [{ name: "search", arguments: '{"query": "issues"}' }] }] });
raw("conversation", "stream-text-with-call", { driver: "stream", steps: [{ content: "Let me check.", reasoning_content: "Plan.", calls: [{ arguments: JSON.stringify({ code: 'async () => tracker.search({ query: "theme" })' }) }] }] });
raw("conversation", "stream-two-rounds", { driver: "stream", steps: [call("async () => 1"), call("async () => undefined")] });
raw("conversation", "stream-spaced-arguments", { driver: "stream", arguments: '{ "code" : "async () => 3", "note": "x" }' });
raw("conversation", "code-tool-custom-description", { formats: ["code_tool"], description: "Run JavaScript.\n\n{{types}}\n\nReturn the result.", code: "async () => mock.add({ a: 1, b: 1 })", servers: ["mock"] });

// ---------------------------------------------------------------- the docs' non-durable MCP example
raw("ai_tools", "ai-tools-names", { formats: ["ai_tools"], code: "async () => { const sum = await codemode.tool_mock_add({ a: 4, b: 5 }); return sum; }" });
raw("ai_tools", "ai-tools-structured", { formats: ["ai_tools"], code: "async () => codemode.tool_tracker_list_issues({ limit: 1 })" });
raw("ai_tools", "ai-tools-error", { formats: ["ai_tools"], code: 'async () => codemode.tool_mock_fail({ reason: "x" })' });
raw("ai_tools", "ai-tools-invalid-arguments", { formats: ["ai_tools"], code: 'async () => codemode.tool_mock_add({ a: "four" })' });

for (const c of cases) if (c.code === null) delete c.code;
const ids = new Set();
for (const c of cases) {
  if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
  ids.add(c.id);
}
export const CASES = cases;
