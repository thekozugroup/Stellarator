# Golden files and the reference harness

Code mode is tested against Cloudflare's own package. This folder is a small Node project that runs
the published `@cloudflare/codemode` and writes the files the Rust tests read. Nothing here runs
in Stellarator itself, and `node_modules` is not committed.

## What is here

| File | What it holds | Written by | Read by |
| --- | --- | --- | --- |
| `cloudflare.json` | Outputs of the package's pure helpers in Node: `normalizeCode`, `sanitizeToolName`, the generated types and description, `truncateResult` | `generate.mjs` | `tests/golden.rs` |
| `v8-json.json` | 514 inputs with V8's `JSON.parse` error messages | `v8-json.mjs --write` | `tests/golden.rs` |
| `fixtures/code_tool.json` | 661 conversations through `createCodeTool` on workerd | `reference/run.mjs` | `crates/engine/tests/conformance.rs` |
| `fixtures/runtime.json` | 749 conversations through `createCodemodeRuntime(...).tool()` on workerd | `reference/run.mjs` | `crates/engine/tests/conformance.rs` |
| `fixtures/ai_tools.json` | 4 conversations through `createCodeTool({ tools: this.mcp.getAITools() })`, for reference: Stellarator has no such format | `reference/run.mjs` | nothing |
| `known-differences.json` | The cases where Stellarator's answer differs, grouped by cause, with one reason per cause | by hand | `crates/engine/tests/conformance.rs` |
| `../../js/cloudflare-runtime.js` | The pure parts of the package's executor and durable runtime | `extract-runtime.mjs` | the sandbox |

The scripts:

| Script | What it does |
| --- | --- |
| `generate.mjs` | Writes `cloudflare.json` (`npm run pure`) |
| `ai-sdk.mjs` | Prints what the AI SDK sends for a canned executor outcome; a quick look, no file |
| `v8-json.mjs` | Compares `js/v8-json.js` with the `JSON.parse` and `JSON.stringify` of the V8 in Node over generated inputs (200,144 parse inputs and 20,000 cyclic values by default, more with `node v8-json.mjs <rounds>`; exits 1 on a difference) |
| `extract-runtime.mjs` | Rebuilds `js/cloudflare-runtime.js` from the installed package; `--check` only compares |
| `reference/corpus.mjs` | The cases: the code a scripted model writes, per group |
| `reference/worker.mjs` | The Worker: an Agents SDK `Agent` that builds either setup and drives the AI SDK |
| `reference/build.mjs` | Bundles the Worker with esbuild and writes the workerd config |
| `reference/run.mjs` | Starts everything, runs the corpus, writes `fixtures/` |

## Versions

Pinned in `package.json` and `package-lock.json`; each fixture file repeats them under `source`.

| Package | Version |
| --- | --- |
| `@cloudflare/codemode` | 0.5.6 |
| `workerd` | 1.20261124.1 |
| `agents` | 0.34.0 |
| `ai` | 7.0.181 |
| `@ai-sdk/openai-compatible` | 3.0.94 |
| `@modelcontextprotocol/sdk` | 1.32.2 |
| `zod` | 4.6.7 |
| `esbuild` | 0.28.2 |

## Write the fixtures again

Needs Node 22 or later and a build of the mock. From this folder:

```bash
npm ci
```

```bash
cargo build -p stellarator-mock
```

```bash
node reference/run.mjs --mock ../../../../target/debug/stellarator-mock
```

