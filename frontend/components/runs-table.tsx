"use client";

import * as React from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { ArrowUpRight, FlaskConical, Star } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { AgentBadge } from "@/components/agent-badge";
import { RunStatusBadge } from "@/components/run-status-badge";
import { fmtDuration, fmtUsd } from "@/lib/format";
import { usePrefs, type Density } from "@/lib/local-prefs";
import type { Run } from "@/lib/types";

export function RunsTable({
  runs,
  isLoading,
  selected,
  onSelectionChange,
}: {
  runs: Run[];
  isLoading: boolean;
  selected: string[];
  onSelectionChange: (ids: string[]) => void;
}): React.ReactElement {
  const { prefs, setPrefs, isPinned, togglePin } = usePrefs();
  const density: Density = prefs.density;
  const router = useRouter();
  const lastSelectedIdxRef = React.useRef<number>(-1);
  const rowH = density === "compact" ? "h-7" : "h-12";
  const cellPad = density === "compact" ? "py-0" : "py-2";
  const textSize = density === "compact" ? "text-xs" : "text-sm";

  const selectMode = selected.length > 0;
  const allSelected = runs.length > 0 && runs.every((r) => selected.includes(r.id));
  const someSelected = !allSelected && runs.some((r) => selected.includes(r.id));

  function toggleAll() {
    if (allSelected) {
      onSelectionChange([]);
    } else {
      onSelectionChange(runs.map((r) => r.id));
    }
  }

  function toggleRow(id: string, idx: number, shiftKey: boolean) {
    if (shiftKey && lastSelectedIdxRef.current >= 0) {
      const lo = Math.min(idx, lastSelectedIdxRef.current);
      const hi = Math.max(idx, lastSelectedIdxRef.current);
      const range = runs.slice(lo, hi + 1).map((r) => r.id);
      const set = new Set([...selected, ...range]);
      onSelectionChange([...set]);
    } else {
      const set = new Set(selected);
      if (set.has(id)) {
        set.delete(id);
      } else {
        set.add(id);
      }
      onSelectionChange([...set]);
    }
    lastSelectedIdxRef.current = idx;
  }

  React.useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape" && selectMode) {
        onSelectionChange([]);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selectMode, onSelectionChange]);

  const COL_COUNT = 11; // checkbox + pin + 9 data cols

  return (
    <div className="overflow-hidden rounded-lg border border-border/60 bg-card">
      <div className="flex items-center justify-between border-b border-border/60 px-3 py-1.5 text-[11px] text-muted-foreground">
        <span className="flex items-center gap-2 font-mono tabular-nums">
          {isLoading ? "loading…" : `${runs.length} runs`}
          {selectMode ? (
            <span className="inline-flex items-center gap-1.5 rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
              Select mode
              <kbd className="rounded bg-primary/20 px-1 font-mono text-[9px]">Esc</kbd>
              to clear
            </span>
          ) : null}
        </span>
        <DensityToggle
          value={density}
          onChange={(d) => setPrefs({ density: d })}
        />
      </div>
      <div className="overflow-x-auto">
      <Table>
        <TableHeader>
          <TableRow className="border-border/50 hover:bg-transparent">
            <TableHead className="w-6 pl-3 pr-1">
              <TriCheckbox
                checked={allSelected}
                indeterminate={someSelected}
                onChange={toggleAll}
                aria-label="Select all visible runs"
              />
            </TableHead>
            <TableHead className="w-8 pl-1 pr-0">
              <span className="sr-only">Pin</span>
            </TableHead>
            <TableHead className="min-w-[18rem]">Name</TableHead>
            <TableHead className="w-28">Owner</TableHead>
            <TableHead className="w-24">Method</TableHead>
            <TableHead className="w-28">Status</TableHead>
            <TableHead className="w-24">GPU</TableHead>
            <TableHead className="w-20 text-right">Cost</TableHead>
            <TableHead className="w-24">Started</TableHead>
            <TableHead className="w-32">Last loss</TableHead>
            <TableHead className="w-8 pr-3" />
          </TableRow>
        </TableHeader>
        <TableBody>
          {isLoading
            ? Array.from({ length: 8 }).map((_, i) => (
                <TableRow key={`s-${i}`} className={rowH}>
                  {Array.from({ length: COL_COUNT }).map((__, j) => (
                    <TableCell key={j} className={cellPad}>
                      <Skeleton className="h-3 w-full max-w-[140px]" />
                    </TableCell>
                  ))}
                </TableRow>
              ))
            : null}

          {!isLoading && runs.length === 0 ? (
            <TableRow>
              <TableCell colSpan={COL_COUNT} className="py-12 text-center text-xs text-muted-foreground">
                No runs match your filters.
              </TableCell>
            </TableRow>
          ) : null}

          {!isLoading &&
            runs.map((r, idx) => {
              const pinned = isPinned(r.id);
              const isSelected = selected.includes(r.id);

              const onKey = (e: React.KeyboardEvent<HTMLTableRowElement>): void => {
                if (e.target !== e.currentTarget) return;
                if (e.key === " ") {
                  e.preventDefault();
                  toggleRow(r.id, idx, false);
                  return;
                }
                if (e.key === "Enter") {
                  e.preventDefault();
                  if (!selectMode) router.push(`/runs/${r.id}`);
                }
              };

              const handleRowClick = (e: React.MouseEvent<HTMLTableRowElement>) => {
                const target = e.target as HTMLElement;
                if (target.closest("a, button, input")) return;
                if (selectMode) {
                  toggleRow(r.id, idx, e.shiftKey);
                } else {
                  router.push(`/runs/${r.id}`);
                }
              };

              return (
                <TableRow
                  key={r.id}
                  tabIndex={0}
                  onKeyDown={onKey}
                  onClick={handleRowClick}
                  aria-label={`${isSelected ? "Deselect" : "Select or open"} run ${r.name}`}
                  aria-selected={isSelected}
                  className={cn(
                    rowH,
                    "border-border/40 transition-colors focus-visible:bg-accent/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
                    isSelected ? "bg-primary/5" : "data-[pinned=true]:bg-warning/5",
                    selectMode && "cursor-pointer",
                  )}
                  data-pinned={pinned}
                >
                  <TableCell className={cn("w-6 pl-3 pr-1", cellPad)}>
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={(e) => {
                        const shift = e.nativeEvent instanceof MouseEvent && (e.nativeEvent as MouseEvent).shiftKey;
                        toggleRow(r.id, idx, shift);
                      }}
                      onClick={(e) => e.stopPropagation()}
                      aria-label={`Select run ${r.name}`}
                      className="size-4 accent-primary cursor-pointer"
                    />
                  </TableCell>
                  <TableCell className={cn("pl-1 pr-0", cellPad)}>
                    <button
                      type="button"
                      onClick={(e) => { e.stopPropagation(); togglePin(r.id); }}
                      aria-label={pinned ? `Unpin ${r.name}` : `Pin ${r.name}`}
                      aria-pressed={pinned}
                      className="grid size-5 place-items-center rounded text-muted-foreground/50 hover:text-warning focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      <Star
                        className={cn(
                          "size-3.5 transition-colors",
                          pinned && "fill-warning text-warning",
                        )}
                      />
                    </button>
                  </TableCell>
                  <TableCell className={cn("min-w-0 pr-2", cellPad)}>
                    <Link
                      href={`/runs/${r.id}`}
                      onClick={(e) => { if (selectMode) e.preventDefault(); }}
                      className={cn("flex min-w-0 items-baseline gap-2 text-foreground outline-none hover:text-primary focus-visible:underline", textSize)}
                    >
                      <span className="truncate font-medium">{r.name}</span>
                      {r.is_sandbox ? (
                        <TooltipProvider>
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <span className="inline-flex shrink-0 items-center gap-0.5 rounded border border-warning/40 bg-warning/10 px-1 py-0.5 font-mono text-[9px] font-bold uppercase tracking-wider text-warning">
                                <FlaskConical className="size-2.5" />S
                              </span>
                            </TooltipTrigger>
                            <TooltipContent side="top" className="max-w-xs text-xs">
                              Sandbox run — used for pre-flight validation, not promoted to
                              production.
                            </TooltipContent>
                          </Tooltip>
                        </TooltipProvider>
                      ) : null}
                      <span className="ml-auto shrink-0 truncate font-mono text-[10px] text-muted-foreground">
                        {r.base_model}
                      </span>
                    </Link>
                  </TableCell>
                  <TableCell className={cellPad}>
                    <AgentBadge agent={r.owner_agent} />
                  </TableCell>
                  <TableCell className={cn("font-mono", textSize, cellPad)}>{r.method}</TableCell>
                  <TableCell className={cellPad}>
                    <RunStatusBadge status={r.status} />
                    <span className="sr-only">Status: {r.status}</span>
                  </TableCell>
                  <TableCell className={cn("font-mono", textSize, cellPad)}>{r.gpu}</TableCell>
                  <TableCell
                    className={cn("text-right font-mono tabular-nums", textSize, cellPad)}
                  >
                    {fmtUsd(r.cost_so_far_usd)}
                  </TableCell>
                  <TableCell
                    className={cn("font-mono tabular-nums text-muted-foreground", textSize, cellPad)}
                  >
                    {r.started_at ? `${fmtDuration(r.started_at)} ago` : "—"}
                  </TableCell>
                  <TableCell className={cn(textSize, cellPad)}>
                    {r.last_metric_loss != null ? (
                      <span className="font-mono tabular-nums">
                        s{r.last_metric_step} · {r.last_metric_loss.toFixed(4)}
                      </span>
                    ) : (
                      <span className="text-muted-foreground">—</span>
                    )}
                  </TableCell>
                  <TableCell className={cn("pr-3", cellPad)}>
                    <Link
                      href={`/runs/${r.id}`}
                      onClick={(e) => { if (selectMode) e.preventDefault(); }}
                      aria-label={`Open ${r.name}`}
                      className="grid size-5 place-items-center rounded text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      <ArrowUpRight className="size-3.5" />
                    </Link>
                  </TableCell>
                </TableRow>
              );
            })}
        </TableBody>
      </Table>
      </div>
    </div>
  );
}

