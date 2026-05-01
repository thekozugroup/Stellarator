"use client";

import { Suspense, useCallback, useMemo } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { useQuery } from "@tanstack/react-query";
import {
  Activity,
  CircleDollarSign,
  Cpu,
  FlaskConical,
  Rocket,
  TrendingDown,
} from "lucide-react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { PageContainer } from "@/components/ui/page-container";
import { OnboardingChecklist } from "@/components/onboarding-checklist";
import { PinnedRuns } from "@/components/pinned-runs";
import {
  RunsFilters,
  applyFilters,
  parseFiltersFromParams,
  type RunsFilterState,
} from "@/components/runs-filters";
import { RunsTable } from "@/components/runs-table";
import { RunsMultiselectActions } from "@/components/runs-multiselect-actions";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { fmtUsd } from "@/lib/format";
import type { Run } from "@/lib/types";

export default function DashboardPage() {
  return (
    <Suspense fallback={<div className="p-8 text-xs text-muted-foreground">Loading…</div>}>
      <Dashboard />
    </Suspense>
  );
}

function Dashboard() {
  const sp = useSearchParams();
  const router = useRouter();
  const showSandbox = sp.get("sandbox") === "1";

  const filterState = useMemo(
    () => parseFiltersFromParams(new URLSearchParams(sp.toString())),
    [sp],
  );

  const stats = useQuery({
    queryKey: ["stats"],
    queryFn: () => api.stats(),
    refetchInterval: 5_000,
  });
  const runs = useQuery({
    queryKey: ["runs"],
    queryFn: () => api.listRuns({ limit: 50 }),
    refetchInterval: 5_000,
  });

  const allRuns: Run[] = runs.data?.runs ?? [];
  const hasCompletedRuns = allRuns.some(
    (r) => r.status === "succeeded" || r.status === "failed",
  );

  // Monthly spend / budget — computed client-side from cost_so_far_usd,
  // gracefully defaulting until the budgets endpoint is wired.
  const monthlySpend = allRuns.reduce((s, r) => s + (r.cost_so_far_usd ?? 0), 0);
  const MONTHLY_BUDGET = 1000;
  const budgetPct = Math.min(100, (monthlySpend / MONTHLY_BUDGET) * 100);

  // By default hide sandbox runs; show them only when sandbox=1
  const visibleRuns = useMemo(
    () => (showSandbox ? allRuns : allRuns.filter((r) => !r.is_sandbox)),
    [allRuns, showSandbox],
  );

  const filtered = useMemo(() => applyFilters(visibleRuns, filterState), [visibleRuns, filterState]);

  // Selection state — persisted in URL as ?selected=id1,id2
  const selected = useMemo(() => {
    const raw = sp.get("selected");
    return raw ? raw.split(",").filter(Boolean) : [];
  }, [sp]);

  const setSelected = useCallback((ids: string[]) => {
    const next = new URLSearchParams(sp.toString());
    if (ids.length === 0) {
      next.delete("selected");
    } else {
      next.set("selected", ids.join(","));
    }
    const qs = next.toString();
    router.replace(qs ? `/?${qs}` : "/", { scroll: false });
  }, [sp, router]);

  function toggleSandbox() {
    const next = new URLSearchParams(sp.toString());
    if (showSandbox) next.delete("sandbox");
    else next.set("sandbox", "1");
    const qs = next.toString();
    router.replace(qs ? `/?${qs}` : "/", { scroll: false });
  }

  return (
    <PageContainer>
      <header className="flex items-end justify-between">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">Runs</h1>
          <p className="text-xs text-muted-foreground">
            Live training jobs across every owning agent.
          </p>
        </div>
        <button
          type="button"
          onClick={toggleSandbox}
          className={cn(
            "inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-[11px] font-medium transition-colors focus-visible:ring-2 focus-visible:ring-ring outline-none",
            showSandbox
              ? "border-warning/40 bg-warning/10 text-warning"
              : "border-border/60 bg-background text-muted-foreground hover:border-border hover:text-foreground",
          )}
          aria-pressed={showSandbox}
        >
          <FlaskConical className="size-3" />
          Sandbox runs
        </button>
      </header>

      <OnboardingChecklist hasCompletedRuns={hasCompletedRuns} />

      <section className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        <StatCard
          label="Active"
          value={stats.data ? String(stats.data.active_runs) : null}
          icon={<Activity className="size-3.5" />}
          accent="text-success"
        />
        <StatCard
          label="Spend today"
          value={stats.data ? fmtUsd(stats.data.spend_today_usd) : null}
          icon={<CircleDollarSign className="size-3.5" />}
          accent="text-muted-foreground"
        />
        <StatCard
          label="GPU-hrs"
          value={stats.data ? stats.data.gpu_hours_today.toFixed(1) : null}
          icon={<Cpu className="size-3.5" />}
        />
        <StatCard
          label="Avg loss Δ"
          value={
            stats.data
              ? `${stats.data.avg_loss_delta >= 0 ? "+" : ""}${stats.data.avg_loss_delta.toFixed(3)}`
              : null
          }
          icon={<TrendingDown className="size-3.5" />}
          accent={
            !stats.data
              ? undefined
              : stats.data.avg_loss_delta < 0
                ? "text-success"
                : stats.data.avg_loss_delta > 0
                  ? "text-destructive"
                  : "text-muted-foreground"
          }
        />
        <BudgetCard spend={monthlySpend} budget={MONTHLY_BUDGET} pct={budgetPct} />
      </section>

      <PinnedRuns runs={allRuns} />

      <RunsFilters runs={visibleRuns} state={filterState} />

      {!runs.isLoading && allRuns.length === 0 && !hasActiveFilters(filterState) ? (
        <div className="flex flex-col items-center justify-center gap-4 rounded-xl border border-dashed py-16 text-center">
          <Rocket className="size-8 text-muted-foreground/50" />
          <div>
            <h2 className="text-sm font-medium">No runs yet</h2>
            <p className="mt-1 text-xs text-muted-foreground">
              Launch your first training run from chat
            </p>
          </div>
          <Button asChild size="sm" className="transition-transform hover:-translate-y-0.5">
            <Link href="/chat">Open chat planner</Link>
          </Button>
        </div>
      ) : (
        <>
          <RunsMultiselectActions
            selected={selected}
            runs={filtered}
            onClear={() => setSelected([])}
          />
          <RunsTable
            runs={filtered}
            isLoading={runs.isLoading}
            selected={selected}
            onSelectionChange={setSelected}
          />
        </>
      )}

      {stats.error || runs.error ? (
        <p className="text-xs text-destructive">
          Backend unreachable at {api.apiUrl()}. Showing last known state.
        </p>
      ) : null}
    </PageContainer>
  );
}

