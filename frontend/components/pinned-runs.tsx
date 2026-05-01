"use client";

import * as React from "react";
import Link from "next/link";
import { ChevronDown, Pin, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { AgentBadge } from "@/components/agent-badge";
import { RunStatusBadge } from "@/components/run-status-badge";
import { fmtUsd } from "@/lib/format";
import { usePrefs } from "@/lib/local-prefs";
import type { Run } from "@/lib/types";

export function PinnedRuns({ runs }: { runs: Run[] }): React.ReactElement | null {
  const { prefs, setPrefs, togglePin } = usePrefs();
  const pinnedIds = new Set(prefs.pinnedRuns);
  const pinned = runs.filter((r) => pinnedIds.has(r.id));

  if (pinned.length === 0) return null;

  const open = prefs.pinnedOpen;

  return (
    <section
      aria-label="Pinned runs"
      id="pinned"
      className="rounded-lg border border-warning/20 bg-warning/[0.04]"
    >
      <button
        type="button"
        onClick={() => setPrefs({ pinnedOpen: !open })}
        aria-expanded={open}
        aria-controls="pinned-grid"
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-warning outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <Pin className="size-3.5 fill-warning/30 text-warning" />
        <span className="font-medium">Pinned</span>
        <span className="font-mono text-micro tabular-nums text-warning/70">
          {pinned.length}
        </span>
        <ChevronDown
          className={cn(
            "ml-auto size-3.5 text-warning/60 transition-transform",
            !open && "-rotate-90",
          )}
        />
      </button>

      {open ? (
        <div
          id="pinned-grid"
          className="grid grid-cols-1 gap-2 border-t border-warning/15 p-2 sm:grid-cols-2 xl:grid-cols-3"
        >
          {pinned.map((r) => (
            <div
              key={r.id}
              className="group relative flex items-center gap-3 rounded-md border border-border/40 bg-card px-2.5 py-1.5 hover:border-border"
            >
              <Link
                href={`/runs/${r.id}`}
                className="flex min-w-0 flex-1 flex-col gap-0.5 outline-none focus-visible:underline"
              >
                <div className="flex min-w-0 items-center gap-2">
                  <span className="truncate text-sm font-medium">{r.name}</span>
                  <span className="ml-auto shrink-0 font-mono text-micro text-muted-foreground">
                    {fmtUsd(r.cost_so_far_usd)}
                  </span>
                </div>
                <div className="flex items-center gap-1.5 text-micro text-muted-foreground">
                  <RunStatusBadge status={r.status} />
                  <AgentBadge agent={r.owner_agent} />
                  <span className="font-mono">{r.method}</span>
                </div>
              </Link>
              <button
                type="button"
                onClick={() => togglePin(r.id)}
                aria-label={`Unpin ${r.name}`}
                className="grid size-6 shrink-0 place-items-center rounded text-muted-foreground opacity-0 transition-opacity hover:bg-accent hover:text-foreground focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring group-hover:opacity-100"
              >
                <X className="size-3.5" />
              </button>
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}
