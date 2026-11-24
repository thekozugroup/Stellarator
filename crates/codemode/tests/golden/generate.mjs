// Writes the golden outputs of @cloudflare/codemode 0.5.6 (its published dist files, run in Node)
// for Stellarator's code mode tests: normalizeCode, sanitizeToolName, generateTypes (as
// createCodeTool composes them into the description), and truncateResult.
//
//   mkdir cf && cd cf && npm init -y && npm install @cloudflare/codemode@0.5.6
//   node generate.mjs cf/node_modules/@cloudflare/codemode/dist/ cloudflare.json
//
// The chunk file names below are those of the 0.5.6 package.
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
const dist = pathToFileURL(resolve(process.argv[2]) + "/").href;
const jst = await import(dist + "json-schema-types-D_m9tVnI.js");
const trunc = await import(dist + "truncate-Dd-VZZYQ.js");
const { t: DEFAULT_DESCRIPTION } = await import(dist + "shared-SMjV5yN4.js");
const normalizeCode = jst.s, sanitizeToolName = jst.a, generateTypesFromJsonSchema = jst.t;
const truncateResult = trunc.n;

const normalizeInputs = [
  "", "   \n\t  ", "async () => { return 1; }", "() => { return 1; }", "async (a, b) => { return a + b; }", "() => 42",
  "async () => { return 1; };", "1 + 2", "const x = 10;\nx * 2", 'console.log("hello")', "await fetch('http://example.com')",
  "const x = 42", "for (let i = 0; i < 10; i++) {}", "if (true) { doStuff(); }", "const a = 1;\nconst b = 2;",
  "const = oops {{{", "  () => 42  ", "```js\nasync () => { return 1; }\n```", "```javascript\nconst x = 1;\nx + 2\n```",
  "```typescript\n() => 42\n```", "```ts\n() => 42\n```", "```tsx\n() => 42\n```", "```jsx\n() => 42\n```",
  "```\nasync () => { return 1; }\n```", "```js\nasync () => { return 1; }\n```  ", "```js\nasync () => { return 1; }",
  "export default async () => { return 1; }", "export default () => { return 1; }", "export default 42",
  "export default function() { return 42; }", "export default class { constructor() {} }",
  "async function doStuff() { return 42; }", "function compute() { return 1 + 2; }",
  "function helper() { return 1; }\nhelper()", "(async () => { return 42; })()", "return 42",
  "```js\nexport default async () => { return 1; }\n```", "```js\nconst x = 1;\nx\n```",
  "const r = await mock.lookup({ query: \"tides\" });\nr", "const s = await mock.add({ a: 2, b: 3 }); // sum\n({ s })",
  "async () => {\n  const a: number = 1;\n  return a;\n}", "this is not valid javascript @#$",
  "const é = ' '; é", "label: for (;;) { break label; }\n'done'",
];
const sanitizeInputs = [
  "my-tool", "3d-render", "delete", "get-weather", "api.v2.search", "my tool", "3drender", "class", "return",
  "hello@world!", "", "@#$", "@#!", "getWeather", "_private", "$jquery", "github", "my-server", "list_pull_requests",
  "await", "yield", "eval", "arguments", "undefined", "NaN", "Infinity", "let", "static", "of", "ünïcode",
];

