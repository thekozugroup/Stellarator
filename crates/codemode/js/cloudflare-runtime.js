// Adapted from @cloudflare/codemode 0.5.6 (https://github.com/cloudflare/agents, packages/codemode,
// commit 4116a8bdf97660c8e79a923b6aec140256282a5d), MIT License, Copyright (c) 2025 Cloudflare, Inc.
// (see LICENSE-cloudflare). These are parts of the package's published dist/index.js (and
// dist/run-code-*.js, dist/base-*.js), the ones that are pure: the value codec, the text of the module
// DynamicWorkerExecutor loads, what the durable runtime checks before it records a value, the MCP
// result unwrapping, a connector's default input schema, codemode.search and codemode.describe, the runtime tool's description and
// input schema, and how a result becomes the model's output. Modified for QuickJS:
// - no ES module syntax, and no imports from "cloudflare:workers": the classes that need Workers
//   (DynamicWorkerExecutor, CodemodeRuntime, the connectors) are not here;
// - the module text DynamicWorkerExecutor.execute() builds is returned by buildExecutorModule()
//   instead of being loaded, with the method's own checks and strings unchanged;
// - runPass() and createProxyTool() are split into the functions below that do not touch the
//   Durable Object: runCodeOutcome(), runPassOutcome().
// It runs after cloudflare-codemode.js, whose sanitizeToolName, generateTypesFromJsonSchema,
// normalizeCode and truncateResult it uses. Everything else is as published.