function TriCheckbox({
  checked,
  indeterminate,
  onChange,
  "aria-label": ariaLabel,
}: {
  checked: boolean;
  indeterminate: boolean;
  onChange: () => void;
  "aria-label"?: string;
}) {
  const ref = React.useRef<HTMLInputElement>(null);
  React.useEffect(() => {
    if (ref.current) {
      ref.current.indeterminate = indeterminate;
    }
  }, [indeterminate]);

  return (
    <input
      ref={ref}
      type="checkbox"
      checked={checked}
      onChange={onChange}
      aria-label={ariaLabel}
      className="size-4 accent-primary cursor-pointer"
    />
  );
}

function DensityToggle({
  value,
  onChange,
}: {
  value: Density;
  onChange: (d: Density) => void;
}): React.ReactElement {
  return (
    <div
      role="radiogroup"
      aria-label="Row density"
      className="flex items-center rounded-md border border-border/60 p-0.5"
    >
      {(["compact", "comfy"] as const).map((d) => (
        <button
          key={d}
          type="button"
          role="radio"
          aria-checked={value === d}
          onClick={() => onChange(d)}
          className={cn(
            "rounded px-2 py-0.5 text-[10px] uppercase tracking-wider transition-colors",
            value === d
              ? "bg-accent text-foreground"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {d}
        </button>
      ))}
    </div>
  );
}
