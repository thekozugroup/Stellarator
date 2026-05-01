"use client";

import { useMemo } from "react";
import { X } from "lucide-react";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { AgentBadge } from "@/components/agent-badge";
import { RunStatusBadge } from "@/components/run-status-badge";
import type { Run } from "@/lib/types";

export interface HyperparamDiffProps {
  runs: Run[];
  colors: string[];
  onRemove: (id: string) => void;
}

/**
 * HyperparamDiff — compares hyperparameters across runs.
 *
 * Diff highlighting: a cell is highlighted (yellow background) if its value
 * differs from the mode value across that row — NOT just column 0. This means
 * every outlier is flagged, not just the first run.
 */
export function HyperparamDiff({ runs, colors, onRemove }: HyperparamDiffProps) {
  const hpKeys = useMemo(() => {
    const keys = new Set<string>();
    runs.forEach((r) => Object.keys(r.hyperparams).forEach((k) => keys.add(k)));
    return Array.from(keys).sort();
  }, [runs]);

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className="pl-6">Param</TableHead>
          {runs.map((r, i) => (
            <TableHead key={r.id} className="text-xs">
              <div className="flex items-center gap-2">
                <span
                  className="size-2 rounded-full shrink-0"
                  style={{ background: colors[i % colors.length] }}
                  aria-hidden
                />
                <span className="truncate">{r.name}</span>
                <button
                  type="button"
                  onClick={() => onRemove(r.id)}
                  className="text-muted-foreground hover:text-foreground"
                  aria-label={`Remove ${r.name} from comparison`}
                >
                  <X className="size-3" />
                </button>
              </div>
              <div className="mt-1 flex items-center gap-1.5">
                <AgentBadge agent={r.owner_agent} />
                <RunStatusBadge status={r.status} />
              </div>
            </TableHead>
          ))}
        </TableRow>
      </TableHeader>
      <TableBody>
        {hpKeys.map((k) => {
          const values = runs.map((r) => r.hyperparams[k]);
          const serialized = values.map((v) => JSON.stringify(v));

          // Compute mode (most frequent serialized value among defined entries)
          const freq = new Map<string, number>();
          serialized.forEach((s) => {
            if (s !== undefined) freq.set(s, (freq.get(s) ?? 0) + 1);
          });
          let modeVal: string | undefined;
          let modeCount = 0;
          freq.forEach((count, val) => {
            if (count > modeCount) {
              modeCount = count;
              modeVal = val;
            }
          });

          const allSame = freq.size <= 1;

          return (
            <TableRow key={k}>
              <TableCell className="pl-6 font-mono text-xs tabular-nums">{k}</TableCell>
              {values.map((v, i) => {
                // Only highlight cells that are present AND differ from the mode.
                // Missing values (undefined) get neutral styling — no diff highlight.
                const isMissing = v === undefined;
                const isDiff = !isMissing && !allSame && serialized[i] !== modeVal;
                return (
                  <TableCell
                    key={i}
                    className={`font-mono text-xs tabular-nums transition-colors ${
                      isDiff ? "bg-warning/15 text-warning" : ""
                    }`}
                    aria-label={isDiff ? `${k} differs: ${String(v)}` : undefined}
                  >
                    {isMissing ? (
                      <span className="text-muted-foreground">—</span>
                    ) : (
                      <span>{String(v)}</span>
                    )}
                  </TableCell>
                );
              })}
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}
