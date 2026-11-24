// Stellarator's code mode helpers. They run in QuickJS after acorn.js, cloudflare-codemode.js,
// cloudflare-runtime.js and v8-json.js, outside the sandbox, and do the steps around Cloudflare's
// executor with the package's own functions:
// - codeTool: createCodeTool's description (packages/codemode/src/tool.ts) and the namespaces'
//   methods, in the order Object.entries gives them;
// - runtimeTool: the tool createCodemodeRuntime(...).tool() returns (proxy-tool.ts) for one
//   McpConnector per server: its description, and each connector as connector.describe() gives it;
// - executorModule: the module DynamicWorkerExecutor loads for one execution;
// - search, describe: codemode.search and codemode.describe of the runtime;
// - invalidInput, resendArguments: what the AI SDK (7, with zod 4) sends the model for a tool call
//   whose input the tool does not accept, and the arguments it sends back with the next request;
// - render, renderRuntime: what the model receives for one execution, through the AI SDK (a JSON
//   output becomes JSON.stringify(output), a thrown error becomes its name and message).
"use strict";

// String.prototype.localeCompare, as V8 (ICU, root collation) orders ASCII text: searchConnectors
// breaks ties between equal scores with it, on paths such as "tracker.get_issue". QuickJS compares
// code units, which puts "_" after the letters and capitals before lowercase. Order: punctuation
// and symbols in ICU's order, digits, then letters with case as the last tiebreak (lower first).
// Text with other characters falls back to code units.
(function () {
  const ORDER = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789";
  const primary = (c) => {
    const i = ORDER.indexOf(c);
    if (i >= 0) return i;
    const lower = c.toLowerCase();
    if (lower >= "a" && lower <= "z") return ORDER.length + (lower.charCodeAt(0) - 97);
    return -1;
  };
  const native = String.prototype.localeCompare;
  const compare = function localeCompare(that) {
    const a = String(this);
    const b = String(that);
    for (const s of [a, b]) for (let i = 0; i < s.length; i++) if (primary(s[i]) < 0) return native.call(a, b);
    const n = Math.min(a.length, b.length);
    for (let i = 0; i < n; i++) {
      const d = primary(a[i]) - primary(b[i]);
      if (d !== 0) return d < 0 ? -1 : 1;
    }
    if (a.length !== b.length) return a.length < b.length ? -1 : 1;
    for (let i = 0; i < n; i++) {
      if (a[i] !== b[i]) return a[i] === a[i].toLowerCase() ? -1 : 1;
    }
    return 0;
  };
  Object.defineProperty(String.prototype, "localeCompare", { value: compare, writable: true, configurable: true });
})();