function hasActiveFilters(f: RunsFilterState): boolean {
  return f.status.size > 0 || f.owner.size > 0 || f.method.size > 0 || f.q.trim().length > 0;
}

function BudgetCard({
  spend,
  budget,
  pct,
}: {
  spend: number;
  budget: number;
  pct: number;
}) {
  const tone =
    pct >= 100
      ? "bg-destructive"
      : pct >= 80
        ? "bg-warning"
        : "bg-primary";
  const accent =
    pct >= 100
      ? "text-destructive"
      : pct >= 80
        ? "text-warning"
        : "text-muted-foreground";
  return (
    <Card>
      <CardContent className="p-3">
        <div className="flex items-center justify-between text-micro uppercase tracking-[0.18em] text-muted-foreground">
          <span>Monthly</span>
          <span className={accent}>
            <CircleDollarSign className="size-3.5" />
          </span>
        </div>
        <div className="mt-1 text-lg font-semibold tabular-nums tracking-tight">
          {fmtUsd(spend)}
          <span className="ml-1 text-xs font-normal text-muted-foreground">
            / {fmtUsd(budget)}
          </span>
        </div>
        <div className="mt-2 h-1 w-full overflow-hidden rounded-full bg-muted">
          <div
            className={cn("h-full transition-all", tone)}
            style={{ width: `${Math.max(2, pct)}%` }}
          />
        </div>
      </CardContent>
    </Card>
  );
}

function StatCard({
  label,
  value,
  icon,
  accent,
}: {
  label: string;
  value: string | null;
  icon: React.ReactNode;
  accent?: string;
}) {
  return (
    <Card>
      <CardContent className="p-3">
        <div className="flex items-center justify-between text-micro uppercase tracking-[0.18em] text-muted-foreground">
          <span>{label}</span>
          <span className={accent ?? "text-muted-foreground"}>{icon}</span>
        </div>
        <div className="mt-1 text-lg font-semibold tabular-nums tracking-tight">
          {value ?? <Skeleton className="h-5 w-16" />}
        </div>
      </CardContent>
    </Card>
  );
}