A full run takes about 2 minutes. It uses ports 8180 (workerd), 8181 (the stub model endpoint) and
8182 (the mock's MCP servers); `--port` moves all three. `--only <text>` runs the cases whose id
contains the text, `--format code_tool|runtime|ai_tools` one format, `--out <folder>` writes
elsewhere.

Execution ids (`exec_<time>_<uuid>`) differ on every run; everything else in the fixtures is the
same from run to run. The conformance test masks the ids.

## What the reference run is

`run.mjs` starts three processes:

| Process | Role |
| --- | --- |
| `stellarator-mock` | Two MCP servers over streamable HTTP: `/mcp` (`lookup`, `add`, `fail`, `slow`, `chart`, `repeat`) and `/tracker/mcp` (`list-issues`, `get_issue`, `create_issue`, `search`, `delete`; it sends `instructions` and one tool has an output schema) |
| A stub model endpoint (in `run.mjs`) | Answers `POST /chat/completions` with the case's next scripted tool call and records each request body |
| `workerd serve --experimental config.capnp` | Runs the bundled Worker |

For each case the Worker connects to the MCP servers with the Agents SDK (`addMcpServer`), builds
the tool, and calls the AI SDK's `generateText` (or `streamText`) with the OpenAI-compatible
provider pointed at the stub. The fixture holds what the stub received: the `tools` array, and the
messages after the prompt (each assistant message as the AI SDK sent it back, each `tool` message).

| Setup | How the Worker builds the tool |
| --- | --- |
| `code_tool` | `createCodeTool({ tools: [{ name: "<server>", tools }, ...], executor })`, one provider per server. Each tool takes the MCP tool's JSON Schema and calls it through an `McpConnector`, so results are unwrapped as Cloudflare unwraps MCP results |
| `runtime` | `createCodemodeRuntime({ ctx, executor, connectors })` with one `McpConnector` per server, then `runtime.tool()` |
| `ai_tools` | `createCodeTool({ tools: this.mcp.getAITools(), executor })` |

The executor is `new DynamicWorkerExecutor({ loader: env.LOADER })` in every setup.

Standalone workerd runs all of it, the durable runtime included. What the config needs:

| Need | In `config.capnp` or on the command line |
| --- | --- |
| The Worker Loader binding (Dynamic Workers) | `(name = "LOADER", workerLoader = (id = "codemode"))`, and `workerd serve --experimental`: without the flag workerd refuses the binding |
| A SQLite-backed Durable Object for the Agent; the runtime runs as a facet of it | `durableObjectNamespaces = [(className = "Chat", uniqueKey = "...", enableSql = true)]` |
| Storage for it on disk; workerd's in-memory storage has no SQL | `durableObjectStorage = (localDisk = "storage")` with a `disk` service |
| The Worker reaches the MCP servers and the stub on localhost | `globalOutbound = "internet"` with `network = (allow = ["public", "private", "local"])` |
| `CodemodeRuntime` as a Durable Object class | `export { CodemodeRuntime }` from the Worker's entry module |
| One ES module | esbuild; `cloudflare:email` (wrangler and miniflare provide it, the Agents SDK imports it) is replaced by a stub, and bare Node module names (`fs`, `path`) are mapped to `node:` |

The bundle is 3.4 MB. The model's code runs in a Dynamic Worker the executor loads with
`globalOutbound: null`, as on Cloudflare.

Two things differ from Cloudflare's network. Standalone workerd has no CPU limit, so code that
never yields (`while (true) {}`) never ends: `run.mjs` gives up after the case's `hang_ms`, restarts
workerd and records `"outcome": "hang"`. And the clock moves during an execution, which it does not
in production.

## The conformance test

```bash
cargo test -p stellarator-engine --test conformance -- --nocapture
```

It runs every case of `fixtures/code_tool.json` and `fixtures/runtime.json` through Stellarator's
sandbox with the same MCP servers (the mock, in process) and compares the `tools` array, every
`tool` message and every assistant message sent back, byte for byte. It prints the counts per
format and fails when:

- a case differs and is not in `known-differences.json`;
- a case in `known-differences.json` matches (the list is out of date);
- the list names a case that does not exist.

`CONFORMANCE_ONLY=<text>` runs the cases whose id contains the text. `CONFORMANCE_REPORT=<file>`
writes every difference found as JSON (`format`, `case`, `cause`, `what`, `cloudflare`,
`stellarator`).

`known-differences.json` is edited by hand: `causes[]` with `id`, `title`, `reason`, and the case
ids under `code_tool` and `runtime`. A difference belongs there only when the sandbox cannot match
it; the table in `docs/tools.md` (How close it is to Cloudflare's) is written from this list.

## Moving to a new release of the package

1. Change the versions in `package.json`, run `npm install`, and update the chunk file names in
   `generate.mjs` and `extract-runtime.mjs`.
2. `npm run pure`, `node extract-runtime.mjs`, then diff `js/cloudflare-codemode.js` against the
   new `dist` files by hand (js/NOTICE.md lists what was changed in it).
3. Write the fixtures again and run the conformance test. Fix what can match; list what cannot.
4. Update `CODE_MODE_FORMAT` in `crates/api`, js/NOTICE.md and docs/tools.md.
