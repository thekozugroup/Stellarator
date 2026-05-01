"use client";

import Link from "next/link";
import { CheckCircle2, XCircle, ExternalLink } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { fmtUsd } from "@/lib/format";
import type { PreflightJson } from "@/lib/types";
import { SandboxLineage } from "@/components/sandbox-lineage";

interface PreflightCardProps {
  preflight: PreflightJson;
  parentRunId?: string | null;
}

function KV({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-3">
      <span className="text-[10px] uppercase tracking-wider text-muted-foreground shrink-0">
        {label}
      </span>
      <span className="text-xs font-medium text-foreground text-right">{value}</span>
    </div>
  );
}

export function PreflightCard({ preflight, parentRunId }: PreflightCardProps) {
  const validated = preflight.validated ?? (!preflight.errors || preflight.errors.length === 0);

  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between pb-2">
        <CardTitle className="text-sm">Pre-flight</CardTitle>
        {validated ? (
          <div className="flex items-center gap-1 text-[11px] text-success font-medium">
            <CheckCircle2 className="size-3.5" />
            Validated
          </div>
        ) : (
          <div className="flex items-center gap-1 text-[11px] text-destructive font-medium">
            <XCircle className="size-3.5" />
            Errors
          </div>
        )}
      </CardHeader>
      <CardContent className="space-y-3 text-xs">
        {preflight.model ? (
          <KV label="Model" value={<code className="font-mono">{preflight.model}</code>} />
        ) : null}
        {preflight.method ? (
          <KV label="Method" value={<code className="font-mono">{preflight.method}</code>} />
        ) : null}
        {preflight.projected_cost_usd != null ? (
          <KV label="Projected cost" value={fmtUsd(preflight.projected_cost_usd)} />
        ) : null}

        {preflight.datasets && preflight.datasets.length > 0 ? (
          <div className="space-y-1">
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
              Datasets
            </div>
            {preflight.datasets.map((d) => (
              <div key={d.name} className="flex items-center justify-between">
                <span className="font-mono text-foreground/80">{d.name}</span>
                {d.weight != null ? (
                  <span className="text-muted-foreground">{d.weight}</span>
                ) : null}
              </div>
            ))}
          </div>
        ) : null}

        {preflight.hyperparam_diff && Object.keys(preflight.hyperparam_diff).length > 0 ? (
          <div className="space-y-1">
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
              Hyperparam diff vs sandbox
            </div>
            {Object.entries(preflight.hyperparam_diff).map(([k, v]) => (
              <div key={k} className="flex items-center justify-between gap-2">
                <span className="font-mono text-foreground/80">{k}</span>
                <span className="font-mono text-foreground">{String(v)}</span>
              </div>
            ))}
          </div>
        ) : null}

        {preflight.citations && preflight.citations.length > 0 ? (
          <div className="space-y-1">
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
              Citations ({preflight.citations.length})
            </div>
            {preflight.citations.slice(0, 4).map((c) => (
              <div
                key={c}
                className="flex items-center gap-1.5 rounded border border-border/50 px-2 py-1"
              >
                <ExternalLink className="size-3 shrink-0 text-muted-foreground" />
                <span className="truncate font-mono text-[11px]">{c}</span>
              </div>
            ))}
            {preflight.citations.length > 4 ? (
              <div className="text-[10px] text-muted-foreground">
                + {preflight.citations.length - 4} more
              </div>
            ) : null}
          </div>
        ) : null}

        {preflight.errors && preflight.errors.length > 0 ? (
          <div className="space-y-1 rounded border border-destructive/30 bg-destructive/5 p-2">
            <div className="text-[10px] uppercase tracking-wider text-destructive">Errors</div>
            {preflight.errors.map((e, i) => (
              <p key={i} className="text-xs text-destructive/90">
                {e}
              </p>
            ))}
          </div>
        ) : null}

        {parentRunId ? (
          <div className="pt-1 border-t border-border/40">
            <SandboxLineage isSandbox={false} parentRunId={parentRunId} />
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}
