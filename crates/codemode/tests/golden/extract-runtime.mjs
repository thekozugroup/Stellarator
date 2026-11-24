// Builds ../../js/cloudflare-runtime.js from the published dist files of @cloudflare/codemode
// 0.5.6: the parts of the executor and of the durable runtime that are pure, copied as published,
// with the glue js/NOTICE.md lists.
//
//   npm ci
//   node extract-runtime.mjs          writes js/cloudflare-runtime.js
//   node extract-runtime.mjs --check  exits 1 when the committed file is not what this writes
//
// The chunk file names below are those of the 0.5.6 package.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "node_modules", "@cloudflare", "codemode", "dist");
const src = readFileSync(join(dist, "index.js"), "utf8");
const runCode = readFileSync(join(dist, "run-code-Q3Jh6Dcu.js"), "utf8");
const base = readFileSync(join(dist, "base-BqhlNCSH.js"), "utf8");
const target = join(here, "..", "..", "js", "cloudflare-runtime.js");

/** The text from `start` up to `end` (not included), with trailing whitespace removed. */
function between(start, end, text = src) {
  const a = text.indexOf(start);
  if (a < 0) throw new Error(`not in the package: ${start.slice(0, 60)}`);
  const b = text.indexOf(end, a + start.length);
  if (b < 0) throw new Error(`not in the package: ${end.slice(0, 60)}`);
  return text.slice(a, b).trimEnd();
}
const must = (text, part) => {
  if (!text.includes(part)) throw new Error(`the package no longer has: ${part.slice(0, 80)}`);
};

const out = [];
out.push(
  [
    "// Adapted from @cloudflare/codemode 0.5.6 (https://github.com/cloudflare/agents, packages/codemode,",
    "// commit 4116a8bdf97660c8e79a923b6aec140256282a5d), MIT License, Copyright (c) 2025 Cloudflare, Inc.",
    "// (see LICENSE-cloudflare). These are parts of the package's published dist/index.js (and",
    "// dist/run-code-*.js, dist/base-*.js), the ones that are pure: the value codec, the text of the module",
    "// DynamicWorkerExecutor loads, what the durable runtime checks before it records a value, the MCP",
    "// result unwrapping, a connector's default input schema, codemode.search and codemode.describe, the runtime tool's description and",
    "// input schema, and how a result becomes the model's output. Modified for QuickJS:",
    '// - no ES module syntax, and no imports from "cloudflare:workers": the classes that need Workers',
    "//   (DynamicWorkerExecutor, CodemodeRuntime, the connectors) are not here;",
    "// - the module text DynamicWorkerExecutor.execute() builds is returned by buildExecutorModule()",
    "//   instead of being loaded, with the method's own checks and strings unchanged;",
    "// - runPass() and createProxyTool() are split into the functions below that do not touch the",
    "//   Durable Object: runCodeOutcome(), runPassOutcome().",
    "// It runs after cloudflare-codemode.js, whose sanitizeToolName, generateTypesFromJsonSchema,",
    "// normalizeCode and truncateResult it uses. Everything else is as published.",
    "",
  ].join("\n"),
);

// ---- codec
out.push("//#region src/codec.ts");
out.push(between('const BINARY_TAG = "__codemode_binary_v1__";', "//#endregion"));
out.push("//#endregion");

// ---- executor
out.push("//#region src/executor.ts");
out.push(between('const CONNECTOR_CONTROL_KEY = "__codemode_control__";', "/**\n* Best-effort synchronous disposal"));
out.push(between("const SANDBOX_CODEC = String.raw`", 'const LATE_CALL = Symbol("codemode.lateCall");'));
out.push("const DEFAULT_DYNAMIC_WORKER_EXECUTION_TIMEOUT_MS = 6e4;");
out.push(
  [
    "/**",
    '* The module DynamicWorkerExecutor.execute() loads as "executor.js" for `normalized` code, or the',
    "* error the method returns for names it refuses: `{ module }` or `{ error }`. `providers` are",
    "* `{ name, prelude? }`, `connectors` are `{ name }`.",
    "*/",
    "function buildExecutorModule(normalized, timeoutMs, providers, connectors) {",
  ].join("\n"),
);
out.push(
  between("\t\tconst RESERVED_NAMES = /* @__PURE__ */ new Set([", "\t\tconst gate = new ExecutionGate();")
    .replaceAll("return {\n\t\t\t\tresult: void 0,\n\t\t\t\terror:", "return {\n\t\t\t\terror:")
    .replace("\t\tconst connectors = options?.connectors ?? [];\n", ""),
);
out.push("\t\treturn { module: executorModule };\n}");
out.push("//#endregion");

