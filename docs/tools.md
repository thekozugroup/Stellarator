# Tools: MCP servers in runs

A run can give the model tools from MCP (Model Context Protocol) servers. The model calls them,
Stellarator runs each call on its MCP server, sends the result back, and the model continues until
it answers. Every step is stored with the sample and exported in the formats trainers read.

- [Why record tool use](#why-record-tool-use)
- [Add MCP servers](#add-mcp-servers)
- [Use tools in a run](#use-tools-in-a-run)
- [What happens during a run](#what-happens-during-a-run)
- [Endpoint requirements](#endpoint-requirements)
- [Exports](#exports)
- [Code mode](#code-mode)
- [Tools run for real](#tools-run-for-real)
- [Stdio servers in server mode](#stdio-servers-in-server-mode)
- [Try it with the mock](#try-it-with-the-mock)
- [API](#api)

## Why record tool use

A model learns to use tools from examples of tool use. To learn when to call a tool, which
arguments to pass and how to use the result, a student model needs the teacher's actual calls and
the actual results in its training data. Stellarator therefore runs the tools during generation
(nothing is simulated) and keeps the whole exchange for each sample:

- every assistant turn: its thinking trace, its text and its tool calls;
- every tool result, as sent back to the model, including errors;
- the final answer.

The sample's `reasoning` and `response` are the final answer's, so screens and exports that only
know those two fields keep working.

## Add MCP servers

Open **MCP servers** and paste the configuration you already use in another app. Stellarator reads
these shapes:

| Source | Shape |
| --- | --- |
| Claude Desktop, Claude Code, Cursor, Windsurf, Cline | `{"mcpServers": {"name": {...}}}` |
| VS Code (`mcp.json`, or `settings.json` under `"mcp"`) | `{"servers": {"name": {...}}}` |
| One server | `{"command": ...}` or `{"url": ...}`, named by its `name` field or its package or host |
| The inner part only | `{"name": {...}, ...}`, also without the outer braces |

Each entry may have `type` (`stdio`, `http`, `streamable-http` or `sse`), `command`, `args`,
`env`, `url` (Windsurf's `serverUrl` works too) and `headers`. Without `type`, a `command` means a
stdio server and a `url` an HTTP server. `disabled: true` saves the server turned off. Comments and
trailing commas are allowed. For example:

```json
{
  "mcpServers": {
    "github": {
      "command": "docker",
      "args": ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN", "ghcr.io/github/github-mcp-server"],
      "env": {"GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_..."}
    },
    "docs": {"type": "http", "url": "https://docs.example.com/mcp", "headers": {"Authorization": "Bearer ..."}}
  }
}
```

The import reports, per entry, what it saved and what it could not:

- A name already in use gets a suffix: `github` becomes `github-2`.
- Names may only use letters, digits, `_` and `-`; other characters become `-`.
- `"type": "sse"` is saved as streamable HTTP. Stellarator does not speak the legacy SSE transport
  (the official Rust SDK it uses dropped it). Most servers that offer `/sse` also offer streamable
  HTTP, usually at `/mcp`; if the check fails, change the URL.
- Values such as `${input:token}` (VS Code) are placeholders: edit the server and enter the real
  value.

Then **Check** each server. A check starts or reaches the server, completes the MCP handshake,
lists its tools and saves the list, with the server's name and version. A failed check says why,
including the end of a stdio server's error output.

**Secrets.** Environment variables (stdio) and headers (HTTP) are treated as secrets: the API
returns their names and the last four characters only, like endpoint API keys. They are stored
unencrypted in the data folder. When you edit a server, names you leave unchanged keep their saved
values; enter a new value to replace one.

**Transports.**

| Transport | How Stellarator reaches it |
| --- | --- |
| `stdio` | Starts `command` with `args` and talks over stdin and stdout. The command is looked up on `PATH`, so `npx` and `uvx` work, also as `npx.cmd` on Windows. The server gets the server's `env` plus a short list of Stellarator's own variables: `PATH`, `HOME`, `USER`, `SHELL`, `TERM`, locale and temp folder, Windows' profile and system folders, and proxy and certificate settings. Other variables, such as `HF_TOKEN` or API keys, are not passed: add what a server needs under its `env`. |
| `http` | Streamable HTTP at `url`, with `headers` on every request. |

## Use tools in a run

In **New run**, turn on **Tools** and pick servers. For each server, offer all of its tools or pick
some. These settings belong to the run:

| Setting | Default | Range | Meaning |
| --- | --- | --- | --- |
| Servers and tools | none | at least one server | All tools of a server, or the ones picked |
| `max_rounds` | 8 | 1 to 32 | Rounds of tool calls per sample. The model gets at most `max_rounds + 1` requests |
| `tool_timeout_s` | 60 | 1 to 3600 | Seconds one call may take; a slower call returns an error result to the model |
| `max_result_chars` | 20,000 | 1,000 to 1,000,000 | Longer results are cut and end with `[truncated: N more characters]` |
| `tool_choice` | `auto` | `auto`, `required` | `required`: the first request of each sample must call a tool; later requests use `auto`, so the model can answer |

When the run is created, Stellarator checks each selected server and stores the exact tool
definitions the run sends (OpenAI function format) and the servers they come from. The run screen
shows them. **Run again** copies the selection, not the old definitions: a new run takes the tools
the servers offer at that moment.

**Tool names.** Each tool keeps its own name. When two selected servers have a tool with the same
name, both are sent as `<server>__<tool>` (for example `github__search` and `docs__search`). Names
must match `^[a-zA-Z0-9_-]{1,64}$`, which OpenAI and most servers require; a run whose names cannot
fit is refused, with the names to leave out.

## What happens during a run

For each sample:

1. Stellarator sends the prompt with `tools` and `tool_choice`.
2. If the answer calls tools, it runs them on their servers, in parallel (up to 8 at a time).
3. It sends the assistant message (its text and its tool calls, without its thinking trace) and one
   `tool` message per call, then the next request.
4. This repeats until an answer has no tool calls, or `max_rounds` rounds have run.

Rules:

| Situation | What happens |
| --- | --- |
| The arguments are not a JSON object, or the tool does not exist | The error text goes back to the model as the tool's result (`is_error`). This is part of the trace, not a failure. |
| The tool reports an error (`isError`) or times out | The same: the error text is the result. |
| The answer after the last round still calls tools | The sample ends as truncated, with finish reason `tool_rounds`; those calls do not run. |
| The answer stops at `max_tokens` | Finish reason `length`, truncated; its calls do not run. |
| An MCP server's connection fails (it crashed or went down) | Stellarator reconnects once (a stdio server is started again) and repeats the call. If that fails too, the sample fails with the server's name; **Retry failed** runs it again later. |
| The endpoint refuses the tool definitions (a 400 naming tools or `tool_choice`) | The run pauses and says the endpoint must support tool calling. No sample fails. |

- Earlier thinking traces are not sent back to the model; they are kept in the trace.
- Text parts of a result are joined with newlines. Images, audio and binary resources become a
  short placeholder, such as `[image: image/png, 12 KB]`: the model gets text only.
- Token usage (prompt, completion and reasoning tokens) is the sum over the sample's requests.
- The run holds one session per server, shared by all its samples. Sessions open when the run
  starts or resumes and close when it pauses, finishes or is deleted. A stdio server runs as one
  process per run.
- While tools run, the in-flight list shows the sample as `tool` with the tools' names, and which
  request of the sample it is on.

## Endpoint requirements

The endpoint must return tool calls as `tool_calls` in its answers, which needs tool calling turned
on in the server and a model trained for it.

| Server | What to do |
| --- | --- |
| vLLM | Start with `--enable-auto-tool-choice --tool-call-parser <parser>`. The parser follows the model: `hermes` for Qwen2.5, QwQ and Qwen3, `qwen3_xml` for Qwen3-Coder, `llama3_json` for Llama 3.1 to 4, `mistral` for Mistral, `deepseek_v3` or `deepseek_v31` for DeepSeek, `openai` for gpt-oss, `granite` or `granite4` for Granite, `glm45` for GLM-4.5 and 4.6, `kimi_k2` for Kimi K2. The full list is in vLLM's [tool calling docs](https://docs.vllm.ai/en/latest/features/tool_calling.html). Add `--reasoning-parser` as usual to keep the thinking trace separate. `tool_choice: required` needs vLLM 0.8.3 or later. |
| SGLang | Start with `--tool-call-parser` for the model (for example `qwen25`, `llama3`, `mistral`, `deepseekv3`, `gpt-oss`). |
| Ollama | Use a model with tool support (its library page lists "tools"). Others answer 400 "does not support tools". |
| LM Studio | Use a model with tool support; LM Studio's OpenAI-compatible API takes `tools`. |
| llama.cpp `llama-server` | Start with `--jinja` so the model's chat template handles tools. |
| OpenRouter | Pick a model that lists tools among its supported parameters. Add `{"provider": {"require_parameters": true}}` to the run's extra body so requests only go to providers that support them. |
| Hosted OpenAI-compatible APIs | Most support `tools`; check the model's page. |

A server without tool calling usually answers 400. Stellarator then pauses the run with a message
naming the vLLM flags and this page; resume after restarting the server with tool calling. A server
that accepts `tools` but writes the calls as text (no parser) returns no `tool_calls`: the text is
stored as the answer and no tool runs. If samples finish without tool calls, look at their
responses for text like `<tool_call>`.

## Exports

For runs with tools, the exports carry the whole trace and the tool definitions.

| Format | Record | Read by |
| --- | --- | --- |
| Chat messages | `{"messages": [...], "tools": [...]}`: the prompt, each assistant turn with `tool_calls` (OpenAI shape, arguments as a JSON object), a `{"role": "tool", "tool_call_id", "name", "content"}` message per result, the final answer | TRL `SFTTrainer`, Axolotl (`type: chat_template`), Hugging Face chat templates |
| ShareGPT | `conversations` with `function_call` and `observation` turns, `tools` as a JSON string | LLaMA-Factory |
| Raw | Every field, plus `tool_calls`, the stored `trace` and `tools` | your own pipeline |
| DPO, KTO (conversational) | `chosen`, `rejected` and `completion` are the whole trajectories (assistant turns, tool messages, final answer), plus `tools` | TRL `DPOTrainer`, `KTOTrainer` |
| Alpaca, prompt-completion, text DPO and KTO | Refused for runs with tools | |

Alpaca and plain-text records have no place for tool calls and results. Prompt-completion records
would put the tool results in the completion, where a trainer learns to write them; use chat
messages with assistant-only loss instead. The export dialog answers these with a message that
names the format to use.

**The reasoning placement** applies to every assistant turn: think tags around each turn's
thinking (including turns that only call tools), a `reasoning_content` or `thinking` field on each
assistant message, or left out. In ShareGPT, thinking goes in think tags before the call;
`reasoning_content` and `thinking` keep only the final answer's thinking in the record's
`reasoning` field, which LLaMA-Factory does not read.

A chat messages record (JSONL, think tags, shortened):

```json
{"messages": [
  {"role": "user", "content": "What are the tide times in Brest, and what is 4 + 5?"},
  {"role": "assistant", "content": "<think>\nI need the tide table and a sum.\n</think>\n\n",
   "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": {"query": "tides Brest"}}},
                  {"id": "call_2", "type": "function", "function": {"name": "add", "arguments": {"a": 4, "b": 5}}}]},
  {"role": "tool", "tool_call_id": "call_1", "name": "lookup", "content": "High tide 06:12, low tide 12:31."},
  {"role": "tool", "tool_call_id": "call_2", "name": "add", "content": "9"},
  {"role": "assistant", "content": "<think>\nBoth results are in.\n</think>\n\nHigh tide is at 06:12; 4 + 5 = 9."}
 ],
 "tools": [{"type": "function", "function": {"name": "lookup", "description": "...", "parameters": {...}}}, ...]}
```

**TRL.** `SFTTrainer` passes the `tools` column to the tokenizer's chat template, as a list or as a
JSON string. Use `assistant_only_loss=True` (with a chat template that marks assistant turns) so
the model learns the calls and answers, not the tool results. DPO and KTO take the conversational
records with their `tools` column. Loading JSONL with arbitrary tool arguments needs `datasets` 4.7
or later (its `Json()` type); with an older version, export Parquet.

**Axolotl.** Use `type: chat_template` with the default `field_messages: messages`; the `tools`
column is read by default (`field_tools: tools`). Check that your chat template supports tools and
which role it expects for results (`tool` for most templates).

**LLaMA-Factory.** Register the ShareGPT export with the tools column:

```json
"my_tool_traces": {
  "file_name": "my-run-sharegpt.jsonl",
  "formatting": "sharegpt",
  "columns": {"messages": "conversations", "tools": "tools"}
}
```

Parallel calls are one `function_call` turn holding a JSON list, and their results one
`observation` turn, joined with `</tool_response><tool_response>` as LLaMA-Factory's own converter
joins them. Text an assistant wrote next to its tool calls is left out of the `function_call` turn,
because LLaMA-Factory reads that turn as JSON.

**Parquet and CSV.** A Parquet column has one type, and tool arguments and definitions differ from
tool to tool. Stellarator writes them as JSON text in columns marked with Arrow's `arrow.json`
type: `datasets` 4.7 and later read them back as objects, older readers as strings (TRL accepts a
`tools` string). Arguments the model wrote as invalid JSON are kept as a JSON string. In CSV, nested
values are JSON text, as for other formats.

The Hugging Face push and the export preview use the same records. The dataset card lists the
tools and their servers.

## Code mode

In code mode the model gets one tool instead of one per MCP tool. It writes JavaScript that calls
the MCP tools; Stellarator runs the code in a sandbox and sends back only what the code returns and
logs. This is Code Mode as Cloudflare defines it in
[`@cloudflare/codemode`](https://developers.cloudflare.com/agents/tools/codemode/) 0.5.6
(github.com/cloudflare/agents, commit `4116a8bdf97660c8e79a923b6aec140256282a5d`), in either of the
two setups its documentation describes. The traces reproduce what a model sees and writes there, so
a student trained on them works in that setup. Every message is compared with the real package
running on workerd; [How close it is to Cloudflare's](#how-close-it-is-to-cloudflares) has the
numbers and what differs.

### When it helps

| Situation | Why code mode helps |
| --- | --- |
| Many tools (dozens of MCP tools) | The model reads the tools as TypeScript declarations, or looks up only the ones it needs |
| Large intermediate results | A listing the code filters or counts stays in the sandbox; only the code's result reaches the model |
| Chained calls (look up, then act on the result) | One execution makes every call; the model does not pay a request per round |
| Loops and retries | The code loops over inputs or catches a failing call itself |

For a run where each sample makes one or two small calls, direct mode is as good and simpler.

### Two formats

`params.tools.code.format` picks which Cloudflare setup the run reproduces.

| | `code_tool` (the default) | `runtime` |
| --- | --- | --- |
| Cloudflare's setup | `createCodeTool({ tools, executor })`, one tool provider per MCP server | `createCodemodeRuntime({ ctx, executor, connectors }).tool()`, one `McpConnector` per MCP server |
| Cloudflare's page | [AI SDK integration](https://developers.cloudflare.com/agents/tools/codemode/ai-sdk/): "Create a stateless Code Mode tool", "Organize tools with providers" | [Durable runtime](https://developers.cloudflare.com/agents/tools/codemode/durable-runtime/) and [Use MCP tools with Code Mode](https://developers.cloudflare.com/agents/tools/codemode/mcp/); the overview lists this setup first |
| The tool's description | Every tool as TypeScript declarations | A workflow, rules and the connectors' names; no declarations |
| How the model learns a tool's input | It reads the description | It runs `codemode.search("...")` and `codemode.describe("connector.method")` first |
| The tool's result | `{"result": ..., "logs": [...]}`, or an error as text | `{"status": "completed", "executionId": ..., "result": ..., "logs": [...]}`, or `{"status": "error", ...}` |
| Result size | Shrunk to `max_result_tokens` (6,000 by default) | Not shrunk by default, as the runtime; console output is shrunk at 6,000 tokens |
| Prompt size | Grows with the number of tools | Stays the same; finding tools takes extra rounds |
| A sample's rounds | Often one execution | Often three: search, describe, call |

Pick `runtime` when the student will run behind `createCodemodeRuntime`, when the servers offer
many tools, or when you want the setup Cloudflare's MCP page builds. Pick `code_tool` when the
student will run behind `createCodeTool`, or when the tools are few and one execution per answer
matters more than prompt size. A student trained on one format does not know the other: the
description, the way tools are found and the result shape all differ.

### Turn it on

In **New run** turn on **Tools**, pick servers, and choose code mode. In the API, set
`params.tools.mode` to `"code"`. The other tool settings keep their meaning: `max_rounds` counts
rounds of code tool calls, `tool_timeout_s` limits each MCP call the code makes, `tool_choice`
applies to the code tool, and `max_result_chars` only caps the copy of each MCP result stored in
the trace (the code always gets the whole result). The `runtime` format needs more rounds than
`code_tool`; the default `max_rounds` of 8 leaves room for the three-step workflow.

The code mode settings, `params.tools.code`:

| Setting | Default | Range | Meaning |
| --- | --- | --- | --- |
| `format` | `code_tool` | `code_tool`, `runtime` | The setup, as above |
| `tool_name` | `codemode` | 1 to 64 letters, digits, `_`, `-` | The tool's name (Cloudflare's docs register both setups' tools as `tools: { codemode }`) |
| `description` | null: Cloudflare's | up to 100,000 characters | `code_tool`: the description template; the first `{{types}}` is replaced by the generated declarations. `runtime`: the whole description, as the `description` option of `runtime.tool()` |
| `namespaces` | `servers` | `servers`, `codemode` | `code_tool` only. `servers`: one global per server, named after it (`github.list_issues(...)`). `codemode`: every tool under `codemode` (`codemode.list_issues(...)`); tools of the same name on two servers become `<server>__<tool>`. The `runtime` format always has one connector per server |
| `timeout_s` | 60 | 1 to 3600 | Seconds one execution may take, its MCP calls included |
| `max_result_tokens` | null | null, 0, or 100 to 1,000,000 | The budget of the result the model sees, at 4 characters per token; a result over it is shrunk as Cloudflare's `truncateResult` does it. 0: no truncation. Null is the format's default: 6,000 for `code_tool` (console output gets the same budget), no truncation for `runtime` |
| `max_result_chars` | null | 1,000 to 4,000,000 | The same budget in characters; overrides `max_result_tokens * 4` |
| `memory_limit_mb` | 128 | 8 to 1024 | Memory one execution may use (a Workers isolate's limit is 128 MB) |
| `connector_hints` | none | up to 2,000 characters each, one line | `runtime` only: a hint per server, shown next to its connector in the description (`` - `github` — Issues and pull requests. ``), as the `connectorHints` option of `runtime.tool()`. Keys are server names |
| `server_instructions` | true | true, false | `runtime` only: whether `codemode.describe("<connector>")` starts with the `instructions` the MCP server sent. True is what `McpConnector` does when the connection has instructions, which the Agents SDK's connections do |

Two defaults are Stellarator's and not Cloudflare's. `createCodeTool` never shrinks a result; set
`max_result_tokens` to 0 in the `code_tool` format to send results whole as it does. And Cloudflare
sets no memory limit itself; Workers gives an isolate 128 MB.

Names become JavaScript identifiers with Cloudflare's `sanitizeToolName`: `my-server` becomes
`my_server`, `3d-render` becomes `_3d_render`, `delete` becomes `delete_`. A run is refused, with the
names to change, when two servers or two tools of a server would get the same name, or when a
server's name would hide a global the sandbox needs (`console`, `Promise`, `Object` and a few
more); in the `code_tool` format `namespaces: "codemode"` avoids both. In the `runtime` format a
server cannot be named `codemode`, which is the runtime's own global.

The run stores the code tool as sent (`RunOut.tools.definitions`, one definition) and, in
`RunOut.tools.code`, the format (`kind`), the generated types and which MCP tool each method calls.
For the `runtime` format it also stores `connectors`: each connector's name, hint, instructions and
tools with their schemas, which is what `codemode.search` and `codemode.describe` answer from.

### What the model sees: `code_tool`

The request's `tools` holds one function, as the AI SDK sends `createCodeTool`'s tool:

```json
{"type": "function", "function": {
  "name": "codemode",
  "description": "Execute code to achieve a goal.\n\nAvailable:\ntype LookupInput = {\n    /** The term to look up. */\n    query: string;\n}\ntype LookupOutput = unknown\ntype AddInput = {\n    a: number;\n    b: number;\n}\ntype AddOutput = unknown\n\ndeclare const mock: {\n\t/**\n\t * Look up a term in the reference catalog and return what it says.\n\t * @param input.query - The term to look up.\n\t */\n\tlookup: (input: LookupInput) => Promise<LookupOutput>;\n\n\t/**\n\t * Add two numbers.\n\t */\n\tadd: (input: AddInput) => Promise<AddOutput>;\n}\n\nWrite an async arrow function in JavaScript that returns the result.\nDo NOT use TypeScript syntax — no type annotations, interfaces, or generics.\nDo NOT define named functions then call them — just write the arrow function body directly.\n\nExample: async () => { const r = await codemode.searchWeb({ query: \"test\" }); return r; }",
  "parameters": {"$schema": "http://json-schema.org/draft-07/schema#", "type": "object",
                 "properties": {"code": {"type": "string", "description": "JavaScript async arrow function to execute"}},
                 "required": ["code"], "additionalProperties": false}}}
```

The types come from each tool's JSON Schema, by Cloudflare's `jsonSchemaToType`: property
descriptions become `/** ... */` comments, optional properties get `?`, enums become unions of
literals, arrays `T[]`, nested objects nested types, and schemas it does not handle `unknown`. A tool
whose server declares an output schema gets a typed result instead of `unknown`. Each namespace is
one block, in the order the servers were picked.

The model calls the tool with its code, for example
`{"code": "async () => {\n  const s = await mock.add({ a: 4, b: 5 });\n  console.log(\"sum\", s);\n  return s;\n}"}`.
Before it runs, Cloudflare's `normalizeCode` strips a markdown fence and wraps code that is not one
arrow function: `` ```js\nconst x = 1;\nx\n``` `` runs as `async () => {\nconst x = 1;\nreturn (x)\n}`.

The tool message the model gets back, as `createCodeTool` returns it and the AI SDK sends it:

| Outcome | Tool message content |
| --- | --- |
| The code returned | `{"result":9,"logs":["sum 9"]}`: `JSON.stringify` of `{result, logs}`; `logs` is left out when nothing was logged, and an undefined result leaves `{}` |
| The code threw (also an MCP error it did not catch, or a ReferenceError) | `Error: Code execution failed: <message>`, then `\n\nConsole output:\n` and the logs when there are any |
| Over the timeout | `Error: Code execution failed: Execution timed out` |
| Over the memory limit | `Error: Code execution failed: out of memory` |
| Not valid JavaScript | `Error: Failed to start Worker:\nUncaught SyntaxError: <message>\n  at executor.js:<line>:<column>`, as workerd reports a module that does not load |
| The input is not `{"code": string}` | The AI SDK's error, such as `AI_InvalidToolInputError: Invalid input for tool codemode: AI_TypeValidationError: Type validation failed: ...` |
| The model called another tool | `AI_NoSuchToolError: Model tried to call unavailable tool 'x'. Available tools: codemode.` |

`console.log` lines are the arguments joined with spaces (`String(arg)`), `console.warn` lines
start with `[warn] `, `console.error` lines with `[error] `; other console methods write nothing,
as in Cloudflare's executor. A result over the budget is shrunk structurally (the largest values
first, with `--- TRUNCATED ---` markers, still valid JSON), and a long text ends with
`--- TRUNCATED ---\nResponse was ~N tokens (limit: 6,000). Narrow the request to reduce response size.`
The console output inside an error text is never shortened, as in Cloudflare's.

### What the model sees: `runtime`

The request's `tools` holds one function, as the AI SDK sends the tool of `runtime.tool()`. Its
parameters are `{"type": "object", "properties": {"code": {"type": "string"}}, "required": ["code"], "additionalProperties": false}`.
Its description, for a run with the servers `mock` and `tracker`:

```text
Execute JavaScript in a sandbox with access to connector SDKs.

## Workflow

1. `const matches = await codemode.search("short intent phrase");`
2. `const docs = await codemode.describe(matches.results[0].path);`
3. Call the method: `await <connector>.<method>(args);`

## Rules

- The ONLY globals are `mock`, `tracker` and `codemode` (plus standard JavaScript). There is no `host`, `fs`, `require`, `process`, or Node.js API — all I/O goes through the connectors below.
- Never guess method names. If you have not used a connector in this conversation, run a discovery pass first: `codemode.search(query)` returns ranked matches across connector methods and saved snippets.
- `codemode.describe("connector.method")` returns TypeScript type declarations.
- `codemode.step(name, fn)` wraps side-effectful or nondeterministic work (raw fetch, random, time) so it runs once and is replayed on resume. Use it for anything that isn't a connector call.
- Some methods require approval. The run pauses until the user approves, then resumes automatically. Write code as if the call returns normally.
- A result with `status: "paused"` means the run is awaiting human approval. Tell the user what is pending and wait — do NOT re-issue the code; the run resumes on its own once approved.
- All code outside connector calls and `codemode.step` must be deterministic so resume can replay it.
- Do not use `fetch` — use connector SDKs.

## Snippets

Snippets are saved scripts you can reuse.
- `codemode.run("name", input)` runs a saved snippet. Snippets appear in `codemode.search` results.
- If a script may be saved as a snippet later, write it as `async (input) => { ... }` so it can take input.

## Available connectors

- `mock`
- `tracker`
```

The text is the package's own (`buildDescription`), so it mentions approvals and snippets even
though a run has none; a deployment with no approvals and no snippets sends the same text. With
`connector_hints`, a line reads `` - `tracker` — Issues of the project. ``

What the code can call, and what comes back, recorded from the package:

| Call | Returns |
| --- | --- |
| `codemode.search("issue")` | `{"results":[{"path":"tracker.create_issue","connector":"tracker","method":"create_issue","description":"Create an issue.","kind":"method","score":295}, ...],"total":5,"truncated":false}`: the methods whose path, connector, method name or description match the query's words, best first, at most 50. A query of one or two words must match in full; a longer one needs 60% of its words. No match: `{"results":[],"total":0,"truncated":false}` |
| `codemode.describe("mock.add")` | `{"path":"mock.add","description":"Add two numbers.","types":"type AddInput = {\n    a: number;\n    b: number;\n}\ntype AddOutput = unknown","kind":"method"}` |
| `codemode.describe("tracker")` | `{"path":"tracker","description":"<the server's instructions>","types":"<the instructions>\n\n<every input and output type>\n\ndeclare const tracker: {...}","kind":"connector"}`; without instructions, `description` is left out and `types` starts with the types |
| `codemode.describe("github.nope")` | `{"path":"github.nope","types":"\"github.nope\" not found.","kind":"method"}` |
| `tracker.list_issues({ limit: 2 })` | What `McpConnector` returns: the same unwrapped MCP result as in the `code_tool` format |
| `codemode.step("name", fn)` | What `fn` returns. It runs once; nothing pauses a run, so nothing is replayed |
| `codemode.run("name")` | `{"error":"Snippet \"name\" not found."}`: a run has no saved snippets |

The tool message the model gets back, as the runtime's tool returns it through the AI SDK:

| Outcome | Tool message content |
| --- | --- |
| The code returned | `{"status":"completed","executionId":"exec_0001795544688032_22ce4512-9ab8-40af-9a59-d2b20cbc05b8","result":9,"logs":["sum 9"]}`; `logs` is left out when nothing was logged, `result` when the code returned undefined |
| The code threw (also an MCP error it did not catch) | `{"status":"error","executionId":"...","error":"Code execution failed: <message>"}`; with console output, the error text continues with `\n\nConsole output:\n` and the logs |
| A global that does not exist | `...,"error":"Code execution failed: fs is not defined (the only globals available in the sandbox are: mock, tracker, codemode)"}` |
| A method the connector does not have | `...,"error":"Code execution failed: Tool \"nope\" not found on mock"}` |
| Over the timeout | `...,"error":"Code execution failed: Execution timed out"}` |
| Not valid JavaScript | `...,"error":"Failed to start Worker:\nUncaught SyntaxError: <message>\n  at executor.js:<line>:<column>"}` |
| A tool's result or arguments over 1,000,000 bytes | `...,"error":"The result of mock.repeat is too large to record durably (1000002 bytes > 1000000 byte limit). Write large data to a file or workspace instead and pass/return a small reference (such as a path)."}` |
| The input is not `{"code": string}`, or the model called another tool | The AI SDK's errors, as in the `code_tool` format |

An execution id is `exec_`, the time in milliseconds padded to 16 digits, `_` and a UUID; every
execution gets a new one. A training example therefore holds ids that nothing later refers to,
which is also what a model in production sees.

A sample in this format, as exported to chat messages (reasoning left out, results shortened):

```json
{"messages": [
  {"role": "user", "content": "list issues"},
  {"role": "assistant", "content": "", "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "codemode",
    "arguments": {"code": "async () => codemode.search(\"list issues\")"}}}]},
  {"role": "tool", "tool_call_id": "call_1", "name": "codemode",
   "content": "{\"status\":\"completed\",\"executionId\":\"exec_0001795545777819_5f2740a0-d27c-44dd-ac5f-5da44369e40f\",\"result\":{\"results\":[{\"path\":\"tracker.list_issues\", ...}],\"total\":5,\"truncated\":false}}"},
  {"role": "assistant", "content": "", "tool_calls": [{"id": "call_2", "type": "function", "function": {"name": "codemode",
    "arguments": {"code": "async () => codemode.describe(\"tracker.list_issues\")"}}}]},
  {"role": "tool", "tool_call_id": "call_2", "name": "codemode",
   "content": "{\"status\":\"completed\",\"executionId\":\"exec_0001795545777963_a458b248-5c96-4c61-bb2b-b480344d391a\",\"result\":{\"path\":\"tracker.list_issues\",\"description\":\"List issues, newest first. Filter by state or label.\",\"types\":\"type ListIssuesInput = {...}\",\"kind\":\"method\"}}"},
  {"role": "assistant", "content": "", "tool_calls": [{"id": "call_3", "type": "function", "function": {"name": "codemode",
    "arguments": {"code": "async () => {\n  const result = await tracker.list_issues({ limit: 3 });\n  return result;\n}"}}}]},
  {"role": "tool", "tool_call_id": "call_3", "name": "codemode",
   "content": "{\"status\":\"completed\",\"executionId\":\"exec_0001795545778145_59468324-de34-4f0d-8ebf-b66486c9b38f\",\"result\":{\"issues\":[...],\"total\":333}}"},
  {"role": "assistant", "content": "There are 333 open issues. The newest three are ..."}
 ],
 "tools": [{"type": "function", "function": {"name": "codemode", "description": "Execute JavaScript in a sandbox with access to connector SDKs. ...", "parameters": {...}}}]}
```

What the durable runtime adds for a deployment, and a run does not simulate:

| Feature of the runtime | In a run |
| --- | --- |
| Approvals (`requiresApproval` on a method pauses the execution; the result has `status: "paused"`) | No method requires approval, so nothing pauses and no result is `paused` |
| Replay after a pause | Nothing pauses, so each execution runs once |
| Saved snippets (`codemode.run`, snippets in search results) | None are saved; `codemode.run` answers that the snippet was not found |
| Rollback (`rollbackExecution`, a connector's `revert`) | Not called |
| The execution log kept in the Durable Object | The trace keeps each execution instead |

This is how the real runtime behaves with none of them configured, which is what the reference run
records.

### What runs

Each execution gets a fresh QuickJS context on its own thread: nothing is shared between
executions or samples, and samples run their code at the same time. The sandbox loads the same
`executor.js` module that Cloudflare's `DynamicWorkerExecutor` builds for the code (its console
capture, its proxies and its timeout), as an ES module. The code sees:

- one global per namespace or connector, with one async method per MCP tool. A call sends the
  first argument as the tool's arguments and returns what Cloudflare's `McpConnector` returns: the
  result's `structuredContent` when it has one; for text results, the text parsed as JSON when it
  is JSON, otherwise the text; for images and other content, the whole MCP result object.
- errors as exceptions the code can catch: a tool's error result throws `Error` with the error
  text; a method that does not exist throws `Tool "name" not found` (`... not found on <connector>`
  in the `runtime` format); an MCP protocol error throws `MCP error <code>: <message>` and a call
  over `tool_timeout_s` `MCP error -32001: Request timed out`, as the MCP TypeScript SDK words them.
- the globals of a Worker that could be made to behave the same, listed below. There is no network,
  file system or other host access.

| Global | In the sandbox |
| --- | --- |
| `console` | `log`, `warn` and `error` are captured; the other methods exist and write nothing |
| `setTimeout`, `setInterval`, `setImmediate`, their `clear` functions, `queueMicrotask`, `scheduler.wait` | As in Workers |
| `URL`, `URLSearchParams` | The WHATWG URL Standard, as in Workers |
| `TextEncoder`, `TextDecoder` | UTF-8, UTF-16LE, UTF-16BE and windows-1252 (`latin1`, `ascii`); other encodings throw |
| `atob`, `btoa`, `structuredClone`, `DOMException` | As in Workers, with its error names and messages |
| `crypto.randomUUID()`, `crypto.getRandomValues()` | As in Workers; `crypto.subtle` is not defined |
| `fetch` | Defined and refused, as in a Dynamic Worker without outbound access: the promise rejects with `This worker is not permitted to access the internet via global functions like fetch(). It must use capabilities (such as bindings in 'env') to talk to the outside world.` An invalid URL rejects with `TypeError: Invalid URL: <url>` |
| `eval`, `new Function` | Throw `EvalError: Code generation from strings disallowed for this context`, as in Workers |
| `Date` | UTC, with V8's string forms; `toLocaleString()` and the like without arguments write en-US |
| `Number.prototype.toLocaleString()`, `localeCompare` | Without a locale: en-US grouping, and the default collation order |
| `performance.now()`, `navigator`, `self`, `process` (`env`, `nextTick`, `platform`) | As a Worker with `nodejs_compat` has them |
| `JSON.parse`, `JSON.stringify` | QuickJS's, with V8's error messages (`Unexpected token 'o', "not json" is not valid JSON`) |

Not defined, so code that uses one fails with a `ReferenceError` where Workers would run it:
`Intl`, `Request`, `Response`, `Headers`, `FormData`, `Blob`, `File`, the streams,
`AbortController`, `AbortSignal`, `Event`, `EventTarget`, `WebSocket`, `EventSource`, `URLPattern`,
`Buffer`, `WebAssembly`, `caches`, `crypto.subtle` and the `node:` modules. Each would need an
implementation large enough that its answers could not be kept the same as Workers', and a global
that works in part makes code run differently instead of failing. `require` is not defined in
either.

Errors the engine itself throws are worded as V8 words them where V8's wording is fixed
(`Cannot read properties of undefined (reading 'x')`, `x is not defined`, `Invalid array length`,
`Invalid time value`), for an uncaught error and for `e.message` in the code alike. Messages V8
builds from the source text stay QuickJS's; the table under
[How close it is to Cloudflare's](#how-close-it-is-to-cloudflares) lists them.

At most 8 MCP calls of one execution run at once, and one execution makes at most 1,000. A call
the code did not wait for is cancelled when the execution ends. When an MCP server cannot be
reached even after a reconnect, the sample fails as in direct mode and **Retry failed** runs it again.

Between rounds, the assistant message goes back to the model the way the AI SDK sends it:
`content` is `null` when the answer had no text, and each call's `arguments` are the parsed input
written again with `JSON.stringify` (`{"code":"..."}` alone for a call the tool accepted, whatever
other keys or spacing the model wrote). The trace keeps the arguments as the model wrote them.

### The trace

The model's call is an ordinary tool call to `codemode` with `{"code": ...}`, and its tool message
holds exactly what the model saw. For viewing and audit, the tool message also has `code`
(`TraceTool.code`): the code as it ran (after `normalizeCode`), the status (`ok`, `error`,
`syntax_error`, `timeout`, `memory_limit`), every MCP call the code made (`method` as the code called
it, server, tool, arguments, result, `is_error`, latency), all console output, the returned value as
JSON before truncation, and the error. `tool_calls` counts the code tool's calls;
`inner_tool_calls` counts the MCP calls inside them, per sample and per run. `codemode.search`,
`codemode.describe`, `codemode.step` and `codemode.run` are the runtime's own and are not MCP
calls: they are not listed or counted, and their answers show in the execution's result. While code
runs, the in-flight list shows the sample as `code`, with the number of MCP calls made and the ones
in progress.

### Exports in code mode

The exports carry what the model saw, nothing from inside the sandbox, in both formats:

| Format | In code mode |
| --- | --- |
| Chat messages | `tools` holds the code tool's definition; assistant turns call `codemode` with `{"code": ...}` as the arguments object; `tool` messages hold the results as sent |
| ShareGPT | `function_call` turns with the code, `observation` turns with the results, `tools` with the code tool |
| DPO, KTO (conversational) | Trajectories of code tool calls and results |
| Raw | Everything above, plus `trace[].code` with the inner calls, logs and errors |

A chat messages record in the `code_tool` format (think tags, shortened); the `runtime` format's
is under [What the model sees: `runtime`](#what-the-model-sees-runtime):

```json
{"messages": [
  {"role": "user", "content": "What is 4 + 5?"},
  {"role": "assistant", "content": "<think>\nI'll add them in code.\n</think>\n\n",
   "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "codemode",
     "arguments": {"code": "async () => {\n  const s = await mock.add({ a: 4, b: 5 });\n  console.log(\"sum\", s);\n  return s;\n}"}}}]},
  {"role": "tool", "tool_call_id": "call_1", "name": "codemode", "content": "{\"result\":9,\"logs\":[\"sum 9\"]}"},
  {"role": "assistant", "content": "<think>\nThe code returned 9.\n</think>\n\n4 + 5 = 9."}
 ],
 "tools": [{"type": "function", "function": {"name": "codemode", "description": "Execute code to achieve a goal. ...", "parameters": {...}}}]}
```

The dataset card names the setup: `Code Mode (@cloudflare/codemode 0.5.6)` or
`Code Mode (@cloudflare/codemode 0.5.6, durable runtime)`.

### At inference

A student trained on these traces expects Cloudflare's Code Mode: the same tool, the same
description, and a runtime that runs its JavaScript and answers in the same format. Match the
run's settings.

For the `code_tool` format, use `createCodeTool` from `@cloudflare/codemode/ai` with an executor
(`DynamicWorkerExecutor` on Workers, or any implementation of its `Executor` interface):

- the same tool name and description (the AI SDK's `tools: { codemode }`, Cloudflare's default
  description unless the run changed it);
- the same namespaces: one provider per server, named as the run named it
  (`{ name: "github", tools: githubTools }`; the run's `tools.code.namespaces` lists them), or all
  tools in one tool set for `namespaces: "codemode"`;
- tools that take the MCP tool's JSON Schema as their input schema and return the MCP result
  unwrapped as `McpConnector` does (structured content, JSON text parsed, errors thrown), which is
  what the traces show the code getting;
- with the default budget, a wrapper around the tool's `execute` that passes the result and the
  logs through `truncateResult` (`createCodeTool` does not); with `max_result_tokens: 0`, nothing.

For the `runtime` format, use `createCodemodeRuntime({ ctx, executor, connectors })` in an Agent,
with one `McpConnector` per MCP server, and pass `runtime.tool()` as `tools: { codemode }`:

- each connector's `name()` is the run's connector name (`tools.code.connectors` lists them);
- `connectorHints` as the run's `connector_hints`, and no `transformResult` unless the run set a
  result budget (then `transformResult: (r) => truncateResult(r, { maxTokens })`);
- no approvals and no snippets, or the student meets `paused` results and snippet matches it has
  never seen.

Cloudflare's MCP page also shows `createCodeTool({ tools: this.mcp.getAITools(), executor })`. That
is a third shape, which neither format reproduces: the tools are named
`tool_<server id>_<tool name>` under `codemode` (the id is random unless `addMcpServer` is given
one), their types gain `[key: string]: unknown`, the code gets the raw MCP result
(`{content: [...], isError: false}`) and arguments are checked by zod before the call.
`crates/codemode/tests/golden/fixtures/ai_tools.json` records it.

Code mode does not train a model for other code-calling formats:

| Format | How it differs |
| --- | --- |
| [goose's Code Mode extension](https://goose-docs.ai/docs/mcp/code-mode-mcp) | Also JavaScript, but the model gets three tools: `list_functions`, `get_function_details` and `execute_javascript`. The code has an `async function run()` entry point and calls tools with `tools.call("extension__tool", {...})`. It runs in Boa. Not reproduced |
| [Claude Code's Code Mode](https://code.claude.com/docs/en/code-mode) (experimental) | A `REPL` tool that takes `code`: JavaScript with a `$` shell template and helpers such as `edit()` that call Claude Code's own tools. Its result holds `result`, `stdout`, `stderr` and `toolsUsed`. Not reproduced |
| Anthropic's programmatic tool calling | The model writes Python that runs in the `code_execution` tool's container. The code calls the tools marked as callable from code; their results go back to the code instead of the model's context, and the code's output returns to the model. Python, not JavaScript, in the Messages API's own blocks. Not reproduced |

The tool's name and description can be changed per run, but the code's format (an async arrow
function in JavaScript), the declarations, the discovery calls and the result format stay
Cloudflare's.

### How close it is to Cloudflare's

`crates/codemode/tests/golden` holds a reference harness: the published package on workerd
1.20261124.1, driven through AI SDK 7.0.181 and its OpenAI-compatible provider against a scripted
model endpoint, with real MCP servers over streamable HTTP. It records what a model server
receives for 661 conversations in the `code_tool` setup and 749 in the `runtime` setup: results of
every type, console output, thrown and engine errors, syntax errors, TypeScript, code shapes that
`normalizeCode` rewrites, tool errors, timers and timeouts, truncation, the globals models reach
for, invalid inputs, and several executions in one conversation. The conformance test
(`crates/engine/tests/conformance.rs`) runs the same calls through Stellarator and compares the
`tools` array, each `tool` message and each assistant message sent back, byte for byte.

| | `code_tool` | `runtime` |
| --- | --- | --- |
| Conversations | 661 | 749 |
| The same byte for byte | 528 | 618 |
| Different | 133 | 131 |
| `tools` arrays (description and schema) | 11 of 11 the same | 15 of 15 the same |

Every difference is listed with its cause in
`crates/codemode/tests/golden/known-differences.json`; the test fails on a difference that is not
listed and on a listed case that matches. The causes:

| Cause | `code_tool` | `runtime` | Cloudflare | Stellarator |
| --- | --- | --- | --- | --- |
| Code that does not parse | 51 | 52 | `Uncaught SyntaxError: Unexpected identifier 'returns'` | The same frame and position rules, with QuickJS's message: `Uncaught SyntaxError: expecting ';'`. For some errors the line or column differs too. TypeScript syntax is a syntax error in both |
| Engine errors whose message quotes the code or a value | 28 | 28 | `api.v1.list is not a function`, `5 is not iterable`, `Assignment to constant variable.`, `Cannot destructure property 'a' of 'undefined' as it is undefined.` | QuickJS's message: `not a function`, `value is not iterable`, `'a' is read-only`, `Cannot convert undefined or null to object` |
| Workers globals the sandbox does not define | 20 | 20 | `crypto.subtle.digest(...)` returns the hash; `new Response(...)` and `Buffer.from(...)` work | `Cannot read properties of undefined (reading 'digest')`; `Response is not defined`, `Buffer is not defined` |
| `Intl` and locales | 15 | 15 | `(1234567.891).toLocaleString("de-DE")` is `1.234.567,891` | `Intl` is not defined; `toLocaleString` with a locale or options throws a `TypeError` that says so; without arguments it writes en-US as V8 does |
| `Error.stack` | 5 | 5 | `at executor.js:80:57`, and `stack` is the error's own property | V8's form with QuickJS's columns (`at executor.js:80:52`); `stack` is read through `Error.prototype` |
| A result the AI SDK cannot serialize | 3 | 0 | A returned BigInt or cycle makes `generateText` throw: the request fails and the model gets no tool message | `Error: Code execution failed: Do not know how to serialize a BigInt`. The `runtime` format answers these itself and matches |
| A tool called with something that is not an object | 3 | 3 | `mock.lookup("tides")` sends the string as the arguments; the answer is the MCP server's | `The arguments of mock.lookup must be an object of the tool's parameters.` |
| Earlier reasoning in the next request | 3 | 3 | The AI SDK sends an assistant turn's reasoning back as `reasoning_content` | Earlier reasoning is not sent back, in any mode |
| Behavior of the engine itself | 3 | 3 | A returned `Proxy` sends nothing; `new Date("Q1 2025")` is invalid | The proxy's properties are sent; QuickJS reads the date |
| Code that never yields (`while (true) {}`) | 2 | 2 | The executor's timeout is a timer and cannot fire. workerd alone never answers; on Cloudflare's network the CPU limit ends the Worker | Interrupted 250 ms after the timeout: `Execution timed out` |

What the comparison cannot show:

| Where | Cloudflare | Stellarator |
| --- | --- | --- |
| Memory | Workers ends an isolate over 128 MB; standalone workerd sets no limit, so this was not recorded | QuickJS's `out of memory`, as an execution error |
| The clock | On Cloudflare's network `Date.now()` does not advance while code runs | It advances, as on standalone workerd |
| Unawaited calls | Answered "late"; the host-side call still runs | Cancelled when the execution ends; the model-visible text is the same |
| Calls per execution | No limit | At most 1,000, 8 at a time |
| Approvals, snippets, rollback | Part of the durable runtime | Not simulated: a run behaves as a runtime with none configured |
| Execution ids | New for every execution | New for every execution; the comparison masks them |

Cloudflare's own JavaScript does the work wherever it is pure, run as published in QuickJS:
`normalizeCode` (with acorn), the type generation, `sanitizeToolName`, `truncateResult`, both
descriptions, the executor's module text, `codemode.search`, `codemode.describe`, the MCP result
unwrapping and the result's final shape (`crates/codemode/js/NOTICE.md` lists the files and what
was changed in them).

## Tools run for real

Every call is a real call, made for every sample:

- A run of 1,000 prompts with 4 samples per prompt makes thousands of calls. Mind rate limits and
  costs of the APIs behind your tools.
- **Retry failed** runs a sample again from the start, tools included. So does a sample that was
  cut off by a restart or a delete, or that was waiting to retry when the run paused.
- Tools that change things (write files, send messages, create issues) do so each time. Use
  read-only tools, a test account or a sandbox.
- Tool arguments and results are stored in the data folder with the samples, and exported.

## Stdio servers in server mode

A stdio server is a command that Stellarator starts. The Docker image and `stellarator-server` have
no sign-in and listen on the network, so anyone who reaches the port could make Stellarator run a
command. In server mode, stdio servers are therefore refused: adding, checking and running them
answers 403 with an explanation, and imported stdio entries are listed as errors. HTTP servers work
everywhere.

The operator can allow stdio servers by starting Stellarator with:

```sh
STELLARATOR_ALLOW_STDIO_MCP=1
```

Do this only on a trusted network. In Docker, the command runs inside the container, which has no
Node.js, Python or Docker: build an image that adds them, or run the MCP server outside and add it
over HTTP. The desktop app allows stdio servers: it opens no port.

`GET /api/settings` returns `stdio_mcp_allowed`.

## Try it with the mock

The mock model server (`stellarator-mock`, or `docker compose --profile mock up -d`) also serves
two small MCP servers:

| Server | Address | Tools |
| --- | --- | --- |
| The catalog | `http://localhost:9100/mcp` (`http://mock:9100/mcp` from the Docker container), or over stdio with `stellarator-mock mcp-stdio` | `lookup` (a short text about the query), `add`, `fail` (always an error result), `slow` (waits, default 2 s), `chart` (a text and an image part), `repeat` (long results) and, over stdio, `crash` and `getenv` |
| The issue tracker | `http://localhost:9100/tracker/mcp` | `list-issues` (500 generated issues; it declares an output schema and answers with structured content), `get_issue` (JSON as text), `create_issue`, `search` (a JSON array as text) and `delete`. The server sends `instructions`, which the `runtime` format shows in `codemode.describe` |

The tracker's names exercise code mode's naming: `list-issues` becomes `list_issues`, `delete`
becomes `delete_`. The models `mock-tools` (calls tools, then answers with the results),
`mock-tools-loop` (always calls tools) and `mock-no-tools` (refuses `tools` like vLLM without a
parser) exercise the loop. Writing `tool:<name>` in a prompt makes `mock-tools` call that tool, and
`both` makes it call two tools in parallel.

In code mode, `mock-tools` writes JavaScript for the code tool, calling the methods its description
declares. `code:plain`, `code:fence` (in a markdown fence, ending in an expression), `code:loop`,
`code:catch` (a try/catch around `fail`), `code:parallel` (`Promise.all`), `code:throw`,
`code:slow`, `code:syntax`, `code:infinite` and `code:memory` in the prompt pick the code; without
one, a hash of the prompt picks among the first five.

Given the `runtime` format's tool, `mock-tools` works as its description says, one execution per
request: `codemode.search` with up to three words of the prompt (numbers and words such as "the"
left out), `codemode.describe` of the best match (of the first connector when nothing matched), a
call of that method with arguments read from the described types, then the answer. A prompt such
as `list issues` finds `tracker.list_issues`, and `add 4 and 5` finds `mock.add`. `code:discover`
searches and describes in one execution; `code:catch`, `code:throw`, `code:step`
(`codemode.step`), `code:slow`, `code:syntax`, `code:infinite` and `code:memory` shape the call.
`mock-tools-loop` repeats the call and never answers.

## API

| Route | Body | Answer |
| --- | --- | --- |
| `GET /api/mcp-servers` | | `[McpServerOut]` |
| `POST /api/mcp-servers` | `McpServerIn` | 201 `McpServerOut` |
| `POST /api/mcp-servers/import` | `{"text": "<pasted JSON>", "check": false}` | `{"created", "errors", "notes"}` |
| `GET /api/mcp-servers/{id}` | | `McpServerOut` |
| `PATCH /api/mcp-servers/{id}` | `McpServerPatch` (`env` and `headers`: a value replaces, `null` keeps, a name left out is removed) | `McpServerOut` |
| `DELETE /api/mcp-servers/{id}` | | 204; 409 while a running run uses it |
| `POST /api/mcp-servers/{id}/check` | | `McpServerOut` with `status` `ok` or `error` |

A run's tools are `params.tools` in `POST /api/runs` (`mode` and `code` for code mode, `code.format`
for its two formats); `RunOut.tools` holds the definitions sent, and in code mode
`RunOut.tools.code` the format (`kind`), the namespaces and, for the `runtime` format, the
connectors. A
sample's `trace` (in `GET /api/samples/{id}`) lists the messages after the prompt, `tool_calls`
counts the calls run and `inner_tool_calls` the MCP calls made by code. The types are in [`crates/api`](../crates/api/src/lib.rs).
