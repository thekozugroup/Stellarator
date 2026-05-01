"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { useQueries } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { AlertCircle, AlertTriangle, Info, Search } from "lucide-react";
import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import { PageContainer } from "@/components/ui/page-container";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import { api } from "@/lib/api";
import { markAlertsSeen } from "@/lib/notifications";
import type { Run, RunAlert } from "@/lib/types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------
type AlertLevel = "error" | "warn" | "info";
type TimeRange = "hour" | "day" | "week" | "all";

interface AggregatedAlert extends RunAlert {
  runId: string;
  runName: string;
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------
const LEVEL_CONFIG = {
  error: {
    icon: AlertCircle,
    badgeVariant: "destructive" as const,
    border: "border-destructive/30 bg-destructive/5",
    iconClass: "text-destructive",
    label: "Error",
  },
  warn: {
    icon: AlertTriangle,
    badgeVariant: "outline" as const,
    border: "border-warning/30 bg-warning/5",
    iconClass: "text-warning",
    label: "Warn",
  },
  info: {
    icon: Info,
    badgeVariant: "secondary" as const,
    border: "border-border/40 bg-muted/30",
    iconClass: "text-muted-foreground",
    label: "Info",
  },
} satisfies Record<AlertLevel, { icon: React.ComponentType<{ className?: string }>; badgeVariant: "destructive" | "outline" | "secondary"; border: string; iconClass: string; label: string }>;

const TIME_RANGE_OPTIONS: { value: TimeRange; label: string }[] = [
  { value: "hour", label: "Last hour" },
  { value: "day", label: "Last day" },
  { value: "week", label: "Last week" },
  { value: "all", label: "All time" },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const s = Math.floor(diff / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

function timeRangeMs(range: TimeRange): number {
  switch (range) {
    case "hour": return 60 * 60 * 1000;
    case "day": return 24 * 60 * 60 * 1000;
    case "week": return 7 * 24 * 60 * 60 * 1000;
    case "all": return Infinity;
  }
}

// ---------------------------------------------------------------------------
// Alert row
// ---------------------------------------------------------------------------
function AlertRow({ alert }: { alert: AggregatedAlert }) {
  const cfg = LEVEL_CONFIG[alert.level];
  const Icon = cfg.icon;

  return (
    <div className={cn("rounded-lg border px-4 py-3", cfg.border)}>
      <div className="flex flex-wrap items-start gap-2.5">
        <Icon className={cn("mt-0.5 size-4 shrink-0", cfg.iconClass)} aria-hidden />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant={cfg.badgeVariant} className="text-[10px] uppercase tracking-wide">
              {cfg.label}
            </Badge>
            <Link
              href={`/runs/${alert.runId}`}
              className="text-[11px] text-muted-foreground hover:text-foreground underline underline-offset-2 transition-colors"
            >
              {alert.runName}
            </Link>
            <span className="font-medium text-sm text-foreground">{alert.title}</span>
            {alert.source ? (
              <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-mono text-muted-foreground">
                {alert.source}
              </span>
            ) : null}
            <span className="ml-auto text-[10px] tabular-nums text-muted-foreground">
              {relativeTime(alert.created_at)}
            </span>
          </div>
          {alert.body ? (
            <p className="mt-1 text-xs text-muted-foreground leading-relaxed line-clamp-2">
              {alert.body}
            </p>
          ) : null}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------
export default function AlertsPage() {
  const [levelFilter, setLevelFilter] = useState<Set<AlertLevel>>(new Set());
  const [sourceFilter, setSourceFilter] = useState("");
  const [timeRange, setTimeRange] = useState<TimeRange>("all");
  const [search, setSearch] = useState("");

  // Mark alerts seen on mount
  useEffect(() => {
    markAlertsSeen();
  }, []);

  // Step 1: fetch all runs (limit 200)
  const runsQuery = useQueries({
    queries: [
      {
        queryKey: ["runs-for-alerts"],
        queryFn: () => api.listRuns({ limit: 200 }),
        refetchInterval: 30_000,
      },
    ],
  });

  const runs: Run[] = runsQuery[0]?.data?.runs ?? [];
  const runsLoading = runsQuery[0]?.isLoading ?? true;

  // Step 2: fetch alerts for each run in parallel
  const alertQueries = useQueries({
    queries: runs.map((r) => ({
      queryKey: ["run-alerts", r.id],
      queryFn: () => api.getRunAlerts(r.id),
      staleTime: 15_000,
      refetchInterval: 30_000,
    })),
  });

  const allLoading = runsLoading || alertQueries.some((q) => q.isLoading && !q.data);

  // Step 3: aggregate, sort desc, cap at 500
  const aggregated = useMemo((): AggregatedAlert[] => {
    const items: AggregatedAlert[] = [];
    runs.forEach((run, i) => {
      const alerts = alertQueries[i]?.data?.alerts ?? [];
      alerts.forEach((a) => {
        items.push({ ...a, runId: run.id, runName: run.name });
      });
    });
    items.sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime());
    return items.slice(0, 500);
  }, [runs, alertQueries]);

  // Step 4: apply filters
  const filtered = useMemo(() => {
    const cutoff = timeRange === "all" ? 0 : Date.now() - timeRangeMs(timeRange);
    return aggregated.filter((a) => {
      if (levelFilter.size > 0 && !levelFilter.has(a.level)) return false;
      if (sourceFilter && a.source !== sourceFilter) return false;
      if (new Date(a.created_at).getTime() < cutoff) return false;
      if (search) {
        const q = search.toLowerCase();
        if (!a.title.toLowerCase().includes(q) && !(a.runName.toLowerCase().includes(q))) return false;
      }
      return true;
    });
  }, [aggregated, levelFilter, sourceFilter, timeRange, search]);

  // Unique sources for filter
  const sources = useMemo(() => {
    const set = new Set<string>();
    aggregated.forEach((a) => { if (a.source) set.add(a.source); });
    return [...set].sort();
  }, [aggregated]);

  function toggleLevel(level: AlertLevel) {
    setLevelFilter((prev) => {
      const next = new Set(prev);
      if (next.has(level)) next.delete(level);
      else next.add(level);
      return next;
    });
  }

  return (
    <PageContainer>
      <header>
        <h1 className="text-xl font-semibold tracking-tight">Alerts</h1>
        <p className="mt-0.5 text-xs text-muted-foreground">
          All alerts emitted by your training scripts.
        </p>
      </header>

      {/* Filters */}
      <div className="flex flex-wrap items-center gap-2">
        {/* Level filter */}
        {(["error", "warn", "info"] as AlertLevel[]).map((lvl) => {
          const cfg = LEVEL_CONFIG[lvl];
          const active = levelFilter.has(lvl);
          return (
            <button
              key={lvl}
              type="button"
              onClick={() => toggleLevel(lvl)}
              className={cn(
                "inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-[11px] font-medium transition-colors focus-visible:ring-2 focus-visible:ring-ring outline-none",
                active
                  ? "border-primary/40 bg-primary/10 text-primary"
                  : "border-border/60 bg-background text-muted-foreground hover:border-border hover:text-foreground",
              )}
              aria-pressed={active}
            >
              {cfg.label}
            </button>
          );
        })}

        {/* Source filter */}
        {sources.length > 0 ? (
          <select
            value={sourceFilter}
            onChange={(e) => setSourceFilter(e.target.value)}
            className="h-7 rounded-full border border-border/60 bg-background px-3 text-[11px] text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
            aria-label="Filter by source"
          >
            <option value="">All sources</option>
            {sources.map((s) => (
              <option key={s} value={s}>{s}</option>
            ))}
          </select>
        ) : null}

        {/* Time range */}
        <select
          value={timeRange}
          onChange={(e) => setTimeRange(e.target.value as TimeRange)}
          className="h-7 rounded-full border border-border/60 bg-background px-3 text-[11px] text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          aria-label="Filter by time range"
        >
          {TIME_RANGE_OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>

        {/* Search */}
        <div className="relative ml-auto">
          <Search className="absolute left-2.5 top-1/2 size-3 -translate-y-1/2 text-muted-foreground" />
          <input
            type="search"
            placeholder="Search by title or run…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="h-7 w-56 rounded-full border border-border/60 bg-background pl-7 pr-3 text-[11px] text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          />
        </div>
      </div>

      {/* Body */}
      {allLoading ? (
        <div className="space-y-2">
          {[1, 2, 3, 4, 5].map((i) => (
            <Skeleton key={i} className="h-14 w-full rounded-lg" />
          ))}
        </div>
      ) : filtered.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border/50 p-16 text-center">
          <p className="text-sm font-medium text-muted-foreground">No alerts yet</p>
          <p className="mt-1 text-xs text-muted-foreground">
            Your training scripts will post here via{" "}
            <code className="font-mono">trackio.alert(...)</code>. See{" "}
            <code className="font-mono">docs/agent-loop.md</code>.
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          <p className="text-[11px] text-muted-foreground tabular-nums">
            {filtered.length} alert{filtered.length !== 1 ? "s" : ""}
            {aggregated.length > filtered.length ? ` (of ${aggregated.length})` : ""}
          </p>
          <VirtualAlertList alerts={filtered} />
        </div>
      )}
    </PageContainer>
  );
}

function VirtualAlertList({ alerts }: { alerts: AggregatedAlert[] }): React.ReactElement {
  const parentRef = useRef<HTMLDivElement | null>(null);
  const virtualizer = useVirtualizer({
    count: alerts.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 92,
    overscan: 8,
  });
  return (
    <div
      ref={parentRef}
      className="max-h-[70vh] overflow-y-auto rounded-md border border-border/40"
    >
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: "100%",
          position: "relative",
        }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const alert = alerts[virtualRow.index];
          return (
            <div
              key={`${alert.runId}-${alert.created_at}-${virtualRow.index}`}
              data-index={virtualRow.index}
              ref={virtualizer.measureElement}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                transform: `translateY(${virtualRow.start}px)`,
              }}
              className="px-2 py-1"
            >
              <AlertRow alert={alert} />
            </div>
          );
        })}
      </div>
    </div>
  );
}
