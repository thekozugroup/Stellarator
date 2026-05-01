// Central model catalog — opencode-style provider/model-name identifiers.
// Kept separate from types.ts to allow dynamic OpenRouter injection.

export type ModelProvider = "claude" | "codex" | "openai" | "openrouter";

export type ModelRequires = "mcp" | "codex-auth" | "openai-key" | "openrouter-key";

export interface ModelOption {
  id: string; // provider/model-name e.g. "claude/opus-4-7"
  provider: ModelProvider;
  label: string;
  subtitle?: string;
  requires: ModelRequires;
}

// ---------------------------------------------------------------------------
// Static catalog
// ---------------------------------------------------------------------------

export const STATIC_MODELS: ModelOption[] = [
  // Claude Code (via MCP — no key needed)
  { id: "claude/opus-4-7",    provider: "claude",    label: "Opus 4.7",    subtitle: "Most capable",  requires: "mcp" },
  { id: "claude/sonnet-4-6",  provider: "claude",    label: "Sonnet 4.6",  subtitle: "Balanced",      requires: "mcp" },
  { id: "claude/haiku-4-5",   provider: "claude",    label: "Haiku 4.5",   subtitle: "Fast",          requires: "mcp" },

  // Codex (Sign in with Codex — primary OpenAI access)
  { id: "codex/gpt-5",        provider: "codex",     label: "GPT-5",       subtitle: "Latest",        requires: "codex-auth" },
  { id: "codex/gpt-5-mini",   provider: "codex",     label: "GPT-5 mini",  subtitle: "Fast",          requires: "codex-auth" },
  { id: "codex/o3",           provider: "codex",     label: "o3",          subtitle: "Reasoning",     requires: "codex-auth" },
  { id: "codex/o1",           provider: "codex",     label: "o1",          subtitle: "Reasoning",     requires: "codex-auth" },

  // OpenAI API key (sessionStorage fallback)
  { id: "openai/gpt-4o",      provider: "openai",    label: "GPT-4o",      subtitle: "Flagship",      requires: "openai-key" },
  { id: "openai/gpt-4o-mini", provider: "openai",    label: "GPT-4o mini", subtitle: "Fast + cheap",  requires: "openai-key" },
  { id: "openai/o1",          provider: "openai",    label: "o1",          subtitle: "Reasoning",     requires: "openai-key" },
  { id: "openai/o1-mini",     provider: "openai",    label: "o1-mini",     subtitle: "Reasoning fast",requires: "openai-key" },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

export function parseModelId(id: string): { provider: ModelProvider; name: string } {
  const slash = id.indexOf("/");
  if (slash === -1) return { provider: "claude", name: id };
  const raw = id.slice(0, slash);
  const provider: ModelProvider =
    raw === "claude" || raw === "codex" || raw === "openai" || raw === "openrouter"
      ? (raw as ModelProvider)
      : "openrouter";
  return { provider, name: id.slice(slash + 1) };
}

export function defaultModelFor(provider: ModelProvider): string {
  switch (provider) {
    case "claude":    return "claude/sonnet-4-6";
    case "codex":     return "codex/gpt-5";
    case "openai":    return "openai/gpt-4o";
    case "openrouter": return "openrouter/openai/gpt-4o";
  }
}

// ---------------------------------------------------------------------------
// Availability resolver
// ---------------------------------------------------------------------------

export interface AvailabilityContext {
  /** Whether the openrouter integration key is set server-side. */
  hasOpenRouterKey: boolean;
  /** Whether OpenAI API key is present in sessionStorage. */
  hasOpenAIKey: boolean;
  /** Codex OAuth user handle, if signed in. */
  codexUser: string | null;
  /** Whether Claude Code MCP is considered installed (defaults to true). */
  mcpInstalled?: boolean;
}

export interface UnavailableModelOption extends ModelOption {
  hint: string;
}

export interface ResolvedModels {
  available: ModelOption[];
  unavailable: UnavailableModelOption[];
}

export function resolveAvailableModels(
  ctx: AvailabilityContext,
  openRouterModels: ModelOption[] = [],
): ResolvedModels {
  const mcpInstalled = ctx.mcpInstalled !== false; // default true
  const all: ModelOption[] = [...STATIC_MODELS, ...openRouterModels];
  const available: ModelOption[] = [];
  const unavailable: UnavailableModelOption[] = [];

  for (const m of all) {
    const ok = isSatisfied(m.requires, ctx, mcpInstalled);
    if (ok) {
      available.push(m);
    } else {
      unavailable.push({ ...m, hint: hintFor(m.requires) });
    }
  }

  return { available, unavailable };
}

function isSatisfied(
  req: ModelRequires,
  ctx: AvailabilityContext,
  mcpInstalled: boolean,
): boolean {
  switch (req) {
    case "mcp":           return mcpInstalled;
    case "codex-auth":    return !!ctx.codexUser;
    case "openai-key":    return ctx.hasOpenAIKey;
    case "openrouter-key": return ctx.hasOpenRouterKey;
  }
}

function hintFor(req: ModelRequires): string {
  switch (req) {
    case "mcp":           return "Requires Claude Code MCP install";
    case "codex-auth":    return "Sign in with Codex in Settings →";
    case "openai-key":    return "Add OpenAI API key in Settings →";
    case "openrouter-key": return "Configure OpenRouter in Settings →";
  }
}

// ---------------------------------------------------------------------------
// Driver derivation — maps provider/model id → backend driver field
// ---------------------------------------------------------------------------
export function driverFromModelId(id: string): string {
  const { provider } = parseModelId(id);
  switch (provider) {
    case "claude":     return "claude";
    case "codex":      return "codex";
    case "openai":     return "openai";
    case "openrouter": return "openrouter";
  }
}

// ---------------------------------------------------------------------------
// Legacy conversion helpers (for existing DB rows using old ModelId format)
// ---------------------------------------------------------------------------
export function legacyModelIdToNew(oldId: string): string {
  const MAP: Record<string, string> = {
    "claude-opus-4-7":   "claude/opus-4-7",
    "claude-sonnet-4-6": "claude/sonnet-4-6",
    "claude-haiku-4-5":  "claude/haiku-4-5",
    "claude-code":       "claude/sonnet-4-6",
    "gpt-4o":            "openai/gpt-4o",
    "gpt-4o-mini":       "openai/gpt-4o-mini",
    "o1":                "openai/o1",
    "o1-mini":           "openai/o1-mini",
    "codex":             "codex/gpt-5",
  };
  return MAP[oldId] ?? oldId;
}
