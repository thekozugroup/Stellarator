"use client";

// Mount point: app/layout.tsx (IA agent responsible for import + mount)
// Import: import { StaleDataBanner } from "@/components/stale-data-banner";
// Usage:  <StaleDataBanner lastSuccessMs={query.dataUpdatedAt} refetchIntervalMs={5000} onRefresh={query.refetch} />

import { RefreshCw } from "lucide-react";
import { useStaleDetector } from "@/lib/use-stale-detector";
import { cn } from "@/lib/utils";

export interface StaleDataBannerProps {
  /** Pass `query.dataUpdatedAt` from TanStack Query. */
  lastSuccessMs: number;
  /** The query's refetch interval in ms. */
  refetchIntervalMs: number;
  /** Called when the user clicks the manual refresh button. */
  onRefresh: () => void;
  className?: string;
}

/**
 * Sticky top banner shown when runs data is more than 2× refetchInterval old.
 * Hides itself when data is fresh.
 */
export function StaleDataBanner({
  lastSuccessMs,
  refetchIntervalMs,
  onRefresh,
  className,
}: StaleDataBannerProps) {
  const { isStale, secondsAgo } = useStaleDetector({ lastSuccessMs, refetchIntervalMs });

  if (!isStale) return null;

  return (
    <div
      role="alert"
      aria-live="assertive"
      className={cn(
        "sticky top-0 z-50 flex items-center justify-between gap-3",
        "bg-warning/10 border-b border-warning/30 px-4 py-2 text-sm text-warning",
        className,
      )}
    >
      <span>
        Showing data from{" "}
        <strong className="tabular-nums">{secondsAgo}</strong> seconds ago.{" "}
        Reconnecting&hellip;
      </span>
      <button
        type="button"
        onClick={onRefresh}
        className="inline-flex items-center gap-1.5 rounded px-2 py-0.5 text-xs font-medium ring-1 ring-warning/40 hover:bg-warning/20 transition-colors duration-[var(--duration-fast)] ease-[var(--ease-out)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[--ring]"
        aria-label="Refresh data now"
      >
        <RefreshCw className="size-3" aria-hidden />
        Refresh now
      </button>
    </div>
  );
}
