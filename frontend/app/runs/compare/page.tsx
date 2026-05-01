"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import { useQueries, useQuery } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { Check, Plus, Share2 } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { PageContainer } from "@/components/ui/page-container";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { RunOverlayChart } from "@/components/compare/run-overlay-chart";
import { HyperparamDiff } from "@/components/compare/hyperparam-diff";
import { api } from "@/lib/api";

const PALETTE = [
  "var(--color-chart-1)",
  "var(--color-chart-2)",
  "var(--color-chart-3)",
  "var(--color-chart-4)",
  "var(--color-chart-5)",
  "oklch(0.78 0.16 320)",
];

const MAX = 6;

export default function ComparePageWrapper() {
  return (
    <Suspense fallback={<div className="p-8 text-sm text-muted-foreground">Loading…</div>}>
      <ComparePage />
    </Suspense>
  );
}

function ComparePage() {
  const router = useRouter();
  const search = useSearchParams();
  const initialIds = useMemo(
    () => (search.get("ids")?.split(",").filter(Boolean) ?? []).slice(0, MAX),
    [search],
  );
  const [selected, setSelected] = useState<string[]>(initialIds);

  useEffect(() => {
    const params = new URLSearchParams();
    if (selected.length) params.set("ids", selected.join(","));
    router.replace(`/runs/compare${params.toString() ? `?${params}` : ""}`);
  }, [selected, router]);

  const all = useQuery({ queryKey: ["runs"], queryFn: () => api.listRuns({ limit: 100 }) });

  const metricsQueries = useQueries({
    queries: selected.map((id) => ({
      queryKey: ["run-metrics", id],
      queryFn: () => api.getRunMetrics(id),
    })),
  });

  const runs = (all.data?.runs ?? []).filter((r) => selected.includes(r.id));

  const series = useMemo(
    () =>
      selected.map((id, i) => {
        const run = runs.find((r) => r.id === id);
        const metrics = metricsQueries[i]?.data?.metrics ?? [];
        return {
          id,
          name: run?.name ?? id.slice(0, 8),
          color: PALETTE[i % PALETTE.length],
          metrics,
        };
      }),
    [selected, runs, metricsQueries],
  );

  const hasAnyMetrics = series.some((s) => s.metrics.length > 0);

  function shareUrl() {
    const url = `${window.location.origin}/runs/compare?ids=${selected.join(",")}`;
    navigator.clipboard.writeText(url);
    toast.success("Share URL copied");
  }

  return (
    <PageContainer>
      <header className="mb-6 flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Compare runs</h1>
          <p className="text-sm text-muted-foreground">
            Overlay loss curves and diff hyperparameters across up to {MAX} concurrent runs.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="sm" disabled={selected.length >= MAX}>
                <Plus /> Add run
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="max-h-80 overflow-auto">
              <DropdownMenuLabel>Select up to {MAX}</DropdownMenuLabel>
              {(all.data?.runs ?? []).map((r) => {
                const checked = selected.includes(r.id);
                return (
                  <DropdownMenuItem
                    key={r.id}
                    onSelect={(e) => {
                      e.preventDefault();
                      setSelected((s) =>
                        checked ? s.filter((x) => x !== r.id) : s.length >= MAX ? s : [...s, r.id],
                      );
                    }}
                    className="flex items-center justify-between gap-3"
                  >
                    <span className="truncate">{r.name}</span>
                    {checked && <Check className="size-3.5 text-primary" />}
                  </DropdownMenuItem>
                );
              })}
            </DropdownMenuContent>
          </DropdownMenu>
          <Button variant="outline" size="sm" onClick={shareUrl} disabled={!selected.length}>
            <Share2 /> Share
          </Button>
        </div>
      </header>

      {selected.length === 0 ? (
        <Card>
          <CardContent className="grid place-items-center gap-2 py-20 text-center">
            <p className="text-sm font-medium">Pick runs to compare</p>
            <p className="max-w-md text-xs text-muted-foreground">
              Add up to {MAX} runs from the menu above. The URL will update so you can share the
              exact comparison.
            </p>
          </CardContent>
        </Card>
      ) : (
        <>
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Loss overlay</CardTitle>
            </CardHeader>
            <CardContent>
              {!hasAnyMetrics ? (
                <Skeleton className="h-80 w-full" />
              ) : (
                <RunOverlayChart series={series} height={360} />
              )}
            </CardContent>
          </Card>

          <Card className="mt-6">
            <CardHeader>
              <CardTitle className="text-sm">Hyperparameter diff</CardTitle>
            </CardHeader>
            <CardContent className="px-0 pb-2">
              <HyperparamDiff
                runs={runs}
                colors={PALETTE}
                onRemove={(id) => setSelected((s) => s.filter((x) => x !== id))}
              />
            </CardContent>
          </Card>
        </>
      )}
    </PageContainer>
  );
}
