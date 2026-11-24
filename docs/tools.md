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
the MCP tools through typed namespaces; Stellarator runs the code in a sandbox and sends back only
what the code returns and logs. This is Code Mode as Cloudflare defines it in
[`@cloudflare/codemode`](https://developers.cloudflare.com/agents/tools/codemode/) 0.5.6
(github.com/cloudflare/agents, commit `4116a8bdf97660c8e79a923b6aec140256282a5d`), and the traces
reproduce what a model sees and writes there, so a student trained on them works in that setup.

### When it helps

| Situation | Why code mode helps |
| --- | --- |
| Many tools (dozens of MCP tools) | The tools are one block of TypeScript declarations in one tool's description, which models read well |
| Large intermediate results | A listing the code filters or counts stays in the sandbox; only the code's result reaches the model |
| Chained calls (look up, then act on the result) | One execution makes every call; the model does not pay a request per round |
| Loops and retries | The code loops over inputs or catches a failing call itself |

For a run where each sample makes one or two small calls, direct mode is as good and simpler.

### Turn it on

In **New run** turn on **Tools**, pick servers, and choose code mode. In the API, set
`params.tools.mode` to `"code"`. The other tool settings keep their meaning: `max_rounds` counts
rounds of code tool calls, `tool_timeout_s` limits each MCP call the code makes, `tool_choice`
applies to the code tool, and `max_result_chars` only caps the copy of each MCP result stored in
the trace (the code always gets the whole result). The code mode settings, `params.tools.code`,
default to Cloudflare's:

| Setting | Default | Range | Meaning |
| --- | --- | --- | --- |
| `tool_name` | `codemode` | 1 to 64 letters, digits, `_`, `-` | The tool's name (Cloudflare's README registers it as `tools: { codemode }`) |
| `description` | null: Cloudflare's | up to 100,000 characters | The description template; the first `{{types}}` is replaced by the generated declarations |
| `namespaces` | `servers` | `servers`, `codemode` | `servers`: one global per server, named after it (`github.list_issues(...)`). `codemode`: every tool under `codemode` (`codemode.list_issues(...)`); tools of the same name on two servers become `<server>__<tool>` |
| `timeout_s` | 60 | 1 to 3600 | Seconds one execution may take, its MCP calls included |
| `max_result_tokens` | 6000 | 0, or 100 to 1,000,000 | The budget of the result and of the console output the model sees, at 4 characters per token; a result or console output over it is shrunk as Cloudflare's `truncateResult` does it. 0: no truncation |
| `max_result_chars` | null | 1,000 to 4,000,000 | The same budget in characters; overrides `max_result_tokens * 4` |
| `memory_limit_mb` | 128 | 8 to 1024 | Memory one execution may use (a Workers isolate's limit is 128 MB) |

Names become JavaScript identifiers with Cloudflare's `sanitizeToolName`: `my-server` becomes
`my_server`, `3d-render` becomes `_3d_render`, `delete` becomes `delete_`. A run is refused, with the
names to change, when two servers or two tools of a server would get the same name, or when a
server's name would hide a global the sandbox needs (`console`, `Promise`, `Object` and a few
more); `namespaces: "codemode"` avoids both.

The run stores the code tool as sent (`RunOut.tools.definitions`, one definition) and, in
`RunOut.tools.code`, the generated types and which MCP tool each method calls.

### What the model sees

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
| Not valid JavaScript | `SyntaxError: <message>` (the executor itself throws) |
| The input is not `{"code": string}` | The AI SDK's error, such as `AI_InvalidToolInputError: Invalid input for tool codemode: AI_TypeValidationError: Type validation failed: ...` |
| The model called another tool | `AI_NoSuchToolError: Model tried to call unavailable tool 'x'. Available tools: codemode.` |

`console.log` lines are the arguments joined with spaces (`String(arg)`), `console.warn` lines
start with `[warn] `, `console.error` lines with `[error] `; other console methods write nothing,
as in Cloudflare's executor. A result over the budget is shrunk structurally (the largest values
first, with `--- TRUNCATED ---` markers, still valid JSON), and a long text ends with
`--- TRUNCATED ---\nResponse was ~N tokens (limit: 6,000). Narrow the request to reduce response size.`

