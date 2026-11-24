// Runs the reference corpus (corpus.mjs) through the published @cloudflare/codemode on workerd and
// writes the fixtures Stellarator's conformance test reads (../fixtures/*.json).
//
//   npm ci
//   cargo build -p stellarator-mock
//   node reference/run.mjs --mock ../../../../target/debug/stellarator-mock
//
// Options: --mock <path to stellarator-mock> (required), --only <substring of a case id>,
// --format code_tool|runtime|ai_tools, --port 8180 (workerd; the stub endpoint takes port + 1 and
// the mock port + 2), --out <folder> (default ../fixtures), --keep (leave the work folder).
//
// What runs: workerd serves worker.mjs (bundled by build.mjs). For each case this script posts the
// setup to the Worker, which builds the code tool and calls the AI SDK's generateText (or
// streamText) against the stub model endpoint served here. The stub answers each request with the
// case's next scripted tool call and records the request body, so a fixture holds exactly what an
// OpenAI-compatible model server receives: the `tools` array and the messages after the prompt.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { bundle, writeConfig } from "./build.mjs";
import { CASES, FORMATS, SERVERS } from "./corpus.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
const flag = (name) => args.includes(`--${name}`);
const port = Number(opt("port", "8180"));
const stubPort = port + 1;
const mockPort = port + 2;
const mockBin = opt("mock");
const only = opt("only");
const onlyFormat = opt("format");
const outDir = resolve(opt("out", join(root, "fixtures")));
if (!mockBin) {
  console.error("Pass --mock <path to the stellarator-mock binary> (cargo build -p stellarator-mock).");
  process.exit(2);
}

/** Tool message contents longer than this are stored as a hash, a length and both ends. */
const INLINE_LIMIT = 40000;
/** A case that has not answered after this long is recorded as hanging, and workerd restarts. */
const HANG_MS = 15000;

const version = (name) => JSON.parse(readFileSync(join(root, "node_modules", name, "package.json"), "utf8")).version;
const versions = Object.fromEntries(
  ["@cloudflare/codemode", "workerd", "ai", "@ai-sdk/openai-compatible", "agents", "@modelcontextprotocol/sdk", "zod", "acorn"].map((n) => [n, version(n)]),
);

// ------------------------------------------------------------------ the stub model endpoint
const scripts = new Map();
const stub = createServer((req, res) => {
  let raw = "";
  req.on("data", (c) => (raw += c));
  req.on("end", () => {
    if (req.method !== "POST" || !req.url.endsWith("/chat/completions")) {
      res.writeHead(404).end();
      return;
    }
    const body = JSON.parse(raw);
    const script = scripts.get(body.model);
    if (!script) {
      res.writeHead(400, { "content-type": "application/json" }).end(JSON.stringify({ error: { message: `no script for ${body.model}` } }));
      return;
    }
    script.requests.push(body);
    const answered = body.messages.filter((m) => m.role === "assistant").length;
    const step = script.steps[answered];
    const message = step
      ? {
          role: "assistant",
          content: step.content ?? null,
          ...(step.reasoning_content ? { reasoning_content: step.reasoning_content } : {}),
          tool_calls: step.calls.map((c, i) => ({
            id: c.id ?? `call_${answered + 1}${step.calls.length > 1 ? `_${i + 1}` : ""}`,
            type: "function",
            function: { name: c.name ?? "codemode", arguments: c.arguments },
          })),
        }
      : { role: "assistant", content: "Done." };
    const finish = step ? "tool_calls" : "stop";
    const base = { id: "chatcmpl-stub", created: 0, model: body.model };
    const usage = { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 };
    if (body.stream) {
      res.writeHead(200, { "content-type": "text/event-stream" });
      const send = (delta, finish_reason = null, extra = {}) =>
        res.write(`data: ${JSON.stringify({ ...base, object: "chat.completion.chunk", choices: [{ index: 0, delta, finish_reason }], ...extra })}\n\n`);
      send({ role: "assistant", content: "" });
      if (message.reasoning_content) send({ reasoning_content: message.reasoning_content });
      if (message.content) send({ content: message.content });
      for (const [index, call] of (message.tool_calls ?? []).entries()) {
        send({ tool_calls: [{ index, id: call.id, type: "function", function: { name: call.function.name, arguments: "" } }] });
        const text = call.function.arguments;
        // The arguments arrive in pieces, as a model streams them.
        for (let at = 0; at < text.length; at += 17) send({ tool_calls: [{ index, function: { arguments: text.slice(at, at + 17) } }] });
      }
      send({}, finish, { usage });
      res.end("data: [DONE]\n\n");
      return;
    }
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ ...base, object: "chat.completion", choices: [{ index: 0, message, finish_reason: finish }], usage }));
  });
});
await new Promise((ok) => stub.listen(stubPort, "127.0.0.1", ok));