// Tool sets: Cloudflare's README example, the mock MCP server's tools, and schemas that exercise
// every branch of jsonSchemaToTypeString.
const toolSets = [
  { namespaces: [{ name: "codemode", tools: [["getWeather", { description: "Get weather for a city", inputSchema: { type: "object", properties: { city: { type: "string", description: "City name" } }, required: ["city"] } }]] }] },
  { namespaces: [{ name: "mock", tools: [
    ["lookup", { description: "Look up a term in the reference catalog and return what it says.", inputSchema: { type: "object", properties: { query: { type: "string", description: "The term to look up." } }, required: ["query"] } }],
    ["add", { description: "Add two numbers.", inputSchema: { type: "object", properties: { a: { type: "number" }, b: { type: "number" } }, required: ["a", "b"] } }],
    ["fail", { description: "Always fails, with an error result.", inputSchema: { type: "object", properties: { reason: { type: "string" } } } }],
    ["slow", { description: "Waits some seconds before answering.", inputSchema: { type: "object", properties: { seconds: { type: "number", description: "Seconds to wait (default 2)." } } } }],
    ["chart", { description: "Draws a chart of the data.", inputSchema: { type: "object", properties: {} } }],
    ["repeat", { description: "Repeats a text.", inputSchema: { type: "object", properties: { text: { type: "string" }, times: { type: "integer" } }, required: ["text", "times"] } }],
  ] }] },
  { namespaces: [
    { name: "github", tools: [
      ["list-pull-requests", { description: "List pull requests.\nFiltered by state.", inputSchema: { type: "object", properties: {
        owner: { type: "string", description: "Repository owner" }, repo: { type: "string" },
        state: { type: "string", enum: ["open", "closed", "all"], description: "*/ not a comment end" },
        per_page: { type: "integer", minimum: 1 }, labels: { type: "array", items: { type: "string" } },
        since: { type: "string", format: "date-time" }, "x-trace-id": { type: "string", description: "Quoted name" },
        filter: { type: "object", properties: { author: { type: "string" }, draft: { type: "boolean" } }, required: ["author"] },
        sort: { anyOf: [{ type: "string" }, { type: "null" }] }, page: { oneOf: [{ type: "integer" }, { type: "string" }] },
        meta: { type: "object", additionalProperties: { type: "string" } }, extra: { type: "object", additionalProperties: true },
        none: { type: "object", additionalProperties: false }, kind: { const: "pr" }, pair: { type: "array", items: [{ type: "string" }, { type: "number" }] },
        tuple: { type: "array", prefixItems: [{ type: "boolean" }] }, any: {}, nullable: { type: "string", nullable: true },
        multi: { type: ["string", "null", "array"] }, ref: { $ref: "#/definitions/Label" }, both: { allOf: [{ type: "object", properties: { a: { type: "string" } } }, { type: "object", properties: { b: { type: "number" } } }] },
        mixed: { enum: [1, "two", null, true, { x: 1 }] }, never: false, always: true, "1st": { type: "number" },
      }, required: ["owner", "repo", "never"], definitions: { Label: { type: "object", properties: { name: { type: "string" } } } } }, outputSchema: { type: "object", properties: { items: { type: "array", items: { type: "object", properties: { number: { type: "integer" }, title: { type: "string" } }, required: ["number"] } } } } }],
      ["delete", { inputSchema: { type: "object", properties: { id: { type: "integer" } } } }],
      ["3d-render", { description: "   ", inputSchema: { type: "string" } }],
    ] },
    { name: "my_docs", tools: [["search", { description: "Search the docs.", inputSchema: { type: "object", properties: { q: { type: "string" } }, required: ["q"] } }]] },
  ] },
];
function codeTool(spec) {
  const typeBlocks = spec.namespaces.map((ns) => {
    const tools = {};
    for (const [k, d] of ns.tools) tools[k] = d;
    return generateTypesFromJsonSchema(tools).replace("declare const codemode: {", `declare const ${ns.name}: {`);
  });
  const typeBlock = typeBlocks.filter(Boolean).join("\n\n");
  return { types: typeBlock, description: (spec.description ?? DEFAULT_DESCRIPTION).replace("{{types}}", typeBlock) };
}

// Large values are described by a generator the Rust test rebuilds (see tests/golden.rs).
function make(gen) {
  switch (gen.kind) {
    case "big": return { items: Array.from({ length: gen.n }, (_, i) => ({ id: i, title: `Item ${i} ` + "x".repeat(i % 50), tags: ["a", "b"] })), note: "y".repeat(gen.note) };
    case "repeat": return gen.text.repeat(gen.n);
    case "range": return Array.from({ length: gen.n }, (_, i) => i * gen.step);
    case "value": return gen.value;
  }
}
const truncateCases = [
  [{ kind: "value", value: "short" }, 100], [{ kind: "repeat", text: "z", n: 30000 }], [{ kind: "repeat", text: "zé\n", n: 2000 }, 1000],
  [{ kind: "big", n: 400, note: 3000 }], [{ kind: "big", n: 400, note: 3000 }, 2000], [{ kind: "big", n: 60, note: 10 }, undefined, 500],
  [{ kind: "value", value: [1, 2, 3] }], [{ kind: "range", n: 3000, step: 1000 }, 1000],
  [{ kind: "value", value: { a: "q".repeat(9000), b: 1, c: "short" } }, 1500], [{ kind: "value", value: null }], [{ kind: "value", value: 12345 }, 1000],
];
const out = {
  source: "@cloudflare/codemode 0.5.6 (cloudflare/agents 4116a8bdf97660c8e79a923b6aec140256282a5d)",
  default_description: DEFAULT_DESCRIPTION,
  normalize: normalizeInputs.map((i) => [i, normalizeCode(i)]),
  sanitize: sanitizeInputs.map((i) => [i, sanitizeToolName(i)]),
  code_tools: toolSets.map((spec) => ({ spec, ...codeTool(spec) })),
  truncate: truncateCases.map(([gen, maxChars, maxTokens]) => {
    const opts = {};
    if (maxChars !== undefined) opts.maxChars = maxChars;
    if (maxTokens !== undefined) opts.maxTokens = maxTokens;
    return { gen, maxChars: maxChars ?? null, maxTokens: maxTokens ?? 6000, out: JSON.stringify(truncateResult(make(gen), opts)) };
  }),
};
writeFileSync(process.argv[3], JSON.stringify(out, null, 1) + "\n");
console.log("wrote", process.argv[3]);
