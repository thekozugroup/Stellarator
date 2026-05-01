"use client";

import { FlaskConical, Rocket, Search, TrendingDown, Wand2 } from "lucide-react";

interface Prompt {
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  prompt: string;
  subtitle: string;
}

// 4 example prompts that walk the agent through Phases 1–6 of the ML Intern loop.
const PROMPTS: Prompt[] = [
  {
    icon: Rocket,
    title: "Replicate a paper end-to-end",
    subtitle: "Phases 1–6 · research → sandbox → preflight → run",
    prompt:
      "Replicate the SFT baseline from arXiv 2412.04567 on Llama-3.1-8B (research → sandbox → preflight → run)",
  },
  {
    icon: FlaskConical,
    title: "Sandbox first, then promote",
    subtitle: "Phases 2–4 · gated promotion via DPO sandbox",
    prompt:
      "Run a small DPO sandbox first; if loss looks good, promote to production with 4×H100",
  },
  {
    icon: Search,
    title: "Hyperparam sweep",
    subtitle: "Phase 3 · LoRA rank ablation, pick by eval_loss",
    prompt:
      "Sweep LoRA r ∈ {16, 32, 64} as sandboxes; pick the best by eval_loss",
  },
  {
    icon: TrendingDown,
    title: "Iterate on a previous run",
    subtitle: "Phases 1, 5–6 · research deltas, then improve",
    prompt:
      "Improve my last DPO run: research what changed in recent papers, then iterate",
  },
];

export function Welcome({ onPick }: { onPick: (text: string) => void }) {
  return (
    <div className="mx-auto max-w-2xl px-4 py-16">
      <div className="mb-6 flex items-center gap-3">
        <div className="grid size-9 place-items-center rounded-full bg-primary/15 text-primary ring-1 ring-primary/30">
          <Wand2 className="size-4" />
        </div>
        <div>
          <h2 className="text-base font-semibold tracking-tight">Plan a training run</h2>
          <p className="text-xs text-muted-foreground">
            Pick an example to walk the 6-phase ML Intern loop, or describe your own goal.
          </p>
        </div>
      </div>
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
        {PROMPTS.map((p) => {
          const Icon = p.icon;
          return (
            <button
              key={p.prompt}
              type="button"
              onClick={() => onPick(p.prompt)}
              className="group flex items-start gap-3 rounded-lg border border-border/60 bg-card/50 px-3.5 py-3 text-left transition-colors hover:border-primary/40 hover:bg-card hover:shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <div className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-full bg-primary/10 text-primary">
                <Icon className="size-3.5" />
              </div>
              <div className="min-w-0">
                <p className="text-xs font-medium text-muted-foreground group-hover:text-foreground">
                  {p.title}
                </p>
                <p className="mt-0.5 line-clamp-2 text-sm leading-snug text-foreground group-hover:text-foreground">
                  {p.prompt}
                </p>
                <p className="mt-1 truncate text-[10px] uppercase tracking-wider text-muted-foreground/70">
                  {p.subtitle}
                </p>
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}