//#region src/codec.ts
const BINARY_TAG = "__codemode_binary_v1__";
const BIGINT_TAG = "__codemode_bigint_v1__";
function bytesToBase64(bytes) {
	let binary = "";
	const chunkSize = 32768;
	for (let i = 0; i < bytes.byteLength; i += chunkSize) binary += String.fromCharCode(...bytes.subarray(i, Math.min(i + chunkSize, bytes.byteLength)));
	return btoa(binary);
}
function base64ToBytes(b64) {
	const binary = atob(b64);
	const bytes = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
	return bytes;
}
function encodeCodemodeValue(value) {
	if (value instanceof Uint8Array) return {
		[BINARY_TAG]: "Uint8Array",
		data: bytesToBase64(value)
	};
	if (value instanceof ArrayBuffer) return {
		[BINARY_TAG]: "ArrayBuffer",
		data: bytesToBase64(new Uint8Array(value))
	};
	if (ArrayBuffer.isView(value)) {
		const view = value;
		return {
			[BINARY_TAG]: "ArrayBufferView",
			data: bytesToBase64(new Uint8Array(view.buffer, view.byteOffset, view.byteLength))
		};
	}
	return value;
}
function decodeCodemodeValue(value) {
	if (!value || typeof value !== "object" || !("__codemode_binary_v1__" in value)) return value;
	const encoded = value;
	if (typeof encoded.data !== "string") return value;
	const bytes = base64ToBytes(encoded.data);
	if (encoded["__codemode_binary_v1__"] === "ArrayBuffer") return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
	return bytes;
}
function stringifyForCodemode(value) {
	return JSON.stringify(value, (_key, nested) => encodeCodemodeValue(nested));
}
function parseForCodemode(json) {
	return JSON.parse(json, (_key, nested) => decodeCodemodeValue(nested));
}
/**
* Serialize a value for durable storage. Returns `undefined` when the value is
* `undefined` (callers store SQL NULL — distinguishing "no value" from a
* recorded `null`, which serializes to the string `"null"`).
*
* Throws on values JSON cannot represent even with the codec (e.g. cycles):
* a durable replay log cannot faithfully store such a value, and silently
* storing an approximation would corrupt replay.
*/
function stringifyForStorage(value) {
	if (value === void 0) return void 0;
	return JSON.stringify(value, (_key, nested) => {
		if (typeof nested === "bigint") return { [BIGINT_TAG]: nested.toString() };
		return encodeCodemodeValue(nested);
	});
}
function parseForStorage(json) {
	if (json === null) return void 0;
	return JSON.parse(json, (_key, nested) => {
		if (nested && typeof nested === "object" && BIGINT_TAG in nested && typeof nested[BIGINT_TAG] === "string") return BigInt(nested[BIGINT_TAG]);
		return decodeCodemodeValue(nested);
	});
}
//#endregion
//#region src/executor.ts
const CONNECTOR_CONTROL_KEY = "__codemode_control__";
const PAUSE_SENTINEL_LITERAL = "__CODEMODE_PAUSE__";
const SANDBOX_CODEC = String.raw`
    const __CODEMODE_BINARY_TAG = "__codemode_binary_v1__";
    function __bytesToBase64(bytes) {
      let binary = "";
      const chunkSize = 0x8000;
      for (let i = 0; i < bytes.byteLength; i += chunkSize) {
        binary += String.fromCharCode(...bytes.subarray(i, Math.min(i + chunkSize, bytes.byteLength)));
      }
      return btoa(binary);
    }
    function __base64ToBytes(b64) {
      const binary = atob(b64);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
      return bytes;
    }
    function __encodeCodemodeValue(value) {
      if (value instanceof Uint8Array) {
        return { [__CODEMODE_BINARY_TAG]: "Uint8Array", data: __bytesToBase64(value) };
      }
      if (value instanceof ArrayBuffer) {
        return { [__CODEMODE_BINARY_TAG]: "ArrayBuffer", data: __bytesToBase64(new Uint8Array(value)) };
      }
      if (ArrayBuffer.isView(value)) {
        return { [__CODEMODE_BINARY_TAG]: "ArrayBufferView", data: __bytesToBase64(new Uint8Array(value.buffer, value.byteOffset, value.byteLength)) };
      }
      return value;
    }
    function __decodeCodemodeValue(value) {
      if (!value || typeof value !== "object" || !(__CODEMODE_BINARY_TAG in value) || typeof value.data !== "string") return value;
      const bytes = __base64ToBytes(value.data);
      if (value[__CODEMODE_BINARY_TAG] === "ArrayBuffer") {
        return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
      }
      return bytes;
    }
    function __stringifyForCodemode(value) {
      return JSON.stringify(value, (_key, nested) => __encodeCodemodeValue(nested));
    }
    function __parseForCodemode(json) {
      return JSON.parse(json, (_key, nested) => __decodeCodemodeValue(nested));
    }
`;
const LATE_CALL_MESSAGE = "The code execution finished before this tool call settled. Await every tool call before returning.";
const DEFAULT_DYNAMIC_WORKER_EXECUTION_TIMEOUT_MS = 6e4;
/**
* The module DynamicWorkerExecutor.execute() loads as "executor.js" for `normalized` code, or the
* error the method returns for names it refuses: `{ module }` or `{ error }`. `providers` are
* `{ name, prelude? }`, `connectors` are `{ name }`.
*/
function buildExecutorModule(normalized, timeoutMs, providers, connectors) {
		const RESERVED_NAMES = /* @__PURE__ */ new Set([
			"__dispatchers",
			"__connectors",
			"__logs",
			"__CODEMODE_BINARY_TAG",
			"__bytesToBase64",
			"__base64ToBytes",
			"__encodeCodemodeValue",
			"__decodeCodemodeValue",
			"__stringifyForCodemode",
			"__parseForCodemode",
			"WorkerEntrypoint",
			"CodeExecutor",
			"Promise",
			"setTimeout",
			"Error",
			"console"
		]);
		const VALID_IDENT = /^[a-zA-Z_$][a-zA-Z0-9_$]*$/;
		const seenNames = /* @__PURE__ */ new Set();
		for (const provider of providers) {
			if (RESERVED_NAMES.has(provider.name)) return {
				error: `Provider name "${provider.name}" is reserved`
			};
			if (!VALID_IDENT.test(provider.name)) return {
				error: `Provider name "${provider.name}" is not a valid JavaScript identifier`
			};
			if (seenNames.has(provider.name)) return {
				error: `Duplicate provider name "${provider.name}"`
			};
			seenNames.add(provider.name);
		}
		const connectorNames = new Set(connectors.map((c) => c.name));
		for (const connector of connectors) {
			if (RESERVED_NAMES.has(connector.name)) return {
				error: `Connector name "${connector.name}" is reserved`
			};
			if (!VALID_IDENT.test(connector.name)) return {
				error: `Connector name "${connector.name}" is not a valid JavaScript identifier`
			};
			if (seenNames.has(connector.name)) return {
				error: `Duplicate name "${connector.name}" (connector clashes with provider)`
			};
			seenNames.add(connector.name);
		}
		const proxyInits = providers.filter((p) => !connectorNames.has(p.name)).map((p) => `    const ${p.name} = new Proxy({}, {\n      get: (target, toolName) => {\n        if (Object.prototype.hasOwnProperty.call(target, toolName)) return target[toolName];\n        if (typeof toolName !== "string") return undefined;\n        return async (...args) => {\n          const resJson = await __dispatchers.${p.name}.call(String(toolName), __stringifyForCodemode(args));\n          const data = __parseForCodemode(resJson);\n          if (data.error) throw new Error(data.error);\n          return data.result;\n        };\n      }\n    });`);
		const preludeInits = providers.filter((p) => !connectorNames.has(p.name) && p.prelude).map((p) => p.prelude);
		const connectorProxyInits = connectors.map((c) => `    const ${c.name} = new Proxy({}, {\n      get: (_, toolName) => {\n        if (typeof toolName !== "string") return undefined;\n        return async (...args) => {\n          const __r = await __connectors.${c.name}.callTool(toolName, args[0]);\n          if (__r && typeof __r === "object") {\n            if (__r.${CONNECTOR_CONTROL_KEY} === "pause") throw new Error("${PAUSE_SENTINEL_LITERAL}");\n            if (__r.${CONNECTOR_CONTROL_KEY} === "error") throw new Error(String(__r.message));\n          }\n          return __r;\n        };\n      }\n    });`);
		const executorModule = [
			"import { WorkerEntrypoint } from \"cloudflare:workers\";",
			"",
			"export default class CodeExecutor extends WorkerEntrypoint {",
			"  async evaluate(__dispatchers = {}, __connectors = {}) {",
			"    const __logs = [];",
			"    console.log = (...a) => { __logs.push(a.map(String).join(\" \")); };",
			"    console.warn = (...a) => { __logs.push(\"[warn] \" + a.map(String).join(\" \")); };",
			"    console.error = (...a) => { __logs.push(\"[error] \" + a.map(String).join(\" \")); };",
			SANDBOX_CODEC,
			...proxyInits,
			...connectorProxyInits,
			...preludeInits,
			"",
			"    try {",
			"      const result = await Promise.race([",
			"        ("
		].concat([normalized]).concat([
			")(),",
			"        new Promise((_, reject) => setTimeout(() => reject(new Error(\"Execution timed out\")), " + timeoutMs + "))",
			"      ]);",
			"      return { result, logs: __logs };",
			"    } catch (err) {",
			"      return { result: undefined, error: err.message, logs: __logs };",
			"    }",
			"  }",
			"}"
		]).join("\n");
		return { module: executorModule };
}
//#endregion
//#region src/run-code.ts
/**
* What runCode() does with the executor's answer `{ result, error?, logs? }`: `{ threw }` with the
* message of the Error it throws, or `{ output }` with what it returns.
*/
function runCodeOutcome(executeResult) {
	if (executeResult.error) {
		const logCtx = executeResult.logs?.length ? `\n\nConsole output:\n${executeResult.logs.join("\n")}` : "";
		return { threw: `Code execution failed: ${executeResult.error}${logCtx}` };
	}
	return { output: executeResult.logs?.length ? {
		result: executeResult.result,
		logs: executeResult.logs
	} : { result: executeResult.result } };
}
//#endregion
//#region src/runtime.ts
/** Connector name used for `codemode.step(name, fn)` log entries. */
const STEP_CONNECTOR = "__step";
/**
* Cap for any single serialized value stored in the durable log (args, a
* recorded result, the final result). SQLite rows in Durable Objects max out
* at ~2MB; 1MB leaves headroom for the rest of the row. Truncating stored
* values is never an option — replay would feed resumed code corrupted data —
* so a breach fails the run with a model-actionable error instead.
*/
const MAX_DURABLE_VALUE_BYTES = 1e6;
/** Model-actionable error for a value too large to record durably. */
function tooLargeMessage(what, size) {
	return `${what} is too large to record durably (${size} bytes > ${MAX_DURABLE_VALUE_BYTES} byte limit). Write large data to a file or workspace instead and pass/return a small reference (such as a path).`;
}
/**
* Serialize a value for a durable column. `undefined` → SQL NULL; throws a
* model-actionable error when the value is too large or not serializable
* (e.g. cyclic) — the durable log can't store an approximation.
*/
function toStored(what, value) {
	let stored;
	try {
		stored = stringifyForStorage(value);
	} catch (err) {
		throw new Error(`${what} could not be recorded durably (not serializable: ${err instanceof Error ? err.message : String(err)}). Only JSON-compatible values (plus binary and bigint) can cross a durable replay boundary.`);
	}
	if (stored === void 0) return null;
	if (stored.length > 1e6) throw new Error(tooLargeMessage(what, stored.length));
	return stored;
}
//#endregion
//#region src/connectors/base.ts
function toolInputSchema(t) {
	const loose = t;
	for (const candidate of [loose.inputSchema, loose.parameters]) if (candidate && typeof candidate === "object" && ("type" in candidate || "properties" in candidate || "$ref" in candidate)) return candidate;
	return { type: "object" };
}
//#endregion
//#region src/connectors/mcp.ts
function unwrapMcpResult(result) {
	if (result.toolResult !== void 0) return result.toolResult;
	if (result.isError) {
		const msg = result.content?.filter((c) => c.type === "text").map((c) => "text" in c ? c.text : "").join("\n") || "Tool call failed";
		throw new Error(msg);
	}
	if (result.structuredContent != null) return result.structuredContent;
	const content = result.content;
	if (!(content !== void 0 && content.length > 0 && content.every((c) => c.type === "text"))) return result;
	const text = content.map((c) => c.text ?? "").join("\n");
	try {
		return JSON.parse(text);
	} catch {
		return text;
	}
}
//#endregion
//#region src/connectors/search.ts
const SEARCH_RESULT_LIMIT = 50;
const FIELD_WEIGHTS = {
	path: 12,
	connector: 8,
	method: 10,
	description: 5
};
function normalizeSearchText(value) {
	return value.replace(/([a-z0-9])([A-Z])/g, "$1 $2").replace(/[_./:-]+/g, " ").toLowerCase().trim();
}
function tokenize(value) {
	return normalizeSearchText(value).split(/[^a-z0-9]+/).map((t) => t.trim()).filter(Boolean);
}
function prepareField(value) {
	return {
		raw: normalizeSearchText(value ?? ""),
		tokens: tokenize(value ?? "")
	};
}
function scoreField(query, queryTokens, field, weight) {
	if (field.raw.length === 0) return {
		score: 0,
		matchedTokens: /* @__PURE__ */ new Set(),
		exactPhrase: false
	};
	let score = 0;
	const matchedTokens = /* @__PURE__ */ new Set();
	const exactPhrase = query.length > 0 && field.raw.includes(query);
	if (query.length > 0) {
		if (field.raw === query) score += weight * 14;
		else if (field.raw.startsWith(query)) score += weight * 9;
		else if (exactPhrase) score += weight * 6;
	}
	for (const token of queryTokens) if (field.tokens.includes(token)) {
		score += weight * 4;
		matchedTokens.add(token);
	} else if (field.tokens.some((c) => c.startsWith(token) || token.startsWith(c))) {
		score += weight * 2;
		matchedTokens.add(token);
	} else if (field.raw.includes(token)) {
		score += weight;
		matchedTokens.add(token);
	}
	return {
		score,
		matchedTokens,
		exactPhrase
	};
}
function scoreMatch(item, query) {
	const normalizedQuery = normalizeSearchText(query);
	const queryTokens = tokenize(query);
	if (normalizedQuery.length === 0 || queryTokens.length === 0) return null;
	const fields = [
		scoreField(normalizedQuery, queryTokens, prepareField(item.path), FIELD_WEIGHTS.path),
		scoreField(normalizedQuery, queryTokens, prepareField(item.connector), FIELD_WEIGHTS.connector),
		scoreField(normalizedQuery, queryTokens, prepareField(item.method), FIELD_WEIGHTS.method),
		scoreField(normalizedQuery, queryTokens, prepareField(item.description), FIELD_WEIGHTS.description)
	];
	const matchedTokens = /* @__PURE__ */ new Set();
	let score = 0;
	let exactPhrase = false;
	for (const field of fields) {
		score += field.score;
		exactPhrase ||= field.exactPhrase;
		for (const t of field.matchedTokens) matchedTokens.add(t);
	}
	if (matchedTokens.size === 0) return null;
	const coverage = matchedTokens.size / queryTokens.length;
	if (coverage < (queryTokens.length <= 2 ? 1 : .6) && !exactPhrase) return null;
	if (coverage === 1) score += 25;
	else score += Math.round(coverage * 10);
	const pathTokens = tokenize(item.path);
	const methodTokens = tokenize(item.method);
	if (pathTokens[0] === queryTokens[0] || methodTokens[0] === queryTokens[0]) score += 8;
	if (normalizeSearchText(item.path) === normalizedQuery || normalizeSearchText(item.method) === normalizedQuery) score += 20;
	return {
		path: item.path,
		connector: item.connector,
		method: item.method,
		description: item.description,
		...item.requiresApproval ? { requiresApproval: true } : {},
		kind: item.kind,
		score
	};
}
function searchConnectors(query, descriptions, snippets) {
	const items = [];
	for (const desc of descriptions) for (const [methodName, descriptor] of Object.entries(desc.descriptors)) items.push({
		path: `${desc.name}.${methodName}`,
		connector: desc.name,
		method: methodName,
		description: descriptor?.description,
		...desc.annotations?.[methodName]?.requiresApproval ? { requiresApproval: true } : {},
		kind: "method"
	});
	if (snippets) for (const snippet of snippets) items.push({
		path: snippet.name,
		connector: "snippet",
		method: snippet.name,
		description: snippet.description,
		kind: "snippet"
	});
	const ranked = items.map((item) => scoreMatch(item, query)).filter((r) => r !== null).sort((a, b) => b.score - a.score || a.path.localeCompare(b.path));
	const total = ranked.length;
	return {
		results: ranked.slice(0, SEARCH_RESULT_LIMIT),
		total,
		truncated: total > SEARCH_RESULT_LIMIT
	};
}
//#endregion
//#region src/connectors/describe.ts
/**
* Connector and snippet docs rendering — derives TypeScript documentation from
* connector descriptors on demand. Also renders snippet source.
*/
function renderConnectorTypes(connectorName, instructions, descriptors) {
	return [instructions, generateTypesFromJsonSchema(descriptors).replace("declare const codemode", `declare const ${sanitizeToolName(connectorName)}`)].filter(Boolean).join("\n\n");
}
function renderMethodTypes(methodName, descriptors) {
	const descriptor = descriptors[methodName];
	if (!descriptor) return "";
	const generated = generateTypesFromJsonSchema({ [methodName]: descriptor });
	return generated.slice(0, generated.indexOf("declare const codemode")).trim();
}
function describeTarget(target, descriptions, snippets) {
	if (snippets) {
		const snippet = snippets.find((s) => s.name === target);
		if (snippet) {
			const parts = [snippet.description];
			parts.push(`\`\`\`ts\n${snippet.code}\n\`\`\``);
			return {
				path: snippet.name,
				description: snippet.description,
				types: parts.join("\n\n"),
				kind: "snippet"
			};
		}
	}
	const [maybeConnector, maybeMethod] = target.includes(".") ? target.split(".", 2) : [target, void 0];
	const connector = descriptions.find((d) => d.name === maybeConnector);
	if (connector && !maybeMethod) return {
		path: connector.name,
		description: connector.instructions,
		types: renderConnectorTypes(connector.name, connector.instructions, connector.descriptors),
		kind: "connector"
	};
	const candidates = connector ? [connector] : descriptions;
	const methodName = maybeMethod ?? target;
	for (const candidate of candidates) if (candidate.descriptors[methodName]) return {
		path: `${candidate.name}.${methodName}`,
		description: candidate.descriptors[methodName]?.description,
		...candidate.annotations?.[methodName]?.requiresApproval ? { requiresApproval: true } : {},
		types: renderMethodTypes(methodName, candidate.descriptors),
		kind: "method"
	};
	return {
		path: target,
		description: void 0,
		types: `"${target}" not found.`,
		kind: "method"
	};
}
//#endregion
//#region src/proxy-tool.ts
const proxyJsonSchema = {
	type: "object",
	properties: { code: { type: "string" } },
	required: ["code"],
	additionalProperties: false
};
const STEP_PRELUDE = String.raw`
    codemode.step = async (name, fn) => {
      const decision = await codemode.__stepDecide(name);
      if (decision.kind === "replay") return decision.result;
      // Anything other than "execute" (i.e. a pause from divergence) aborts
      // the run; the reason is recorded on the execution.
      if (decision.kind !== "execute") throw new Error("${"__CODEMODE_PAUSE__"}");
      const value = await fn();
      await codemode.__stepRecord(decision.seq, value);
      return value;
    };`;