// ---- run-code
out.push("//#region src/run-code.ts");
const logCtx = 'const logCtx = executeResult.logs?.length ? `\\n\\nConsole output:\\n${executeResult.logs.join("\\n")}` : "";';
must(runCode, "Code execution failed: ${executeResult.error}${logCtx}");
must(runCode, logCtx);
out.push(
  [
    "/**",
    "* What runCode() does with the executor's answer `{ result, error?, logs? }`: `{ threw }` with the",
    "* message of the Error it throws, or `{ output }` with what it returns.",
    "*/",
    "function runCodeOutcome(executeResult) {",
    "\tif (executeResult.error) {",
    "\t\t" + logCtx,
    "\t\treturn { threw: `Code execution failed: ${executeResult.error}${logCtx}` };",
    "\t}",
    "\treturn { output: executeResult.logs?.length ? {",
    "\t\tresult: executeResult.result,",
    "\t\tlogs: executeResult.logs",
    "\t} : { result: executeResult.result } };",
    "}",
  ].join("\n"),
);
out.push("//#endregion");

// ---- runtime
out.push("//#region src/runtime.ts");
out.push(between("/** Connector name used for `codemode.step(name, fn)` log entries. */", "/** Default number of terminal executions"));
out.push(between("/**\n* Cap for any single serialized value stored in the durable log", "/** Default age after which a paused"));
out.push(between("/** Model-actionable error for a value too large to record durably. */", "/** Pending actions (awaiting approval) of one execution. */"));
out.push(between("/**\n* Serialize a value for a durable column.", "var CodemodeRuntime = class extends DurableObject {"));
out.push("//#endregion");

// ---- connector base
out.push("//#region src/connectors/base.ts");
out.push(between("function toolInputSchema(t) {", "/**\n* Base class for codemode connectors.", base));
out.push("//#endregion");

// ---- mcp connector
out.push("//#region src/connectors/mcp.ts");
out.push(between("function unwrapMcpResult(result) {", "/**\n* Connector backed by an MCP connection."));
out.push("//#endregion");

// ---- search, describe
out.push(between("//#region src/connectors/search.ts", "//#region src/proxy-tool.ts"));

// ---- proxy tool
out.push("//#region src/proxy-tool.ts");
out.push(between("const proxyJsonSchema = {", "const proxySchema = {"));
out.push(between("const STEP_PRELUDE = String.raw`", "function createCursor() {"));
out.push(between("function buildDescription(connectors, customDescription, connectorHints) {", "async function runPass("));
out.push(between("/**\n* A sandbox `ReferenceError` usually means the model invented a global", "/**\n* Apply the result transform, defending against a buggy transform"));
out.push(between("/**\n* Project a tool output for the model: drop the audit-only `calls` log", "function createProxyTool(options) {"));
out.push(
  [
    "/**",
    "* What runPass() returns for one pass, from what the Durable Object and the executor would have",
    "* given it: `execution` is the execution's record (`{ status, error? }`), `threw` the message of",
    "* the error runCode() threw (or null), `output` what it returned. `calls` (the audit log) is left",
    "* out: toModelOutput() drops it before the model sees the result. `transformResult` is synchronous",
    "* here (truncateResult, or none).",
    "*/",
    "function runPassOutcome(executionId, execution, threw, output, setup, transformResult) {",
    '\tif (execution?.status === "error") return {',
    '\t\tstatus: "error",',
    "\t\texecutionId,",
    '\t\terror: execution.error ?? "Codemode execution failed"',
    "\t};",
    "\tif (threw !== null && threw !== void 0) return {",
    '\t\tstatus: "error",',
    "\t\texecutionId,",
    "\t\terror: withGlobalsHint(threw, setup)",
    "\t};",
    "\tconst result = output?.result;",
    "\treturn {",
    '\t\tstatus: "completed",',
    "\t\texecutionId,",
    "\t\tresult: transformResult ? transformResult(result) : result,",
    "\t\tlogs: output?.logs",
    "\t};",
    "}",
  ].join("\n"),
);
out.push("//#endregion");

const text = out.join("\n") + "\n";
if (process.argv.includes("--check")) {
  const committed = readFileSync(target, "utf8");
  if (committed !== text) {
    console.error("js/cloudflare-runtime.js is not what this script writes from the installed package.");
    process.exit(1);
  }
  console.log(`js/cloudflare-runtime.js: ${text.split("\n").length - 1} lines, as extracted`);
} else {
  writeFileSync(target, text);
  console.log(`wrote js/cloudflare-runtime.js: ${text.split("\n").length - 1} lines`);
}