// ------------------------------------------------------------------ the processes
const work = mkdtempSync(join(tmpdir(), "codemode-reference-"));
const { bytes } = await bundle(work);
writeConfig(work, port);
console.error(`bundled the Worker: ${bytes} bytes`);

const children = [];
function start(command, argv, cwd) {
  const child = spawn(command, argv, { cwd, stdio: ["ignore", "pipe", "pipe"] });
  child.log = "";
  child.stdout.on("data", (d) => (child.log += d));
  child.stderr.on("data", (d) => (child.log += d));
  children.push(child);
  return child;
}
async function waitFor(url, what, child) {
  for (let i = 0; i < 150; i++) {
    try {
      await fetch(url);
      return;
    } catch {
      await new Promise((r) => setTimeout(r, 100));
    }
  }
  stop();
  throw new Error(`${what} did not start:\n${child.log}`);
}
const mock = start(resolve(mockBin), ["--host", "127.0.0.1", "--port", String(mockPort)]);
await waitFor(`http://127.0.0.1:${mockPort}/v1/models`, "stellarator-mock", mock);
const workerdBin = join(root, "node_modules", ".bin", "workerd");
let workerd;
async function startWorkerd() {
  // --experimental: Worker Loader bindings need it in workerd.
  workerd = start(workerdBin, ["serve", "--experimental", "config.capnp"], work);
  await waitFor(`http://127.0.0.1:${port}/`, "workerd", workerd);
}
await startWorkerd();
function stop() {
  for (const c of children) if (c.exitCode === null) c.kill();
  stub.close();
  if (!flag("keep")) rmSync(work, { recursive: true, force: true });
}
// Whatever ends this script, the mock and workerd end with it.
process.on("exit", () => {
  for (const c of children) if (c.exitCode === null) c.kill();
});
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => process.exit(1));

// ------------------------------------------------------------------ the cases
const sha256 = (text) => createHash("sha256").update(text, "utf8").digest("hex");
/** The runtime's execution ids differ on every run: a long content is hashed with them written as `exec_<id>`. */
const maskIds = (text) => text.replace(/exec_\d{16}_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g, "exec_<id>");
function stored(message) {
  if (message.role !== "tool" || typeof message.content !== "string" || message.content.length <= INLINE_LIMIT) return message;
  const { content, ...rest } = message;
  return { ...rest, content_sha256: sha256(maskIds(content)), content_length: content.length, content_head: content.slice(0, 300), content_tail: content.slice(-300) };
}

