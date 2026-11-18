import type { Endpoint } from "@/lib/types"
import { count, DASH, relative } from "@/lib/format"

/** The server answered but refused the key (the backend words it "The server rejected the API key (401)"). */
export const isKeyRejected = (error: string | null | undefined) => !!error && /^the server rejected the api key/i.test(error)

/** The server's own words after "The server rejected the API key (401)": ": User not found." becomes "401: User not found." */
export function serverReason(error: string | null | undefined) {
  if (!error) return null
  const m = error.match(/\((\d{3})\)\s*[:.\-–]?\s*(.*)$/s)
  if (!m) return null
  const body = m[2].trim()
  return body ? `${m[1]}: ${body}` : m[1]
}

/** "N models" / "Key rejected" / "Unreachable" + the reason / "Not checked". Red only on the failing fact. */
export function EndpointStatus({ endpoint, detail = true }: { endpoint: Endpoint; detail?: boolean }) {
  if (endpoint.last_ok === null) {
    return <span className="text-muted-foreground">Not checked</span>
  }
  if (endpoint.last_ok) {
    return <span className="tabular">{count(endpoint.models.length, "model", "models")}</span>
  }
  const rejected = isKeyRejected(endpoint.last_error)
  const reason = rejected ? serverReason(endpoint.last_error) : endpoint.last_error
  const word = rejected ? "Key rejected" : "Unreachable"
  // Without the reason it stays inline, so it can sit in a "·" separated line (the phone rows).
  if (!detail || !reason) return <span className="font-medium text-destructive">{word}</span>
  return (
    <span className="grid gap-0.5">
      <span className="font-medium text-destructive">{word}</span>
      <span title={endpoint.last_error ?? undefined} className="line-clamp-3 max-w-[34ch] text-xs text-muted-foreground">
        {reason}
      </span>
    </span>
  )
}

export function CheckedAt({ endpoint }: { endpoint: Endpoint }) {
  if (!endpoint.last_checked_at) return <span className="text-muted-foreground">{DASH}</span>
  return <span title={new Date(endpoint.last_checked_at).toLocaleString()}>{relative(endpoint.last_checked_at)}</span>
}
