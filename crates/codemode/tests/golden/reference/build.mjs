// Bundles worker.mjs into one ES module for workerd, and writes the workerd config next to it.
//
// Standalone workerd has no bundler and no `cloudflare:email` module (wrangler and miniflare add
// that one; the Agents SDK imports it for a feature this harness does not use), so it is replaced
// by a stub. `cloudflare:workers` and the `node:` modules are workerd's own.
import { build as esbuild } from "esbuild";
import { builtinModules } from "node:module";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

const emailStub = {
  name: "cloudflare-email-stub",
  setup(b) {
    b.onResolve({ filter: /^cloudflare:email$/ }, () => ({ path: "cloudflare:email", namespace: "stub" }));
    b.onLoad({ filter: /.*/, namespace: "stub" }, () => ({
      contents: "export class EmailMessage { constructor() { throw new Error('cloudflare:email is not available in this harness'); } }",
      loader: "js",
    }));
  },
};

// Dependencies that import Node's modules by their bare names (`fs`, `os`, `path`): workerd has
// them as `node:fs` and so on (nodejs_compat).
const nodeBuiltins = {
  name: "node-builtins",
  setup(b) {
    b.onResolve({ filter: new RegExp(`^(${builtinModules.join("|")})$`) }, (args) => ({ path: `node:${args.path}`, external: true }));
  },
};

/** Bundle the Worker into `<dir>/worker.bundle.mjs`; returns the bundle's size in bytes. */
export async function bundle(dir) {
  const outfile = join(dir, "worker.bundle.mjs");
  const result = await esbuild({
    entryPoints: [join(here, "worker.mjs")],
    bundle: true,
    format: "esm",
    platform: "neutral",
    mainFields: ["module", "main"],
    conditions: ["workerd", "worker", "import"],
    external: ["cloudflare:workers", "node:*"],
    plugins: [emailStub, nodeBuiltins],
    // CommonJS dependencies call require() for Node's modules; an ES module has none of its own.
    banner: { js: 'import { createRequire as __createRequire } from "node:module";\nconst require = __createRequire("/");' },
    outfile,
    metafile: true,
    logLevel: "warning",
  });
  return { outfile, bytes: result.metafile.outputs[Object.keys(result.metafile.outputs)[0]].bytes };
}

/**
 * The workerd config: the Worker with its Agent as a SQLite-backed Durable Object namespace on
 * local disk (the Agents SDK and the durable runtime's facet both keep their state in SQLite), the Worker Loader binding
 * DynamicWorkerExecutor needs, and an outbound network that reaches localhost (the MCP servers and
 * the stub model endpoint). The Dynamic Workers the executor loads get `globalOutbound: null` from
 * the executor itself, so the model's code has no network.
 */
export function writeConfig(dir, port) {
  const path = join(dir, "config.capnp");
  // SQLite-backed Durable Objects need storage on disk (workerd's in-memory storage has no SQL).
  mkdirSync(join(dir, "storage"), { recursive: true });
  writeFileSync(
    path,
    `using Workerd = import "/workerd/workerd.capnp";

const config :Workerd.Config = (
  services = [
    (name = "main", worker = .mainWorker),
    (name = "internet", network = (allow = ["public", "private", "local"])),
    (name = "storage", disk = (path = "storage", writable = true)),
  ],
  sockets = [(name = "http", address = "127.0.0.1:${port}", http = (), service = "main")],
);

const mainWorker :Workerd.Worker = (
  modules = [(name = "worker", esModule = embed "worker.bundle.mjs")],
  compatibilityDate = "2026-11-01",
  bindings = [
    (name = "LOADER", workerLoader = (id = "codemode")),
    (name = "Chat", durableObjectNamespace = "Chat"),
  ],
  durableObjectNamespaces = [(className = "Chat", uniqueKey = "stellarator-codemode-reference", enableSql = true)],
  durableObjectStorage = (localDisk = "storage"),
  globalOutbound = "internet",
);
`,
  );
  return path;
}
