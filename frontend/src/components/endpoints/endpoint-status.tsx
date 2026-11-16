import type { Endpoint } from "@/lib/types"
import { count, DASH, relative } from "@/lib/format"

/** Reachable · 7 models / Unreachable + the reason / Not checked. Red only on the failing fact. */
export function EndpointStatus({ endpoint }: { endpoint: Endpoint }) {
  if (endpoint.last_ok === null) {
    return <span className="text-muted-foreground">Not checked</span>
  }
  if (endpoint.last_ok) {
    return (
      <span>
        Reachable<span className="text-muted-foreground"> · {count(endpoint.models.length, "model", "models")}</span>
      </span>
    )
  }
  return (
    <span className="grid gap-0.5">
      <span className="font-medium text-destructive">Unreachable</span>
      {endpoint.last_error ? <span className="max-w-[34ch] text-xs text-muted-foreground">{endpoint.last_error}</span> : null}
    </span>
  )
}

export function CheckedAt({ endpoint }: { endpoint: Endpoint }) {
  if (!endpoint.last_checked_at) return <span className="text-muted-foreground">{DASH}</span>
  return <span title={new Date(endpoint.last_checked_at).toLocaleString()}>{relative(endpoint.last_checked_at)}</span>
}

export const locationLabel = (e: Pick<Endpoint, "location">) => (e.location === "cloud" ? "Cloud" : "Local")
