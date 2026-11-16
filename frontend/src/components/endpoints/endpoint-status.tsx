import type { Endpoint } from "@/lib/types"
import { count, DASH, relative } from "@/lib/format"

/** The server answered but refused the key (the backend words it "The server rejected the API key (401)"). */
export const isKeyRejected = (error: string | null | undefined) => !!error && /^the server rejected the api key/i.test(error)

/** "N models" / "Key rejected" / "Unreachable" + the reason / "Not checked". Red only on the failing fact. */
export function EndpointStatus({ endpoint, detail = true }: { endpoint: Endpoint; detail?: boolean }) {
  if (endpoint.last_ok === null) {
    return <span className="text-muted-foreground">Not checked</span>
  }
  if (endpoint.last_ok) {
    return <span className="tabular">{count(endpoint.models.length, "model", "models")}</span>
  }
  return (
    <span className="grid gap-0.5">
      <span className="font-medium text-destructive">{isKeyRejected(endpoint.last_error) ? "Key rejected" : "Unreachable"}</span>
      {detail && endpoint.last_error ? (
        <span title={endpoint.last_error} className="line-clamp-3 max-w-[34ch] text-xs text-muted-foreground">
          {endpoint.last_error}
        </span>
      ) : null}
    </span>
  )
}

export function CheckedAt({ endpoint }: { endpoint: Endpoint }) {
  if (!endpoint.last_checked_at) return <span className="text-muted-foreground">{DASH}</span>
  return <span title={new Date(endpoint.last_checked_at).toLocaleString()}>{relative(endpoint.last_checked_at)}</span>
}
