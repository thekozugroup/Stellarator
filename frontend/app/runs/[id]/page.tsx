"use client";

import { useMemo } from "react";
import { useParams, useRouter, useSearchParams } from "next/navigation";
import { useIsOwner } from "@/lib/use-whoami";
import { useQuery } from "@tanstack/react-query";
import { ChevronDown, ChevronRight, Pin, PinOff } from "lucide-react";
import Link from "next/link";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { PageContainer } from "@/components/ui/page-container";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { usePrefs } from "@/lib/local-prefs";
import type { Run } from "@/lib/types";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { AgentBadge } from "@/components/agent-badge";
import { CitationsList } from "@/components/citations-list";
import { LiveMetricsChart } from "@/components/live-metrics-chart";
import { NotesTimeline } from "@/components/notes-timeline";
import { PreflightCard } from "@/components/preflight-card";
import { RunActions } from "@/components/run-actions";
import { RunAlerts } from "@/components/run-alerts";
import { RunStatusBadge } from "@/components/run-status-badge";
import { SandboxLineage } from "@/components/sandbox-lineage";
import { api } from "@/lib/api";
import { fmtUsd } from "@/lib/format";
import type { RunMetric, RunNote } from "@/lib/types";
import { useRunStream } from "@/lib/ws";

