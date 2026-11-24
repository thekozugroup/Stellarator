// What an OpenAI-compatible endpoint receives from Cloudflare's createCodeTool through the AI SDK:
// the code tool's definition and, for each kind of execution outcome, the tool message. Code mode's
// rendering (js/stellarator.js, `render` and `invalidInput`) reproduces these; this was checked with
// ai 7.0.181, @ai-sdk/openai-compatible 3.0.94, zod 4.6.7 and @cloudflare/codemode 0.5.6.
//
//   mkdir cf && cd cf && npm init -y
//   npm install @cloudflare/codemode@0.5.6 ai@7 zod@4 @ai-sdk/openai-compatible
//   node ai-sdk.mjs cf ok|nologs|undef|err|errnolog|throw [tools] ['<arguments>'] [tool name]
//
// createCodeTool is reproduced from packages/codemode/src/tool.ts: its module also loads the
// Workers-only connector base, which Node cannot import. The executor returns canned results.
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
const root = resolve(process.argv[2]);
const require = createRequire(root + "/package.json");
const dist = pathToFileURL(root + "/node_modules/@cloudflare/codemode/dist/").href;
const jst = await import(dist + "json-schema-types-D_m9tVnI.js");
const { t: runCode } = await import(dist + "run-code-Q3Jh6Dcu.js");
const { t: DEFAULT_DESCRIPTION } = await import(dist + "shared-SMjV5yN4.js");
const ai = await import(pathToFileURL(require.resolve("ai")).href);
const { z } = await import(pathToFileURL(require.resolve("zod")).href);
const { createOpenAICompatible } = await import(pathToFileURL(require.resolve("@ai-sdk/openai-compatible")).href);

const descriptors = {
  lookup: { description: "Look up a term in the reference catalog and return what it says.", inputSchema: { type: "object", properties: { query: { type: "string", description: "The term to look up." } }, required: ["query"] } },
  add: { description: "Add two numbers.", inputSchema: { type: "object", properties: { a: { type: "number" }, b: { type: "number" } }, required: ["a", "b"] } },
};
// generateTypes(tools, "mock") on jsonSchema() inputs: generateTypesFromJsonSchema with the namespace.
const types = jst.t(descriptors).replace("declare const codemode:", "declare const mock:");

const scenario = process.argv[3] ?? "ok";
const executor = {
  execute: async () => {
    if (scenario === "ok") return { result: { sum: 9, note: "é   <tag>" }, logs: ["hello", "[warn] careful"] };
    if (scenario === "nologs") return { result: "plain string", logs: [] };
    if (scenario === "undef") return { result: undefined, logs: [] };
    if (scenario === "err") return { result: undefined, error: "boom", logs: ["a log"] };
    if (scenario === "errnolog") return { result: undefined, error: "x is not defined", logs: [] };
    throw new Error("Unexpected token");
  },
};
const codeSchema = z.object({ code: z.string().describe("JavaScript async arrow function to execute") });
const codemode = ai.tool({
  description: DEFAULT_DESCRIPTION.replace("{{types}}", types),
  inputSchema: codeSchema,
  execute: async ({ code }) => runCode({ code, executor, providers: [{ name: "mock", fns: {} }] }),
});
const bodies = [];
const fetch = async (_url, init) => {
  bodies.push(JSON.parse(init.body));
  const first = bodies.length === 1;
  const message = first
    ? { role: "assistant", content: null, tool_calls: [{ id: "call_1", type: "function", function: { name: process.argv[6] ?? "codemode", arguments: process.argv[5] ?? JSON.stringify({ code: "async () => 1" }) } }] }
    : { role: "assistant", content: "done" };
  return new Response(
    JSON.stringify({ id: "x", object: "chat.completion", created: 0, model: "m", choices: [{ index: 0, message, finish_reason: first ? "tool_calls" : "stop" }], usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 } }),
    { headers: { "content-type": "application/json" } },
  );
};
const provider = createOpenAICompatible({ name: "x", baseURL: "http://localhost/v1", fetch });
await ai.generateText({ model: provider.chatModel("m"), prompt: "hi", tools: { codemode }, stopWhen: (ai.stepCountIs ?? ai.isStepCount)(3) });
console.log(JSON.stringify(process.argv[4] === "tools" ? bodies[0].tools : bodies[1].messages.slice(1)));
