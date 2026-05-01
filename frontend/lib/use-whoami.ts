"use client";

import { useQuery } from "@tanstack/react-query";
import { z } from "zod";
import type { Run } from "@/lib/types";

const API_URL =
  (typeof process !== "undefined" &&
    process.env.NEXT_PUBLIC_API_URL?.replace(/\/$/, "")) ||
  "http://localhost:8000";

const WhoamiSchema = z.object({ agent: z.string() });
export type Whoami = z.infer<typeof WhoamiSchema>;

async function fetchWhoami(): Promise<Whoami> {
  const token =
    typeof window !== "undefined"
      ? window.localStorage.getItem("stellarator.viewerToken") ?? undefined
      : undefined;

  const headers = new Headers({ Accept: "application/json" });
  if (token) headers.set("Authorization", `Bearer ${token}`);

  const res = await fetch(`${API_URL}/v1/whoami`, { headers, cache: "no-store" });
  if (!res.ok) {
    // Graceful fallback — treat as anonymous when endpoint is absent
    return { agent: "anonymous" };
  }
  const json = await res.json();
  return WhoamiSchema.parse(json);
}

/**
 * Fetches the currently-authenticated agent identity once per session.
 * Cached forever (staleTime: Infinity) so it is never re-fetched.
 */
export function useWhoami(): { agent: string; isLoading: boolean } {
  const { data, isLoading } = useQuery({
    queryKey: ["whoami"],
    queryFn: fetchWhoami,
    staleTime: Infinity,
    retry: false,
  });
  return { agent: data?.agent ?? "anonymous", isLoading };
}

/**
 * Returns true when the current agent is the owner of the given run.
 * Import this hook wherever ownership gating is needed (e.g. runs-table, run-actions).
 *
 * Usage:
 *   import { useIsOwner } from "@/lib/use-whoami";
 *   const isOwner = useIsOwner(run);
 */
export function useIsOwner(run: Pick<Run, "owner_agent">): boolean {
  const { agent } = useWhoami();
  return agent === run.owner_agent;
}
