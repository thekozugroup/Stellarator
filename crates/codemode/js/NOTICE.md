# Third-party JavaScript in code mode

Code mode runs Cloudflare's own Code Mode JavaScript in QuickJS, so the formats a model sees come
from their code. This folder holds it, with its licenses.

| File | What it is | License |
| --- | --- | --- |
| `acorn.js` | acorn 8.19.0, `dist/acorn.js` as published on npm, unmodified. Cloudflare's `normalizeCode` parses with it. | MIT, `LICENSE-acorn` |
| `cloudflare-codemode.js` | Adapted from `@cloudflare/codemode` 0.5.6 (github.com/cloudflare/agents, `packages/codemode`, commit `4116a8bdf97660c8e79a923b6aec140256282a5d`): the published `dist/json-schema-types-*.js` (normalizeCode, sanitizeToolName, jsonSchemaToType, generateTypesFromJsonSchema), `dist/truncate-*.js` (truncateResult) and `DEFAULT_DESCRIPTION`. | MIT, Copyright (c) 2025 Cloudflare, Inc., `LICENSE-cloudflare` |
| `stellarator.js`, `sandbox.js` | Stellarator's: the steps around Cloudflare's executor (createCodeTool's description, runCode and the AI SDK's tool messages, the sandbox's timers and dispatchers). | Stellarator's MIT license |

`cloudflare-codemode.js` was modified:

- ES module syntax removed: `acorn` is the global `acorn.js` defines, and nothing is exported.
- `Number.prototype.toLocaleString()` replaced by `__enUS(n)`, which groups digits as V8 does in
  en-US (`6,000`); QuickJS's `toLocaleString` does not group.
- `generateTypesForNamespace(tools, namespace)` added: `generateTypesFromJsonSchema` with the
  `namespace` parameter of `generateTypes` in `dist/ai.js`, which treats JSON Schema inputs the
  same way.

The sandbox's harness in `src/sandbox.rs` (`harness`) reproduces the `executor.js` module that
Cloudflare's `DynamicWorkerExecutor` (`packages/codemode/src/executor.ts`) generates: the console
capture, one `Proxy` per namespace that dispatches calls and throws `new Error(data.error)`, and the
`Promise.race` with `"Execution timed out"`.

`tests/golden/cloudflare.json` holds outputs of the published package, run in Node by
`tests/golden/generate.mjs`.
