// The reference Worker: the published @cloudflare/codemode on workerd, set up the way Cloudflare's
// docs set it up, and driven through the AI SDK against a scripted model endpoint.
//
// One Agent (a Durable Object, from the Agents SDK) per case. It connects to the MCP servers over
// streamable HTTP with the Agents SDK's own MCP client (`this.addMcpServer`), builds the code tool
// for the requested setup, and runs generateText or streamText with the OpenAI-compatible provider
// pointed at the stub endpoint that run.mjs serves. The stub records every request, so the
// fixtures hold what a model server receives on the wire.
//
// Setups:
// - "code_tool": createCodeTool with DynamicWorkerExecutor (developers.cloudflare.com/agents/
//   tools/codemode/ai-sdk/). One ToolProvider per MCP server, named after it; each tool's execute
//   calls the server through the package's own McpConnector, so the code gets the unwrapped result.
//   `namespaces: "codemode"` passes one tool set instead (every tool under `codemode`).
// - "runtime": createCodemodeRuntime({ctx, executor, connectors}).tool() with one McpConnector per
//   server (developers.cloudflare.com/agents/tools/codemode/mcp/ and durable-runtime/).
// - "ai_tools": createCodeTool({tools: this.mcp.getAITools(), executor}), the docs' non-durable MCP
//   example, recorded for reference only.
import { Agent, getAgentByName } from "agents";
import {
  CodemodeRuntime,
  DynamicWorkerExecutor,
  McpConnector,
  createCodemodeRuntime,
  sanitizeToolName,
  truncateResult,
} from "@cloudflare/codemode";
import { createCodeTool } from "@cloudflare/codemode/ai";
import * as ai from "ai";
import { createOpenAICompatible } from "@ai-sdk/openai-compatible";

// The durable runtime's facet class must be an export of the Worker's entry module
// (@cloudflare/codemode/vite adds this line to src/server.ts).
export { CodemodeRuntime };

/** One MCP server as a connector, named as the caller asks (the docs' GithubConnector, generic). */
class ServerConnector extends McpConnector {
  #name;
  #connection;
  constructor(ctx, env, name, connection) {
    super(ctx, env);
    this.#name = name;
    this.#connection = connection;
  }
  name() {
    return this.#name;
  }
  createConnection() {
    return this.#connection;
  }
}

export class Chat extends Agent {
  async onRequest(request) {
    let body;
    try {
      body = await request.json();
      return Response.json(await this.#run(body));
    } catch (e) {
      return Response.json({ failed: String(e), stack: e?.stack ?? null }, { status: 500 });
    }
  }

  async #run(body) {
    // The MCP servers, in the order given, with stable ids so getAITools() names are repeatable.
    const connections = [];
    for (const s of body.servers) {
      const { id } = await this.addMcpServer(s.name, s.url, { id: s.id ?? s.name });
      connections.push({ ...s, id });
    }
    await this.mcp.waitForConnections();
    for (const c of connections) {
      c.connection = this.mcp.mcpConnections[c.id];
      if (!c.connection) throw new Error(`No MCP connection for ${c.name}`);
    }

    const executor = new DynamicWorkerExecutor({
      loader: this.env.LOADER,
      ...(body.timeout_ms ? { timeout: body.timeout_ms } : {}),
    });
    const connector = (c) => new ServerConnector(this.ctx, this.env, c.namespace ?? sanitizeToolName(c.name), c.connection);

    let codemode;
    const extra = {};
    if (body.setup === "runtime") {
      const runtime = createCodemodeRuntime({
        ctx: this.ctx,
        executor,
        connectors: connections.map(connector),
        ...(body.transform_result ? { transformResult: (result) => truncateResult(result, body.transform_result === true ? undefined : body.transform_result) } : {}),
      });
      codemode = runtime.tool(body.tool_options ?? undefined);
      if (body.host_calls) {
        // runtime.search and runtime.describe from the host, without sandbox code.
        extra.host = [];
        for (const call of body.host_calls) extra.host.push({ call, result: await runtime[call[0]](call[1]) });
      }
      extra.executions = async () => (await runtime.executions()).map((e) => ({ status: e.status, calls: e.log.length }));
    } else if (body.setup === "code_tool") {
      const providers = [];
      const merged = {};
      const seen = new Map();
      for (const c of connections) for (const t of c.connection.tools) seen.set(t.name, (seen.get(t.name) ?? 0) + 1);
      for (const c of connections) {
        const conn = connector(c);
        const tools = {};
        for (const t of c.connection.tools) {
          const tool = {
            description: t.description,
            inputSchema: ai.jsonSchema(t.inputSchema),
            ...(t.outputSchema ? { outputSchema: ai.jsonSchema(t.outputSchema) } : {}),
            execute: (args) => conn.executeTool(sanitizeToolName(t.name), args),
          };
          tools[t.name] = tool;
          merged[seen.get(t.name) > 1 ? `${c.name}__${t.name}` : t.name] = tool;
        }
        providers.push({ name: c.namespace ?? sanitizeToolName(c.name), tools });
      }
      codemode = createCodeTool({
        tools: body.namespaces === "codemode" ? merged : providers,
        executor,
        ...(body.description ? { description: body.description } : {}),
      });
    } else if (body.setup === "ai_tools") {
      codemode = createCodeTool({ tools: this.mcp.getAITools(), executor });
    } else {
      throw new Error(`Unknown setup ${body.setup}`);
    }

    const provider = createOpenAICompatible({ name: "stub", baseURL: body.stub });
    const stop = (ai.stepCountIs ?? ai.isStepCount)(body.max_steps ?? 8);
    const call = {
      model: provider.chatModel(body.model),
      prompt: body.prompt ?? "Run the case.",
      tools: { [body.tool_name ?? "codemode"]: codemode },
      stopWhen: stop,
    };
    let text;
    if (body.driver === "stream") {
      const result = ai.streamText(call);
      for await (const _ of result.fullStream) {
        // Drain: the steps run as the stream is read.
      }
      text = await result.text;
    } else {
      text = (await ai.generateText(call)).text;
    }
    const out = { text };
    if (extra.host) out.host = extra.host;
    if (extra.executions) out.executions = await extra.executions();
    return out;
  }
}

export default {
  async fetch(request, env) {
    const name = new URL(request.url).searchParams.get("agent") ?? "default";
    const agent = await getAgentByName(env.Chat, name);
    return agent.fetch(request);
  },
};