/** The request body for the Worker, and the key the tool definitions are stored under. */
function setupOf(format, c) {
  const servers = (c.servers ?? ["mock", "tracker"]).map((name) => ({ name, url: `http://127.0.0.1:${mockPort}${SERVERS[name].path}` }));
  const setup = {
    servers: servers.map((s) => s.name),
    ...(c.namespaces ? { namespaces: c.namespaces } : {}),
    ...(c.timeout_ms ? { timeout_ms: c.timeout_ms } : {}),
    ...(c.transform_result ? { transform_result: c.transform_result } : {}),
    ...(c.tool_options ? { tool_options: c.tool_options } : {}),
    ...(c.description ? { description: c.description } : {}),
    ...(c.driver ? { driver: c.driver } : {}),
  };
  // The tools array depends on these only.
  const definitionKey = [setup.servers.join("+"), setup.namespaces, setup.tool_options && JSON.stringify(setup.tool_options), setup.description && JSON.stringify(setup.description)]
    .filter(Boolean)
    .join(" ");
  return { setup, definitionKey, body: { ...setup, setup: format, servers, stub: `http://127.0.0.1:${stubPort}/v1` } };
}

async function runCase(format, c) {
  const key = `${format}:${c.id}`;
  const steps = c.steps ?? [{ calls: [{ arguments: c.arguments ?? JSON.stringify({ code: c.code }) }] }];
  scripts.set(key, { steps, requests: [] });
  const { setup, definitionKey, body } = setupOf(format, c);
  const entry = { id: c.id, group: c.group, ...(c.note ? { note: c.note } : {}), setup, definitions: definitionKey, steps };
  let text;
  try {
    const res = await fetch(`http://127.0.0.1:${port}/?agent=${encodeURIComponent(key)}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ ...body, model: key, max_steps: steps.length + 1, ...(c.host_calls ? { host_calls: c.host_calls } : {}) }),
      signal: AbortSignal.timeout(c.hang_ms ?? HANG_MS),
    });
    text = await res.text();
  } catch (e) {
    // No answer: the Worker is stuck (code that never yields has no limit in workerd). Start over.
    entry.outcome = "hang";
    entry.detail = `no answer after ${c.hang_ms ?? HANG_MS} ms (${e.name})`;
    workerd.kill("SIGKILL");
    await new Promise((r) => setTimeout(r, 300));
    await startWorkerd();
    scripts.delete(key);
    return { entry };
  }
  let answer;
  try {
    answer = JSON.parse(text);
  } catch {
    answer = { failed: text.slice(0, 2000) };
  }
  const { requests } = scripts.get(key);
  scripts.delete(key);
  if (answer.failed) {
    entry.outcome = "failed";
    entry.detail = answer.failed;
    return { entry };
  }
  const last = requests[requests.length - 1];
  entry.messages = last.messages.slice(1).map(stored);
  if (answer.host) entry.host = answer.host;
  if (answer.executions) entry.executions = answer.executions;
  return { entry, tools: requests[0].tools, definitionKey, toolChoice: requests[0].tool_choice ?? null };
}

const summary = {};
try {
  for (const format of FORMATS) {
    if (onlyFormat && onlyFormat !== format) continue;
    const cases = CASES.filter((c) => (c.formats ?? ["code_tool", "runtime"]).includes(format)).filter((c) => !only || c.id.includes(only));
    const out = { source: versions, format, servers: SERVERS, definitions: {}, cases: [] };
    for (const c of cases) {
      const { entry, tools, definitionKey } = await runCase(format, c);
      if (tools) {
        const known = out.definitions[definitionKey];
        if (known && JSON.stringify(known) !== JSON.stringify(tools)) throw new Error(`${c.id}: the tools of "${definitionKey}" changed between cases`);
        out.definitions[definitionKey] = tools;
      }
      out.cases.push(entry);
      if (entry.outcome) console.error(`${format} ${c.id}: ${entry.outcome} ${entry.detail}`);
    }
    summary[format] = out.cases.length;
    if (only || flag("print")) {
      console.log(JSON.stringify(out, null, 1));
    } else {
      mkdirSync(outDir, { recursive: true });
      writeFileSync(join(outDir, `${format}.json`), JSON.stringify(out, null, 1) + "\n");
    }
  }
} finally {
  if (flag("logs")) console.error(workerd.log);
  stop();
}
console.error("cases", summary);
