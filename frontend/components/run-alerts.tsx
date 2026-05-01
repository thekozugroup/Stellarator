"use client";

import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertCircle, AlertTriangle, ChevronDown, ChevronRight, Info } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { api } from "@/lib/api";
import { markAlertsSeen } from "@/lib/notifications";
import type { RunAlert, RunStatus } from "@/lib/types";

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
    label: "Warning",
  },
  info: {
    icon: Info,
    badgeVariant: "secondary" as const,
    border: "border-primary/20 bg-primary/5",
    iconClass: "text-primary",
    label: "Info",
  },
} as const;

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

function AlertCard({ alert }: { alert: RunAlert }) {
  const [expanded, setExpanded] = useState(false);
  const cfg = LEVEL_CONFIG[alert.level];
  const Icon = cfg.icon;

  return (
    <div className={cn("rounded-lg border px-3.5 py-3 text-sm", cfg.border)}>
      <div className="flex items-start gap-2.5">
        <Icon className={cn("mt-0.5 size-4 shrink-0", cfg.iconClass)} aria-hidden />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant={cfg.badgeVariant} className="text-[10px] uppercase tracking-wide">
              {cfg.label}
            </Badge>
            <span className="font-medium text-foreground">{alert.title}</span>
            {alert.source ? (
              <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground font-mono">
                {alert.source}
              </span>
            ) : null}
            <span className="ml-auto text-[10px] text-muted-foreground tabular-nums">
              {relativeTime(alert.created_at)}
            </span>
          </div>
          {alert.body ? (
            <div className="mt-1.5">
              <button
                type="button"
                onClick={() => setExpanded((v) => !v)}
                className="flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring outline-none rounded"
                aria-expanded={expanded}
              >
                {expanded ? (
                  <ChevronDown className="size-3" />
                ) : (
                  <ChevronRight className="size-3" />
                )}
                {expanded ? "Hide details" : "Show details"}
              </button>
              {expanded ? (
                <p className="mt-1.5 text-xs text-foreground/80 leading-relaxed whitespace-pre-wrap">
                  {alert.body}
                </p>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}

export function RunAlerts({
  runId,
  runStatus,
}: {
  runId: string;
  runStatus: RunStatus;
}) {
  const isLive = runStatus === "running" || runStatus === "queued" || runStatus === "provisioning";

  const { data, isLoading } = useQuery({
    queryKey: ["run-alerts", runId],
    queryFn: () => api.getRunAlerts(runId),
    refetchInterval: isLive ? 5_000 : false,
  });

  const alerts = data?.alerts ?? [];

  // Clear unread badge whenever alerts surface here.
  useEffect(() => {
    markAlertsSeen();
  }, [alerts.length]);

  if (isLoading) {
    return (
      <div className="space-y-2">
        {[1, 2, 3].map((i) => (
          <div key={i} className="h-14 animate-pulse rounded-lg border border-border/40 bg-muted/30" />
        ))}
      </div>
    );
  }

  if (alerts.length === 0) {
    return (
      <div className="rounded-lg border border-dashed border-border/50 p-10 text-center text-xs text-muted-foreground">
        No alerts yet
        {isLive ? " — polling every 5 s" : "."}
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {alerts.map((alert, i) => (
        <AlertCard key={`${alert.created_at}-${i}`} alert={alert} />
      ))}
    </div>
  );
}