const CONTROL_KEY = "__codemode_control__";
function buildDescription(connectors, customDescription, connectorHints) {
	if (customDescription) return customDescription;
	const namespaces = connectors.map((c) => {
		const name = c.name();
		const hint = connectorHints?.[name];
		return hint ? `- \`${name}\` — ${hint}` : `- \`${name}\``;
	}).join("\n");
	return [
		"Execute JavaScript in a sandbox with access to connector SDKs.",
		"",
		"## Workflow",
		"",
		"1. `const matches = await codemode.search(\"short intent phrase\");`",
		"2. `const docs = await codemode.describe(matches.results[0].path);`",
		"3. Call the method: `await <connector>.<method>(args);`",
		"",
		"## Rules",
		"",
		`- The ONLY globals are ${connectors.map((c) => `\`${c.name()}\``).join(", ")} and \`codemode\` (plus standard JavaScript). There is no \`host\`, \`fs\`, \`require\`, \`process\`, or Node.js API — all I/O goes through the connectors below.`,
		"- Never guess method names. If you have not used a connector in this conversation, run a discovery pass first: `codemode.search(query)` returns ranked matches across connector methods and saved snippets.",
		"- `codemode.describe(\"connector.method\")` returns TypeScript type declarations.",
		"- `codemode.step(name, fn)` wraps side-effectful or nondeterministic work (raw fetch, random, time) so it runs once and is replayed on resume. Use it for anything that isn't a connector call.",
		"- Some methods require approval. The run pauses until the user approves, then resumes automatically. Write code as if the call returns normally.",
		"- A result with `status: \"paused\"` means the run is awaiting human approval. Tell the user what is pending and wait — do NOT re-issue the code; the run resumes on its own once approved.",
		"- All code outside connector calls and `codemode.step` must be deterministic so resume can replay it.",
		"- Do not use `fetch` — use connector SDKs.",
		"",
		"## Snippets",
		"",
		"Snippets are saved scripts you can reuse.",
		"- `codemode.run(\"name\", input)` runs a saved snippet. Snippets appear in `codemode.search` results.",
		"- If a script may be saved as a snippet later, write it as `async (input) => { ... }` so it can take input.",
		"",
		"## Available connectors",
		"",
		namespaces
	].join("\n");
}
/**
* A sandbox `ReferenceError` usually means the model invented a global (e.g.
* `host.writeFile(...)`). Append the real globals so the retry is informed
* instead of another guess.
*/
function withGlobalsHint(message, setup) {
	if (!/\bis not defined\b/.test(message)) return message;
	return `${message} (the only globals available in the sandbox are: ${[...setup.connectorsByName.keys(), "codemode"].join(", ")})`;
}
/**
* Project a tool output for the model: drop the audit-only `calls` log and
* bound the sandbox `logs` the same way `truncateResult` bounds a result.
* Never throws — a completed run must not fail at model assembly because its
* value carried a BigInt or a cycle.
*/
function toModelOutput(output) {
	if (typeof output !== "object" || output === null || Array.isArray(output)) return {
		type: "json",
		value: toJSONValue(output)
	};
	const { calls: _calls, ...rest } = output;
	if (Array.isArray(rest.logs)) rest.logs = truncateResult(rest.logs);
	return {
		type: "json",
		value: toJSONValue(rest)
	};
}
function toJSONValue(value) {
	try {
		const serialized = JSON.stringify(value, (_key, v) => typeof v === "bigint" ? v.toString() : v);
		return serialized === void 0 ? null : JSON.parse(serialized);
	} catch (err) {
		return { error: `Result could not be serialized for the model: ${err instanceof Error ? err.message : String(err)}` };
	}
}
/**
* What runPass() returns for one pass, from what the Durable Object and the executor would have
* given it: `execution` is the execution's record (`{ status, error? }`), `threw` the message of
* the error runCode() threw (or null), `output` what it returned. `calls` (the audit log) is left
* out: toModelOutput() drops it before the model sees the result. `transformResult` is synchronous
* here (truncateResult, or none).
*/
function runPassOutcome(executionId, execution, threw, output, setup, transformResult) {
	if (execution?.status === "error") return {
		status: "error",
		executionId,
		error: execution.error ?? "Codemode execution failed"
	};
	if (threw !== null && threw !== void 0) return {
		status: "error",
		executionId,
		error: withGlobalsHint(threw, setup)
	};
	const result = output?.result;
	return {
		status: "completed",
		executionId,
		result: transformResult ? transformResult(result) : result,
		logs: output?.logs
	};
}
//#endregion