### What runs

Each execution gets a fresh QuickJS context on its own thread: nothing is shared between
executions or samples, and samples run their code at the same time. The code sees:

- one global per namespace, with one async method per MCP tool. A call sends the first argument as
  the tool's arguments and returns what Cloudflare's MCP integration returns: the result's
  `structuredContent` when it has one; for text results, the text parsed as JSON when it is JSON,
  otherwise the text; for images and other content, the whole MCP result object.
- errors as exceptions the code can catch: a tool's error result throws `Error` with the error
  text; a method the namespace does not have throws `Tool "name" not found`; an MCP protocol error
  throws `MCP error <code>: <message>` and a call over `tool_timeout_s`
  `MCP error -32001: Request timed out`, as the MCP TypeScript SDK words them.
- `console`, `setTimeout`, `setInterval`, their `clear` functions and `queueMicrotask`; no `fetch`,
  file system or other host access.

At most 8 MCP calls of one execution run at once, and one execution makes at most 1,000. A call
the code did not wait for is cancelled when the execution ends. When an MCP server cannot be
reached even after a reconnect, the sample fails as in direct mode and **Retry failed** runs it again.

### The trace

The model's call is an ordinary tool call to `codemode` with `{"code": ...}`, and its tool message
holds exactly what the model saw. For viewing and audit, the tool message also has `code`
(`TraceTool.code`): the code as it ran (after `normalizeCode`), the status (`ok`, `error`,
`syntax_error`, `timeout`, `memory_limit`), every MCP call the code made (`method` as the code called
it, server, tool, arguments, result, `is_error`, latency), all console output, the returned value as
JSON before truncation, and the error. `tool_calls` counts the code tool's calls;
`inner_tool_calls` counts the MCP calls inside them, per sample and per run. While code runs, the
in-flight list shows the sample as `code`, with the number of MCP calls made and the ones in progress.

### Exports in code mode

The exports carry what the model saw, nothing from inside the sandbox:

| Format | In code mode |
| --- | --- |
| Chat messages | `tools` holds the code tool's definition; assistant turns call `codemode` with `{"code": ...}` as the arguments object; `tool` messages hold the results as sent |
| ShareGPT | `function_call` turns with the code, `observation` turns with the results, `tools` with the code tool |
| DPO, KTO (conversational) | Trajectories of code tool calls and results |
| Raw | Everything above, plus `trace[].code` with the inner calls, logs and errors |

A chat messages record (think tags, shortened):

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

### At inference

A student trained on these traces expects Cloudflare's Code Mode: the same tool, the same
declarations, and a runtime that runs its JavaScript and answers in the same format. Use
`createCodeTool` from `@cloudflare/codemode/ai` with an executor (`DynamicWorkerExecutor` on
Workers, or any implementation of its `Executor` interface), and match the run's settings:

- the same tool name and description (the AI SDK's `tools: { codemode }`, Cloudflare's default
  description unless the run changed it);
- the same namespaces: one provider per server, named as the run named it
  (`{ name: "github", tools: githubTools }`; the run's `tools.code.namespaces` lists them), or all
  tools in one tool set for `namespaces: "codemode"`;
- the MCP results unwrapped as Cloudflare's `McpConnector` and `codeMcpServer` do (structured
  content, JSON text parsed, errors thrown), which is what the traces show the code getting;
- with the default budget, `truncateResult` on the result and the logs (the `transformResult`
  option of `createCodemodeRuntime` does this); with `max_result_tokens: 0`, nothing.

Code mode does not train a model for other code-calling formats:

| Format | How it differs |
| --- | --- |
| Goose's Code Mode extension | Also JavaScript, but the model gets three meta-tools to find tools, read their interfaces and run code, and the code runs in Deno. Its tool names, descriptions and results differ from Cloudflare's |
| Anthropic's programmatic tool calling | The model writes Python that runs in the `code_execution` tool's container. The code calls the tools marked as callable from code; their results go back to the code instead of the model's context, and the code's output returns to the model. Python, not JavaScript, in the Messages API's own blocks |

The tool's name and description template can be changed per run, but the code's format (an async
arrow function in JavaScript), the namespaces' declarations and the result format stay Cloudflare's.

### What is not reproduced

The formats come from Cloudflare's own JavaScript, run as published in QuickJS: `normalizeCode`
(with acorn), the type generation, `sanitizeToolName`, `truncateResult` and the default
description; the golden tests in `crates/codemode/tests` compare them with the package's output.
The executor's harness, `runCode` and the AI SDK's messages are reproduced from their source and
were checked against AI SDK 7.0.181 with its OpenAI-compatible provider. What differs:

| Where | Cloudflare | Stellarator |
| --- | --- | --- |
| Engine | V8 in a Dynamic Worker | QuickJS-NG. Error messages of the engine itself differ: V8's `Cannot read properties of undefined (reading 'x')` is `cannot read property 'x' of undefined`. `x is not defined` is the same |
| Syntax errors | The Worker fails to load; the exact text is the runtime's | `SyntaxError: <QuickJS message>`, as the error the executor throws |
| Code that never yields (`while (true) {}`) | Ends at Workers' CPU limit | Interrupted at the timeout: `Execution timed out` |
| Memory | Workers ends the isolate | QuickJS's `out of memory` error, which the code could catch |
| Globals | Workers' APIs (`URL`, `TextEncoder`, `crypto`, `atob`; `fetch` throws) | Only `console`, timers and `queueMicrotask`; `fetch` is not defined |
| Values | Workers RPC with Cloudflare's codec (also binary data) | JSON both ways |
| Unawaited calls | Answered "late"; the host-side call still runs | Cancelled when the execution ends, and recorded with Cloudflare's late-call message |
| Calls per execution | No limit | At most 1,000, 8 at a time |
| Arguments | The AI SDK's `getAITools` validates them with zod first | Sent to the MCP server, which validates them |
| Error text with long console output | Not truncated | The console output is cut to the budget |
| The assistant message sent back | The AI SDK re-serializes the parsed input | The arguments as the model wrote them |

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

The mock model server (`stellarator-mock`, or `docker compose --profile mock up -d`) also serves a
small MCP server:

- over streamable HTTP at `http://localhost:9100/mcp` (`http://mock:9100/mcp` from the Docker
  container);
- over stdio: `stellarator-mock mcp-stdio`.

Its tools: `lookup` (a short text about the query), `add`, `fail` (always an error result), `slow`
(waits, default 2 s), `chart` (a text and an image part), `repeat` (long results) and, over stdio,
`crash` and `getenv`. The models `mock-tools` (calls tools, then answers with the results),
`mock-tools-loop` (always calls tools) and `mock-no-tools` (refuses `tools` like vLLM without a
parser) exercise the loop. Writing `tool:<name>` in a prompt makes `mock-tools` call that tool, and
`both` makes it call two tools in parallel.

In code mode, `mock-tools` writes JavaScript for the code tool, calling the methods its description
declares. `code:plain`, `code:fence` (in a markdown fence, ending in an expression), `code:loop`,
`code:catch` (a try/catch around `fail`), `code:parallel` (`Promise.all`), `code:throw`,
`code:slow`, `code:syntax`, `code:infinite` and `code:memory` in the prompt pick the code; without
one, a hash of the prompt picks among the first five.

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

A run's tools are `params.tools` in `POST /api/runs` (`mode` and `code` for code mode);
`RunOut.tools` holds the definitions sent, and in code mode `RunOut.tools.code` the namespaces. A
sample's `trace` (in `GET /api/samples/{id}`) lists the messages after the prompt, `tool_calls`
counts the calls run and `inner_tool_calls` the MCP calls made by code. The types are in [`crates/api`](../crates/api/src/lib.rs).