const __stellarator = {
  /** spec: {description: string | null, namespaces: [{name, tools: [[key, {description, inputSchema, outputSchema}]]}]} */
  codeTool(specJson) {
    const spec = JSON.parse(specJson);
    const typeBlocks = [];
    const namespaces = [];
    for (const ns of spec.namespaces) {
      // A record, as the AI SDK ToolSet a provider passes.
      const tools = {};
      for (const [key, descriptor] of ns.tools) tools[key] = descriptor;
      typeBlocks.push(generateTypesForNamespace(tools, ns.name));
      namespaces.push({ name: ns.name, methods: Object.keys(tools).map((key) => ({ key, name: sanitizeToolName(key) })) });
    }
    const typeBlock = typeBlocks.filter(Boolean).join("\n\n");
    const description = (spec.description ?? DEFAULT_DESCRIPTION).replace("{{types}}", typeBlock);
    return JSON.stringify({ description, types: typeBlock, namespaces });
  },

  /**
   * spec: {description: string | null, hints: {name: hint}, connectors: [{name, instructions:
   * string | null, tools: [[key, {description, inputSchema, outputSchema}]]}]}.
   * Returns the tool's description and input schema, and per connector its methods (as
   * McpConnector.tools() names them) and its description (as connector.describe() returns it).
   */
  runtimeTool(specJson) {
    const spec = JSON.parse(specJson);
    const connectors = [];
    const clashes = [];
    for (const c of spec.connectors) {
      const descriptors = {};
      const methods = [];
      const sources = new Map();
      for (const [key, tool] of c.tools) {
        const name = sanitizeToolName(key);
        const existing = sources.get(name);
        if (existing !== undefined) {
          clashes.push(`"${existing}" and "${key}" both become \`${c.name}.${name}\``);
          continue;
        }
        sources.set(name, key);
        methods.push({ key, name });
        // McpConnector.tools() and CodemodeConnector.describe().
        descriptors[name] = { description: tool.description, inputSchema: toolInputSchema({ inputSchema: tool.inputSchema }), outputSchema: tool.outputSchema };
      }
      const description = { name: c.name, descriptors, annotations: {} };
      // McpConnector.describe(): the connection's instructions; the connector's own are undefined.
      if (c.instructions) description.instructions = [c.instructions, undefined].filter(Boolean).join("\n\n");
      connectors.push({ name: c.name, methods, description });
    }
    const named = connectors.map((c) => ({ name: () => c.name }));
    return JSON.stringify({
      description: buildDescription(named, spec.description ?? undefined, spec.hints ?? undefined),
      parameters: proxyJsonSchema,
      types: connectors.map((c) => renderConnectorTypes(c.name, c.description.instructions, c.description.descriptors)).join("\n\n"),
      connectors,
      clashes,
    });
  },

  /** spec: {normalized, timeoutMs, providers: [name], connectors: [name], runtime: boolean}. */
  executorModule(specJson) {
    const spec = JSON.parse(specJson);
    const providers = spec.providers.map((name) => (spec.runtime && name === "codemode" ? { name, prelude: STEP_PRELUDE } : { name }));
    return JSON.stringify(buildExecutorModule(spec.normalized, spec.timeoutMs, providers, spec.connectors.map((name) => ({ name }))));
  },

  /**
   * codemode.search(...args) or codemode.describe(...args), as the runtime's platform provider
   * answers them through its ToolDispatcher: the dispatcher's JSON, `{result}`.
   */
  platform(method, descriptionsJson, argsJson) {
    const descriptions = JSON.parse(descriptionsJson);
    const parsed = argsJson ? parseForCodemode(argsJson) : [];
    const args = Array.isArray(parsed) ? parsed : [parsed];
    const result = method === "search" ? searchConnectors(String(args[0]), descriptions, []) : describeTarget(String(args[0]), descriptions, []);
    return stringifyForCodemode({ result });
  },

  sanitize(name) {
    return sanitizeToolName(name);
  },

  normalize(code) {
    return normalizeCode(code);
  },

  /** truncateResult(JSON.parse(json), options) as JSON (for tests). */
  truncate(json, optionsJson) {
    return JSON.stringify(truncateResult(JSON.parse(json), JSON.parse(optionsJson)));
  },

  /** V8's message for JSON.parse(text), or null (for tests). */
  jsonError(text) {
    return __v8JsonParseError(text);
  },

  /** createProxyTool's answer to code the durable log cannot hold, as the model receives it. */
  codeTooLarge(length) {
    return JSON.stringify(toModelOutput({ status: "error", executionId: "", error: tooLargeMessage("The execution code", Number(length)) }).value);
  },

  /** The tool call's input as the AI SDK parses it: `{value}`, or `{error}` with the SyntaxError. */
  parseInput(text) {
    if (text.trim() === "") return { value: {} };
    const message = __v8JsonParseError(text);
    if (message !== null) return { error: `SyntaxError: ${message}` };
    return { value: JSON.parse(text) };
  },

  /**
   * null when `text` is a valid {"code": string}; else the AI SDK's error text for the model.
   * `format` picks the schema: "code_tool" is zod's z.object({code: z.string()}), "runtime" the
   * runtime tool's own Standard Schema.
   */
  invalidInput(format, toolName, text) {
    const parsed = this.parseInput(text);
    if (parsed.error) {
      return `AI_InvalidToolInputError: Invalid input for tool ${toolName}: AI_JSONParseError: JSON parsing failed: Text: ${text}.\nError message: ${parsed.error}`;
    }
    const value = parsed.value;
    let issues = null;
    if (format === "runtime") {
      const ok = typeof value === "object" && value !== null && "code" in value && typeof value.code === "string";
      if (!ok) issues = JSON.stringify([{ message: "Expected an object with a string code property" }]);
    } else {
      const kind = (v) => (v === null ? "null" : Array.isArray(v) ? "array" : typeof v);
      let issue = null;
      if (kind(value) !== "object") {
        issue = { expected: "object", code: "invalid_type", path: [], message: `Invalid input: expected object, received ${kind(value)}` };
      } else if (typeof value.code !== "string") {
        issue = { expected: "string", code: "invalid_type", path: ["code"], message: `Invalid input: expected string, received ${kind(value.code)}` };
      }
      if (issue !== null) issues = JSON.stringify([issue], null, 2);
    }
    if (issues === null) return null;
    return `AI_InvalidToolInputError: Invalid input for tool ${toolName}: AI_TypeValidationError: Type validation failed: Value: ${JSON.stringify(value)}.\nError message: ${issues}`;
  },

  /**
   * The `arguments` the AI SDK sends back in the assistant message of the next request for a tool
   * call the model made with `text`: the validated input for a valid call of the code tool
   * (`{"code": ...}` alone), the parsed input as JSON for one it refused when that is an object,
   * an array or null, else `{}`.
   */
  resendArguments(valid, text) {
    const parsed = this.parseInput(text);
    if (parsed.error) return "{}";
    const value = parsed.value;
    if (valid && value && typeof value.code === "string") return JSON.stringify({ code: value.code });
    return typeof value === "object" ? JSON.stringify(value) : "{}";
  },

  /**
   * input: {threw: {name, message} | null (the executor itself threw), error: string | null (the
   * execution's error), resultJson: string | null, logs: string[], resultBudget: {maxChars,
   * maxTokens} | null (truncateResult's budget for the result; null: none), logsBudget: the same
   * for the console output}. Returns {content, isError, truncated}.
   */
  render(inputJson) {
    const r = JSON.parse(inputJson);
    if (r.threw !== null) return JSON.stringify({ content: `${r.threw.name}: ${r.threw.message}`, isError: true, truncated: false });
    const logs = r.logs;
    if (r.error !== null) {
      // The console output is part of the error's message, whatever its length.
      return JSON.stringify({ content: `Error: ${runCodeOutcome({ error: r.error, logs }).threw}`, isError: true, truncated: false });
    }
    let result = r.resultJson === null ? undefined : JSON.parse(r.resultJson);
    let shownLogs = logs;
    let truncated = false;
    if (r.resultBudget) {
      const cut = truncateResult(result, r.resultBudget);
      truncated = cut !== result;
      result = cut;
    }
    if (r.logsBudget && logs.length) {
      shownLogs = truncateResult(logs, r.logsBudget);
      truncated = truncated || shownLogs !== logs;
    }
    const { output } = runCodeOutcome({ result, logs: shownLogs });
    return JSON.stringify({ content: JSON.stringify(output), isError: false, truncated });
  },

  /**
   * The runtime format: runPass() and toModelOutput() for one execution. input: render's fields
   * (the console output's budget is toModelOutput's own), and executionId, connectors (names),
   * execution: {status, error?} (the execution's record), unserializable: string | null (why the
   * result has no JSON form). Returns {content, isError, truncated, error}: `error` is the message
   * the model saw, if any.
   */
  renderRuntime(inputJson) {
    const r = JSON.parse(inputJson);
    const setup = { connectorsByName: new Map(r.connectors.map((name) => [name, true])) };
    let threw = r.threw === null ? null : r.threw.message;
    let output;
    if (threw === null) {
      const outcome = runCodeOutcome({ result: r.resultJson === null ? undefined : JSON.parse(r.resultJson), error: r.error ?? undefined, logs: r.logs });
      if (outcome.threw !== undefined) threw = outcome.threw;
      else output = outcome.output;
    }
    let truncated = false;
    const transform = r.resultBudget
      ? (value) => {
          const cut = truncateResult(value, r.resultBudget);
          truncated = cut !== value;
          return cut;
        }
      : null;
    const passed = runPassOutcome(r.executionId, r.execution, threw, output, setup, transform);
    if (passed.status === "completed" && r.unserializable) {
      // toJSONValue() on a result JSON.stringify refuses (a cycle).
      const value = { error: `Result could not be serialized for the model: ${r.unserializable}` };
      return JSON.stringify({ content: JSON.stringify(value), isError: true, truncated: false, error: value.error });
    }
    if (Array.isArray(passed.logs) && truncateResult(passed.logs) !== passed.logs) truncated = true;
    const model = toModelOutput(passed);
    return JSON.stringify({ content: JSON.stringify(model.value), isError: passed.status !== "completed", truncated, error: passed.status === "error" ? passed.error : null });
  },
};
