"use client";

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { BookText, ChevronRight, ExternalLink } from "lucide-react";
import Link from "next/link";

import { Badge } from "@/components/ui/badge";
import { PageContainer } from "@/components/ui/page-container";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { api } from "@/lib/api";
import type { ResearchTranscript } from "@/lib/types";

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const s = Math.floor(diff / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return new Date(iso).toLocaleDateString();
}

function TranscriptDrawer({
  transcript,
  onClose,
}: {
  transcript: ResearchTranscript | null;
  onClose: () => void;
}) {
  return (
    <Dialog open={!!transcript} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
        {transcript ? (
          <>
            <DialogHeader>
              <DialogTitle className="text-sm font-semibold leading-snug pr-6">
                {transcript.task}
              </DialogTitle>
            </DialogHeader>
            <div className="space-y-4 text-xs">
              <div className="flex flex-wrap gap-3 text-muted-foreground">
                <span>
                  Agent: <span className="font-mono text-foreground">{transcript.agent}</span>
                </span>
                <span>{relativeTime(transcript.started_at)}</span>
                {transcript.run_id ? (
                  <Link
                    href={`/runs/${transcript.run_id}`}
                    className="flex items-center gap-1 text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring outline-none rounded"
                  >
                    <ExternalLink className="size-3" />
                    Run {transcript.run_id.slice(0, 8)}
                  </Link>
                ) : null}
              </div>

              {transcript.context ? (
                <div>
                  <div className="mb-1 text-[10px] uppercase tracking-wider text-muted-foreground">
                    Context
                  </div>
                  <p className="text-foreground/80 leading-relaxed whitespace-pre-wrap">
                    {transcript.context}
                  </p>
                </div>
              ) : null}

              {transcript.result_summary ? (
                <div>
                  <div className="mb-1 text-[10px] uppercase tracking-wider text-muted-foreground">
                    Summary
                  </div>
                  <p className="text-foreground/80 leading-relaxed whitespace-pre-wrap">
                    {transcript.result_summary}
                  </p>
                </div>
              ) : null}

              {transcript.result_json !== undefined ? (
                <div>
                  <div className="mb-1 text-[10px] uppercase tracking-wider text-muted-foreground">
                    Full result JSON
                  </div>
                  <pre className="overflow-x-auto rounded border border-border/50 bg-muted/40 p-3 text-[11px] leading-relaxed text-foreground/90">
                    {JSON.stringify(transcript.result_json, null, 2)}
                  </pre>
                </div>
              ) : null}
            </div>
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

export default function AgentResearchPage() {
  const [selected, setSelected] = useState<ResearchTranscript | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ["research-transcripts"],
    queryFn: () => api.getResearchTranscripts({ limit: 100 }),
    refetchInterval: 30_000,
  });

  const transcripts = data?.transcripts ?? [];

  return (
    <PageContainer className="max-w-[1100px]">
      <header className="mb-6">
        <div className="flex items-center gap-2.5">
          <div className="grid size-8 shrink-0 place-items-center rounded-lg bg-primary/10 text-primary">
            <BookText className="size-4" />
          </div>
          <div>
            <h1 className="text-xl font-semibold tracking-tight">Agent research log</h1>
            <p className="text-xs text-muted-foreground">
              What your agents looked up automatically — read-only audit.
            </p>
          </div>
        </div>
      </header>

      {isLoading ? (
        <div className="space-y-2">
          {Array.from({ length: 6 }).map((_, i) => (
            <Skeleton key={i} className="h-12 w-full" />
          ))}
        </div>
      ) : transcripts.length === 0 ? (
        <Card>
          <CardContent className="grid place-items-center gap-3 py-16 text-center">
            <BookText className="size-8 text-muted-foreground/50" />
            <p className="text-sm font-medium">No research calls yet</p>
            <p className="max-w-md text-xs text-muted-foreground">
              When your agents invoke the{" "}
              <code className="font-mono">research()</code> sub-agent, each
              call appears here automatically.
            </p>
          </CardContent>
        </Card>
      ) : (
        <div className="overflow-hidden rounded-lg border border-border/60 bg-card">
          <Table>
            <TableHeader>
              <TableRow className="border-border/50 hover:bg-transparent">
                <TableHead className="w-36">When</TableHead>
                <TableHead className="w-32">Agent</TableHead>
                <TableHead>Task</TableHead>
                <TableHead className="w-48">Summary</TableHead>
                <TableHead className="w-16 text-right">Citations</TableHead>
                <TableHead className="w-28">Run</TableHead>
                <TableHead className="w-8" />
              </TableRow>
            </TableHeader>
            <TableBody>
              {transcripts.map((t, i) => (
                <TableRow
                  key={t.id ?? i}
                  tabIndex={0}
                  onClick={() => setSelected(t)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      setSelected(t);
                    }
                  }}
                  className="cursor-pointer border-border/40 hover:bg-accent/40 focus-visible:bg-accent/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
                  aria-label={`View research call: ${t.task}`}
                >
                  <TableCell className="tabular-nums text-xs text-muted-foreground">
                    {relativeTime(t.started_at)}
                  </TableCell>
                  <TableCell>
                    <Badge
                      variant="secondary"
                      className="font-mono text-[10px] max-w-[8rem] truncate"
                    >
                      {t.agent}
                    </Badge>
                  </TableCell>
                  <TableCell className="max-w-[24rem]">
                    <span className="line-clamp-2 text-sm font-medium text-foreground leading-snug">
                      {t.task}
                    </span>
                  </TableCell>
                  <TableCell className="max-w-[12rem]">
                    {t.result_summary ? (
                      <span className="line-clamp-2 text-xs text-muted-foreground leading-snug">
                        {t.result_summary}
                      </span>
                    ) : (
                      <span className="text-xs text-muted-foreground/50">—</span>
                    )}
                  </TableCell>
                  <TableCell className="text-right font-mono text-xs text-muted-foreground tabular-nums">
                    {t.citations_count ?? "—"}
                  </TableCell>
                  <TableCell>
                    {t.run_id ? (
                      <Link
                        href={`/runs/${t.run_id}`}
                        onClick={(e) => e.stopPropagation()}
                        className="inline-flex items-center gap-1 font-mono text-[11px] text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring outline-none rounded"
                      >
                        <ExternalLink className="size-3" />
                        {t.run_id.slice(0, 8)}
                      </Link>
                    ) : (
                      <span className="text-xs text-muted-foreground/50">—</span>
                    )}
                  </TableCell>
                  <TableCell>
                    <ChevronRight className="size-3.5 text-muted-foreground/50" />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      <TranscriptDrawer transcript={selected} onClose={() => setSelected(null)} />
    </PageContainer>
  );
}
