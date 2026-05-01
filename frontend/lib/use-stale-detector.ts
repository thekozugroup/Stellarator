"use client";

import { useEffect, useState } from "react";

export interface StaleDetectorOptions {
  /** Timestamp (ms) of the last successful data fetch. Pass `query.dataUpdatedAt`. */
  lastSuccessMs: number;
  /** The query's refetch interval in ms. Staleness triggers at 2× this value. */
  refetchIntervalMs: number;
}

export interface StaleState {
  isStale: boolean;
  /** Seconds since last successful fetch (integer, for display). */
  secondsAgo: number;
}

/**
 * Watches a query's last-success timestamp and returns whether data is stale.
 * Staleness is defined as: Date.now() - lastSuccessMs > 2 * refetchIntervalMs.
 * Re-evaluates every second while mounted.
 */
export function useStaleDetector({
  lastSuccessMs,
  refetchIntervalMs,
}: StaleDetectorOptions): StaleState {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1_000);
    return () => clearInterval(id);
  }, []);

  const elapsed = now - lastSuccessMs;
  const isStale = elapsed > 2 * refetchIntervalMs;
  const secondsAgo = Math.floor(elapsed / 1_000);

  return { isStale, secondsAgo };
}
