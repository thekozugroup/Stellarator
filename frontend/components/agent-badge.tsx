import { Bot, Sparkles, Terminal, User } from "lucide-react";
import { Badge } from "@/components/ui/badge";

type ColorConfig = { dot: string; badge: string };

const TEAL: ColorConfig = {
  dot: "bg-agent-openai",
  badge: "border-agent-openai/30 bg-agent-openai/10 text-agent-openai ring-1 ring-agent-openai/25",
};
const VIOLET: ColorConfig = {
  dot: "bg-agent-claude",
  badge: "border-agent-claude/30 bg-agent-claude/10 text-agent-claude ring-1 ring-agent-claude/25",
};
const AMBER: ColorConfig = {
  dot: "bg-agent-codex",
  badge: "border-agent-codex/30 bg-agent-codex/10 text-agent-codex ring-1 ring-agent-codex/25",
};
const ZINC: ColorConfig = {
  dot: "bg-agent-system",
  badge: "border-agent-system/30 bg-agent-system/10 text-agent-system ring-1 ring-agent-system/25",
};

// Prefix rules in priority order: first match wins.
const PREFIX_COLORS: Array<[string, ColorConfig]> = [
  ["gpt-", TEAL],
  ["o1", TEAL],
  ["claude-", VIOLET],
  ["codex", AMBER],
  ["system", ZINC],
];

function resolveColor(key: string): ColorConfig {
  for (const [prefix, color] of PREFIX_COLORS) {
    if (key.startsWith(prefix)) return color;
  }
  return ZINC;
}

const ICONS: Record<string, React.ComponentType<{ className?: string }>> = {
  "claude-code": Sparkles,
  codex: Terminal,
  "gpt-4o": Sparkles,
  openai: Sparkles,
  o1: Sparkles,
  user: User,
  system: Bot,
};

export function AgentBadge({ agent }: { agent: string }) {
  const key = agent.toLowerCase();
  const Icon = ICONS[key] ?? Bot;
  const color = resolveColor(key);

  return (
    <Badge
      variant="outline"
      className={`gap-1.5 font-mono text-[11px] border ${color.badge}`}
    >
      {/* Signal 1: colored dot */}
      <span className={`size-1.5 rounded-full shrink-0 ${color.dot}`} aria-hidden />
      {/* Signal 2: icon */}
      <Icon className="size-3 shrink-0" aria-hidden />
      {/* Signal 3: label */}
      {agent}
    </Badge>
  );
}
