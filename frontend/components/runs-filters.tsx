"use client";

import * as React from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Search, X } from "lucide-react";
import { cn } from "@/lib/utils";
import type { Run, RunStatus } from "@/lib/types";

const STATUSES: RunStatus[] = [
  "queued",
  "provisioning",
  "running",
  "paused",
  "succeeded",
  "failed",
  "cancelled",
];

export interface RunsFilterState {
  status: Set<RunStatus>;
  owner: Set<string>;
  method: Set<string>;
  q: string;
}

export function parseFiltersFromParams(sp: URLSearchParams): RunsFilterState {
  const split = (key: string): Set<string> => {
    const raw = sp.get(key);
    if (!raw) return new Set<string>();
    return new Set(raw.split(",").filter(Boolean));
  };
  return {
    status: split("status") as Set<RunStatus>,
    owner: split("owner"),
    method: split("method"),
    q: sp.get("q") ?? "",
  };
}

export function applyFilters(runs: Run[], f: RunsFilterState): Run[] {
  const q = f.q.trim().toLowerCase();
  return runs.filter((r) => {
    if (f.status.size && !f.status.has(r.status)) return false;
    if (f.owner.size && !f.owner.has(r.owner_agent)) return false;
    if (f.method.size && !f.method.has(r.method)) return false;
    if (q) {
      const hay = `${r.name} ${r.id} ${r.base_model} ${r.method} ${r.owner_agent}`.toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  });
}

export function RunsFilters({
  runs,
  state,
}: {
  runs: Run[];
  state: RunsFilterState;
}): React.ReactElement {
  const router = useRouter();
  const sp = useSearchParams();

  const owners = React.useMemo<string[]>(
    () => uniqueSorted(runs.map((r) => r.owner_agent)),
    [runs],
  );
  const methods = React.useMemo<string[]>(
    () => uniqueSorted(runs.map((r) => r.method)),
    [runs],
  );

  const update = React.useCallback(
    (mut: (q: URLSearchParams) => void): void => {
      const next = new URLSearchParams(sp.toString());
      mut(next);
      const qs = next.toString();
      router.replace(qs ? `/?${qs}` : "/", { scroll: false });
    },
    [router, sp],
  );

  const toggle = (key: "status" | "owner" | "method", value: string): void => {
    update((q) => {
      const cur = new Set((q.get(key) ?? "").split(",").filter(Boolean));
      if (cur.has(value)) cur.delete(value);
      else cur.add(value);
      if (cur.size) q.set(key, [...cur].join(","));
      else q.delete(key);
    });
  };

  const setQ = (v: string): void => {
    update((q) => {
      if (v) q.set("q", v);
      else q.delete("q");
    });
  };

  const clearAll = (): void => {
    update((q) => {
      q.delete("status");
      q.delete("owner");
      q.delete("method");
      q.delete("q");
    });
  };

  const activeCount =
    state.status.size + state.owner.size + state.method.size + (state.q ? 1 : 0);

  return (
    <div
      role="region"
      aria-label="Filters"
      className="flex flex-wrap items-center gap-x-3 gap-y-2"
    >
      <div className="relative">
        <Search
          aria-hidden
          className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground"
        />
        <input
          type="search"
          aria-label="Search runs"
          placeholder="Search name, id, model..."
          value={state.q}
          onChange={(e) => setQ(e.target.value)}
          className="h-7 w-60 rounded-md border border-border/60 bg-background pl-8 pr-2 text-xs outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"
        />
      </div>

      <ChipGroup label="Status">
        {STATUSES.map((s) => (
          <Chip
            key={s}
            active={state.status.has(s)}
            onClick={() => toggle("status", s)}
            aria-pressed={state.status.has(s)}
          >
            {s}
          </Chip>
        ))}
      </ChipGroup>

      {owners.length > 0 ? (
        <ChipGroup label="Owner">
          {owners.map((o) => (
            <Chip
              key={o}
              active={state.owner.has(o)}
              onClick={() => toggle("owner", o)}
              aria-pressed={state.owner.has(o)}
            >
              {o}
            </Chip>
          ))}
        </ChipGroup>
      ) : null}

      {methods.length > 0 ? (
        <ChipGroup label="Method">
          {methods.map((m) => (
            <Chip
              key={m}
              active={state.method.has(m)}
              onClick={() => toggle("method", m)}
              aria-pressed={state.method.has(m)}
            >
              {m}
            </Chip>
          ))}
        </ChipGroup>
      ) : null}

      {activeCount > 0 ? (
        <button
          type="button"
          onClick={clearAll}
          className="ml-auto inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
        >
          <X className="size-3" /> Clear ({activeCount})
        </button>
      ) : null}
    </div>
  );
}

function ChipGroup({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div className="flex items-center gap-1.5" role="group" aria-label={label}>
      <span className="text-[10px] uppercase tracking-[0.18em] text-muted-foreground/80">
        {label}
      </span>
      <div className="flex flex-wrap gap-1">{children}</div>
    </div>
  );
}

function Chip({
  active,
  children,
  onClick,
  ...rest
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { active: boolean }): React.ReactElement {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "h-6 rounded-full border px-2 text-[11px] font-mono lowercase tracking-tight transition-colors focus-visible:ring-2 focus-visible:ring-ring",
        active
          ? "border-primary/40 bg-primary/15 text-primary"
          : "border-border/60 bg-background text-muted-foreground hover:border-border hover:text-foreground",
      )}
      {...rest}
    >
      {children}
    </button>
  );
}

function uniqueSorted(values: string[]): string[] {
  return [...new Set(values)].sort((a, b) => a.localeCompare(b));
}