export default function RunDetailPage() {
  const { id } = useParams<{ id: string }>();
  const searchParams = useSearchParams();
  const initialTab = (searchParams.get("tab") as "metrics" | "notes" | "alerts" | "logs" | null) ?? "metrics";
  const run = useQuery({
    queryKey: ["run", id],
    queryFn: () => api.getRun(id),
    refetchInterval: 8_000,
  });
  const initialMetrics = useQuery({
    queryKey: ["run-metrics", id],
    queryFn: () => api.getRunMetrics(id),
  });
  const initialNotes = useQuery({
    queryKey: ["run-notes", id],
    queryFn: () => api.getRunNotes(id),
  });

  const { status, events } = useRunStream(id);

  const metrics = useMemo<RunMetric[]>(() => {
    const seed = initialMetrics.data?.metrics ?? [];
    const live = events.filter((e) => e.type === "metric").map((e) => e.data as RunMetric);
    const seen = new Set<number>();
    return [...seed, ...live].filter((m) => {
      if (seen.has(m.step)) return false;
      seen.add(m.step);
      return true;
    });
  }, [initialMetrics.data, events]);

  const notes = useMemo<RunNote[]>(() => {
    const seed = initialNotes.data?.notes ?? [];
    const live = events.filter((e) => e.type === "note").map((e) => e.data as RunNote);
    const seen = new Set<string>();
    return [...seed, ...live].filter((n) => {
      if (seen.has(n.id)) return false;
      seen.add(n.id);
      return true;
    });
  }, [initialNotes.data, events]);

  if (run.isLoading) return <DetailSkeleton />;
  if (!run.data)
    return (
      <div className="p-8 text-sm text-muted-foreground">
        Run not found. <Link href="/" className="text-primary underline">Back to dashboard</Link>
      </div>
    );

  const r = run.data;
  const isOwner = useIsOwner(r);

  return (
    <PageContainer>
      <RunHeaderNav current={r} />

      <header className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <div className="flex flex-wrap items-center gap-3">
            <h1 className="text-2xl font-semibold tracking-tight">{r.name}</h1>
            <RunStatusBadge status={r.status} />
            <SandboxLineage isSandbox={r.is_sandbox} parentRunId={r.parent_run_id} />
            <PinToggle id={r.id} name={r.name} />
          </div>
          <div className="mt-1.5 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <AgentBadge agent={r.owner_agent} />
            <span>·</span>
            <span className="font-mono">{r.method}</span>
            <span>·</span>
            <span className="font-mono">{r.gpu}</span>
            <span>·</span>
            <span className="font-mono tabular-nums">{fmtUsd(r.cost_so_far_usd)}</span>
            {r.reward_mean != null && (
              <>
                <span>·</span>
                <span className="font-mono tabular-nums text-chart-5">
                  Reward {r.reward_mean.toFixed(3)}
                </span>
              </>
            )}
            {r.percent_correct != null && (
              <>
                <span>·</span>
                <span className="font-mono tabular-nums text-chart-4">
                  Pass {r.percent_correct.toFixed(1)}%
                </span>
              </>
            )}
            <span>·</span>
            <span
              className={
                status === "open"
                  ? "text-success"
                  : status === "connecting"
                    ? "text-warning"
                    : "text-muted-foreground"
              }
            >
              ws: {status}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {r.status === "succeeded" && r.checkpoint_url ? (
            <a
              href={r.checkpoint_url}
              download
              className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-background px-3 text-xs text-foreground hover:bg-accent transition-colors"
            >
              Download checkpoint ↓
            </a>
          ) : null}
          <RunActions run={r} onChange={() => run.refetch()} />
        </div>
      </header>

      <Separator className="my-6" />

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-[380px_1fr]">
        <aside className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Configuration</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3 text-xs">
              <KV label="Base model" value={<code className="font-mono text-foreground">{r.base_model}</code>} />
              <KV label="Method" value={<code className="font-mono text-foreground">{r.method}</code>} />
              <KV label="GPU" value={<code className="font-mono text-foreground">{r.gpu}</code>} />
              <Separator />
              <div>
                <div className="mb-1.5 text-micro uppercase tracking-wider text-muted-foreground">
                  Hyperparameters
                </div>
                <pre className="max-h-56 overflow-auto rounded-md border bg-background/40 p-2 font-mono text-[11px] leading-relaxed">
{JSON.stringify(r.hyperparams, null, 2)}
                </pre>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Dataset mixture</CardTitle>
            </CardHeader>
            <CardContent className="px-0 pb-2">
              {r.dataset_mixture.length === 0 ? (
                <p className="px-6 text-xs text-muted-foreground">No mixture configured.</p>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead className="pl-6">Source</TableHead>
                      <TableHead className="text-right">Weight</TableHead>
                      <TableHead className="pr-6 text-right">Rows</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {r.dataset_mixture.map((d) => (
                      <TableRow key={d.name}>
                        <TableCell className="pl-6 font-mono text-xs">{d.name}</TableCell>
                        <TableCell className="text-right tabular-nums">
                          {(d.weight * 100).toFixed(1)}%
                        </TableCell>
                        <TableCell className="pr-6 text-right text-muted-foreground">
                          {d.rows ? d.rows.toLocaleString() : "—"}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Goal &amp; plan</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3 text-xs">
              <div>
                <div className="mb-1 text-micro uppercase tracking-wider text-muted-foreground">
                  User goal
                </div>
                <p className="leading-relaxed text-foreground">
                  {r.user_goal ?? <span className="text-muted-foreground">No goal recorded.</span>}
                </p>
              </div>
              <Separator />
              <div>
                <div className="mb-1 text-micro uppercase tracking-wider text-muted-foreground">
                  Agent plan
                </div>
                <p className="whitespace-pre-wrap leading-relaxed text-foreground">
                  {r.agent_plan ?? (
                    <span className="text-muted-foreground">No plan attached.</span>
                  )}
                </p>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Citations</CardTitle>
            </CardHeader>
            <CardContent>
              <CitationsList citations={r.citations} />
            </CardContent>
          </Card>

          {r.preflight_json ? (
            <PreflightCard preflight={r.preflight_json} parentRunId={r.parent_run_id} />
          ) : null}
        </aside>

        <section>
          <Tabs defaultValue={initialTab}>
            <TabsList>
              <TabsTrigger value="metrics">Metrics</TabsTrigger>
              <TabsTrigger value="notes">Notes</TabsTrigger>
              <TabsTrigger value="alerts">Alerts</TabsTrigger>
              <TabsTrigger value="logs">Logs</TabsTrigger>
            </TabsList>

            <TabsContent value="metrics">
              <Card>
                <CardHeader className="flex-row items-center justify-between">
                  <div>
                    <CardTitle className="text-sm">Training loss</CardTitle>
                    <p className="text-xs text-muted-foreground">
                      Solid: train · dashed: eval. {metrics.length} datapoints.
                    </p>
                  </div>
                  <div className="flex gap-2">
                    <LegendDot color="var(--color-chart-1)" label="loss" />
                    <LegendDot color="var(--color-chart-2)" label="eval_loss" dashed />
                  </div>
                </CardHeader>
                <CardContent>
                  <LiveMetricsChart metrics={metrics} />
                </CardContent>
              </Card>
            </TabsContent>

            <TabsContent value="notes">
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Append-only timeline</CardTitle>
                </CardHeader>
                <CardContent>
                  <NotesTimeline notes={notes} />
                </CardContent>
              </Card>
            </TabsContent>

            <TabsContent value="alerts">
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Training alerts</CardTitle>
                  <p className="text-xs text-muted-foreground">
                    Emitted by training scripts and the agent loop.
                  </p>
                </CardHeader>
                <CardContent>
                  <RunAlerts runId={r.id} runStatus={r.status} />
                </CardContent>
              </Card>
            </TabsContent>

            <TabsContent value="logs">
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Worker logs</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="rounded-md border border-dashed p-10 text-center text-xs text-muted-foreground">
                    Log streaming endpoint not yet wired.
                  </div>
                </CardContent>
              </Card>
            </TabsContent>
          </Tabs>
        </section>
      </div>
    </PageContainer>
  );
}

function KV({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-micro uppercase tracking-wider text-muted-foreground">{label}</span>
      <span className="text-right text-xs">{value}</span>
    </div>
  );
}

function LegendDot({ color, label, dashed }: { color: string; label: string; dashed?: boolean }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-[11px] text-muted-foreground">
      <span
        className="inline-block h-0.5 w-4"
        style={{
          background: dashed
            ? `repeating-linear-gradient(90deg, ${color} 0 4px, transparent 4px 7px)`
            : color,
        }}
      />
      <span className="font-mono">{label}</span>
    </span>
  );
}

function DetailSkeleton() {
  return (
    <PageContainer>
      <Skeleton className="h-8 w-72" />
      <Skeleton className="h-4 w-48" />
      <div className="grid grid-cols-[380px_1fr] gap-6 pt-4">
        <Skeleton className="h-96" />
        <Skeleton className="h-96" />
      </div>
    </PageContainer>
  );
}


function RunHeaderNav({ current }: { current: Run }): React.ReactElement {
  const router = useRouter();
  const allQuery = useQuery({
    queryKey: ["runs"],
    queryFn: () => api.listRuns({ limit: 50 }),
    staleTime: 10_000,
  });
  const others: Run[] = (allQuery.data?.runs ?? []).filter((r) => r.id !== current.id);

  return (
    <nav
      aria-label="Breadcrumb"
      className="mb-4 flex items-center gap-1.5 text-xs text-muted-foreground"
    >
      <Link
        href="/"
        className="rounded px-1 py-0.5 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
      >
        Runs
      </Link>
      <ChevronRight aria-hidden className="size-3 text-muted-foreground/60" />
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className="inline-flex items-center gap-1 rounded px-1 py-0.5 text-foreground hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring"
            aria-label="Switch to another run"
          >
            <span className="max-w-[28ch] truncate">{current.name}</span>
            <ChevronDown className="size-3 text-muted-foreground" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="max-h-[60vh] w-72 overflow-y-auto">
          <DropdownMenuLabel>Switch run</DropdownMenuLabel>
          <DropdownMenuSeparator />
          {others.length === 0 ? (
            <DropdownMenuItem disabled>No other runs</DropdownMenuItem>
          ) : (
            others.slice(0, 30).map((r) => (
              <DropdownMenuItem
                key={r.id}
                onSelect={() => router.push(`/runs/${r.id}`)}
                className="flex flex-col items-start gap-0.5 py-1.5"
              >
                <span className="w-full truncate text-xs">{r.name}</span>
                <span className="w-full truncate font-mono text-micro text-muted-foreground">
                  {r.base_model} · {r.method}
                </span>
              </DropdownMenuItem>
            ))
          )}
        </DropdownMenuContent>
      </DropdownMenu>
    </nav>
  );
}

function PinToggle({ id, name }: { id: string; name: string }): React.ReactElement {
  const { isPinned, togglePin } = usePrefs();
  const pinned = isPinned(id);
  return (
    <Button
      type="button"
      variant="ghost"
      size="sm"
      aria-pressed={pinned}
      aria-label={pinned ? `Unpin ${name}` : `Pin ${name}`}
      onClick={() => togglePin(id)}
      className="h-7 gap-1.5 px-2 text-xs text-muted-foreground hover:text-warning"
    >
      {pinned ? (
        <>
          <PinOff className="size-3.5" /> Unpin
        </>
      ) : (
        <>
          <Pin className="size-3.5" /> Pin
        </>
      )}
    </Button>
  );
}
