# Third-party JavaScript in code mode

Code mode runs Cloudflare's own Code Mode JavaScript in QuickJS, so the formats a model sees come
from their code. This folder holds it, with its licenses.

| File | What it is | License |
| --- | --- | --- |
| `acorn.js` | acorn 8.19.0, `dist/acorn.js` as published on npm, unmodified. Cloudflare's `normalizeCode` parses with it. | MIT, `LICENSE-acorn` |
| `cloudflare-codemode.js` | Adapted from `@cloudflare/codemode` 0.5.6 (github.com/cloudflare/agents, `packages/codemode`, commit `4116a8bdf97660c8e79a923b6aec140256282a5d`): the published `dist/json-schema-types-*.js` (normalizeCode, sanitizeToolName, jsonSchemaToType, generateTypesFromJsonSchema), `dist/truncate-*.js` (truncateResult) and `DEFAULT_DESCRIPTION`. | MIT, Copyright (c) 2025 Cloudflare, Inc., `LICENSE-cloudflare` |
| `cloudflare-runtime.js` | Adapted from the same release: parts of the published `dist/index.js`, `dist/run-code-*.js` and `dist/base-*.js`. The value codec; the text of the `executor.js` module `DynamicWorkerExecutor` loads (console capture, the provider and connector proxies, the `Promise.race` with `"Execution timed out"`); what the durable runtime checks before it records a value (`MAX_DURABLE_VALUE_BYTES`, `tooLargeMessage`, `toStored`); `unwrapMcpResult`; `toolInputSchema`; `searchConnectors` and `describeTarget` with their scoring and type rendering; the runtime tool's input schema, `STEP_PRELUDE`, `buildDescription`, `withGlobalsHint`, `toModelOutput` and `toJSONValue`. | MIT, Copyright (c) 2025 Cloudflare, Inc., `LICENSE-cloudflare` |
| `v8-json.js` | Written for Stellarator after V8 (v8.dev, `src/json/json-parser.cc`, `src/json/json-stringifier.cc`, `src/common/message-template.h`, V8 12 to 14): JavaScript that returns the error message V8's `JSON.parse` throws for a text, and the one `JSON.stringify` throws for a value that contains itself. It follows the steps of V8's parser and uses V8's message texts; it is a rewrite in JavaScript, modified to return a message instead of parsing. `tests/golden/v8-json.mjs` compares it with the V8 in Node. | BSD 3-Clause, Copyright 2014, the V8 project authors, `LICENSE-v8`; Stellarator's changes under its MIT license |
| `workers-globals.js` | Stellarator's. The globals of a Cloudflare Worker that the sandbox defines, written to behave as workerd does in the reference fixtures. Error messages are worded as V8 and workerd word them; no code of either is included. | Stellarator's MIT license |
| `stellarator.js`, `sandbox.js` | Stellarator's: the steps around Cloudflare's code (createCodeTool's description, the AI SDK's tool messages and the arguments it sends back, the sandbox's timers, the dispatchers behind the proxies, the Workers RPC rules for a returned value). | Stellarator's MIT license |

`cloudflare-codemode.js` was modified:

- ES module syntax removed: `acorn` is the global `acorn.js` defines, and nothing is exported.
- `Number.prototype.toLocaleString()` replaced by `__enUS(n)`, which groups digits as V8 does in
  en-US (`6,000`); QuickJS's `toLocaleString` does not group.
- `generateTypesForNamespace(tools, namespace)` added: `generateTypesFromJsonSchema` with the
  `namespace` parameter of `generateTypes` in `dist/ai.js`, which treats JSON Schema inputs the
  same way.

`cloudflare-runtime.js` was modified, and `tests/golden/extract-runtime.mjs` writes it from the
installed package so the changes can be read there:

- ES module syntax and the imports from `cloudflare:workers` removed. The classes that need the
  Workers runtime (`DynamicWorkerExecutor`, `CodemodeRuntime`, the connectors, `createProxyTool`)
  are not included.
- The body of `DynamicWorkerExecutor.execute()` that builds the module text became
  `buildExecutorModule(normalized, timeoutMs, providers, connectors)`, which returns the text
  instead of loading it. The checks and the template are unchanged, except that the early returns
  hold only `error`.
- `runCodeOutcome(executeResult)` holds what `runCode()` does with the executor's answer, and
  `runPassOutcome(...)` what `runPass()` returns for one pass, without the Durable Object calls
  around them. `transformResult` is called synchronously.
- Everything else is as published.

The sandbox (`src/sandbox.rs`) evaluates that module text in QuickJS as the ES module
`executor.js`, with a stub for `cloudflare:workers`, and calls `evaluate(dispatchers, connectors)`
on the class it exports, as the executor does through Workers RPC.

`tests/golden/cloudflare.json` holds outputs of the published package's pure helpers, run in Node
by `tests/golden/generate.mjs`. `tests/golden/fixtures/` holds what the package does on workerd,
tool calls included (`tests/golden/README.md`).
