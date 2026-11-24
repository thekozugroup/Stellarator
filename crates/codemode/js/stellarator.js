// Stellarator's code mode helpers. They run in QuickJS after cloudflare-codemode.js (and acorn.js,
// for normalizeCode), outside the sandbox, and reproduce the steps around Cloudflare's executor:
// - codeTool: createCodeTool's description (packages/codemode/src/tool.ts) and the namespaces'
//   methods, in the order Object.entries gives them;
// - invalidInput: what the AI SDK sends the model when the code tool's input is not
//   {"code": string} (AI SDK 7, zod 4);
// - render: what the model receives for one execution: runCode (run-code.ts) turned into a tool
//   message by the AI SDK (a JSON output becomes JSON.stringify(output), a thrown error becomes
//   error.toString()), with truncateResult applied to the result and the logs the way
//   createCodemodeRuntime's transformResult and toModelOutput apply it (proxy-tool.ts).
"use strict";

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

  /** null when `text` is a valid {"code": string}; else the AI SDK's error text for the model. */
  invalidInput(toolName, text) {
    let value;
    if (text.trim() === "") {
      value = {};
    } else {
      try {
        value = JSON.parse(text);
      } catch (e) {
        return `AI_InvalidToolInputError: Invalid input for tool ${toolName}: AI_JSONParseError: JSON parsing failed: Text: ${text}.\nError message: ${String(e)}`;
      }
    }
    const kind = (v) => (v === null ? "null" : Array.isArray(v) ? "array" : typeof v);
    let issue = null;
    if (kind(value) !== "object") {
      issue = { expected: "object", code: "invalid_type", path: [], message: `Invalid input: expected object, received ${kind(value)}` };
    } else if (typeof value.code !== "string") {
      issue = { expected: "string", code: "invalid_type", path: ["code"], message: `Invalid input: expected string, received ${kind(value.code)}` };
    }
    if (issue === null) return null;
    return `AI_InvalidToolInputError: Invalid input for tool ${toolName}: AI_TypeValidationError: Type validation failed: Value: ${JSON.stringify(value)}.\nError message: ${JSON.stringify([issue], null, 2)}`;
  },

  /**
   * input: {threw: string | null (the executor itself threw: error.toString()), error: string | null
   * (the execution's error, already a string), resultJson: string | null, logs: string[],
   * maxChars: number | null (null: no truncation), maxTokens: number}
   * Returns {content, isError, truncated}.
   */
  render(inputJson) {
    const r = JSON.parse(inputJson);
    if (r.threw !== null) return JSON.stringify({ content: r.threw, isError: true, truncated: false });
    const budget = r.maxChars === null ? null : { maxChars: r.maxChars, maxTokens: r.maxTokens };
    const logs = r.logs;
    let truncated = false;
    if (r.error) {
      let logText = logs.join("\n");
      if (budget) {
        const cut = truncateResponse(logText, budget);
        truncated = cut !== logText;
        logText = cut;
      }
      const logCtx = logs.length ? `\n\nConsole output:\n${logText}` : "";
      return JSON.stringify({ content: `Error: Code execution failed: ${r.error}${logCtx}`, isError: true, truncated });
    }
    let result = r.resultJson === null ? undefined : JSON.parse(r.resultJson);
    let shownLogs = logs;
    if (budget) {
      const cut = truncateResult(result, budget);
      truncated = cut !== result;
      result = cut;
      if (logs.length) {
        shownLogs = truncateResult(logs, budget);
        truncated = truncated || shownLogs !== logs;
      }
    }
    const output = logs.length ? { result, logs: shownLogs } : { result };
    return JSON.stringify({ content: JSON.stringify(output), isError: false, truncated });
  },
};
