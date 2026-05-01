// Chat domain types — local to /chat surface, no dep on lib/types.ts schemas.

export type Driver =
  | "claude"      // Claude Code via MCP
  | "openai"      // OpenAI via API key (sessionStorage)
  | "openrouter"  // OpenRouter via API key (encrypted at rest)
  | "codex";      // Codex CLI via OAuth

// opencode-style provider/model-name string e.g. "claude/sonnet-4-6"
export type ModelId = string;

export interface ModelInfo {
  id: ModelId;
  label: string;
  driver: Driver;
  hint?: string;
}

// Kept for runtime compatibility — not used for new UI.
export const MODELS: ModelInfo[] = [
  { id: "claude/opus-4-7",    label: "Claude Opus 4.7",   driver: "claude",    hint: "Most capable" },
  { id: "claude/sonnet-4-6",  label: "Claude Sonnet 4.6", driver: "claude",    hint: "Balanced" },
  { id: "claude/haiku-4-5",   label: "Claude Haiku 4.5",  driver: "claude",    hint: "Fast" },
  // Legacy aliases
  { id: "claude-opus-4-7",    label: "Claude Opus 4.7",   driver: "claude",    hint: "Most capable" },
  { id: "claude-sonnet-4-6",  label: "Claude Sonnet 4.6", driver: "claude",    hint: "Balanced" },
  { id: "claude-haiku-4-5",   label: "Claude Haiku 4.5",  driver: "claude",    hint: "Fast" },
  { id: "claude-code",        label: "Claude Code",        driver: "claude",    hint: "Anthropic agent" },
  { id: "gpt-4o",             label: "GPT-4o",             driver: "openai",    hint: "OpenAI flagship" },
  { id: "gpt-4o-mini",        label: "GPT-4o mini",        driver: "openai",    hint: "Fast + cheap" },
  { id: "o1",                 label: "o1",                 driver: "openai",    hint: "Reasoning" },
  { id: "o1-mini",            label: "o1-mini",            driver: "openai",    hint: "Reasoning fast" },
  { id: "codex/gpt-5",        label: "GPT-5",              driver: "codex",     hint: "Latest" },
  { id: "codex/o3",           label: "o3",                 driver: "codex",     hint: "Reasoning" },
  { id: "codex",              label: "Codex",              driver: "codex",     hint: "Signed-in OAuth" },
];

export const MODELS_BY_DRIVER: Record<Driver, ModelInfo[]> = {
  "claude":     MODELS.filter((m) => m.driver === "claude"),
  "openai":     MODELS.filter((m) => m.driver === "openai"),
  "openrouter": MODELS.filter((m) => m.driver === "openrouter"),
  "codex":      MODELS.filter((m) => m.driver === "codex"),
};

export const DRIVER_LABELS: Record<Driver, string> = {
  "claude":     "Claude Code · MCP",
  "openai":     "OpenAI · API key",
  "openrouter": "OpenRouter",
  "codex":      "Codex · Sign-in",
};

export const DEFAULT_MODEL_FOR_DRIVER: Record<Driver, string> = {
  "claude":     "claude/sonnet-4-6",
  "openai":     "openai/gpt-4o",
  "openrouter": "openrouter/openai/gpt-4o",
  "codex":      "codex/gpt-5",
};

export function driverFor(model: ModelId): Driver {
  const m = MODELS.find((x) => x.id === model);
  if (m) return m.driver;
  // opencode-style id: derive from prefix
  if (model.startsWith("claude/")) return "claude";
  if (model.startsWith("codex/"))  return "codex";
  if (model.startsWith("openai/")) return "openai";
  if (model.startsWith("openrouter/")) return "openrouter";
  return "openai";
}

export type ToolStepStatus = "running" | "done" | "error";

export interface ToolStep {
  id: string;
  name: string;
  args: unknown;
  result?: unknown;
  error?: string;
  status: ToolStepStatus;
  startedAt: number;
  endedAt?: number;
}

export type ChatRole = "user" | "assistant" | "system";

export interface ChatMessage {
  id: string;
  threadId: string;
  role: ChatRole;
  content: string;
  // Tool steps that occurred WITHIN this assistant turn, in order.
  toolSteps?: ToolStep[];
  model?: ModelId;
  driver?: Driver;
  createdAt: number;
  // True while streaming, false once finalized.
  streaming?: boolean;
}

export interface ChatThread {
  id: string;
  title: string;
  model: ModelId;
  driver: Driver;
  createdAt: number;
  updatedAt: number;
  archived: 0 | 1; // dexie indexes booleans poorly; use 0/1
}

// SSE event shape emitted by backend.
export type ChatStreamEvent =
  | { type: "delta"; text: string }
  | { type: "tool_call"; id: string; name: string; args: unknown }
  | { type: "tool_result"; id: string; result?: unknown; error?: string }
  | { type: "done" }
  | { type: "error"; message: string };
