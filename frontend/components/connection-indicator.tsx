"use client";

import { useQuery } from "@tanstack/react-query";
import { useRef } from "react";
import { cn } from "@/lib/utils";

const API_URL =
  (typeof process !== "undefined" &&
    process.env.NEXT_PUBLIC_API_URL?.replace(/\/$/, "")) ||
  "http://localhost:8000";

const POLL_INTERVAL_MS = 10_000;
const FAILURE_THRESHOLD = 2;
const RECONNECTING_THRESHOLD_MS = 12_000;

async function fetchHealthz(): Promise<{ ok: true; ts: number }> {
  const res = await fetch(`${API_URL}/healthz`, { cache: "no-store" });
  if (!res.ok) throw new Error(`healthz ${res.status}`);
  return { ok: true, ts: Date.now() };
}

type Status = "live" | "reconnecting" | "disconnected";

export function useConnectionStatus(): Status {
  const consecutiveFailures = useRef(0);
  const lastSuccessTs = useRef<number>(Date.now());

  const { isError, dataUpdatedAt } = useQuery({
    queryKey: ["healthz"],
    queryFn: fetchHealthz,
    refetchInterval: POLL_INTERVAL_MS,
    retry: false,
    staleTime: POLL_INTERVAL_MS,
  });

  if (!isError && dataUpdatedAt) {
    consecutiveFailures.current = 0;
    lastSuccessTs.current = dataUpdatedAt;
  } else if (isError) {
    consecutiveFailures.current += 1;
  }

  if (consecutiveFailures.current >= FAILURE_THRESHOLD) return "disconnected";
  if (Date.now() - lastSuccessTs.current > RECONNECTING_THRESHOLD_MS)
    return "reconnecting";
  return "live";
}

const STATUS_CONFIG: Record<
  Status,
  { label: string; dotClass: string; pillClass: string }
> = {
  live: {
    label: "Live",
    dotClass: "bg-success animate-pulse",
    pillClass: "bg-success/15 text-success ring-success/30",
  },
  reconnecting: {
    label: "Reconnecting",
    dotClass: "bg-warning animate-pulse",
    pillClass: "bg-warning/15 text-warning ring-warning/30",
  },
  disconnected: {
    label: "Disconnected",
    dotClass: "bg-destructive",
    pillClass: "bg-destructive/15 text-destructive ring-destructive/30",
  },
};

/**
 * ConnectionIndicator — mount in the sidebar footer.
 * Polls /healthz every 10 s; shows Live / Reconnecting / Disconnected.
 * Never renders a static green dot — state is always dynamic.
 */
export function ConnectionIndicator({ className }: { className?: string }) {
  const status = useConnectionStatus();
  const { label, dotClass, pillClass } = STATUS_CONFIG[status];

  return (
    <div
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-medium ring-1",
        pillClass,
        className,
      )}
      role="status"
      aria-live="polite"
      aria-label={`Connection status: ${label}`}
    >
      <span className={cn("size-1.5 rounded-full shrink-0", dotClass)} aria-hidden />
      {label}
    </div>
  );
}
